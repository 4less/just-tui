//! Asking Slurm what partitions, accounts and QoS exist.

use std::collections::BTreeMap;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use super::{Field, format_mem, format_time, parse_mem, parse_time};

/// How long any one Slurm query may take before it is given up on.
const QUERY_TIMEOUT: Duration = Duration::from_secs(4);

// ---------------------------------------------------------------------------
// What the cluster offers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Partition {
    pub name: String,
    pub is_default: bool,
    pub state: String,
    pub max_time: Option<u64>,
    pub default_time: Option<u64>,
    /// Memory on the largest node in the partition, in MB.
    pub max_mem_mb: Option<u64>,
    /// CPUs on the largest node in the partition.
    pub node_cpus: Option<u32>,
    pub total_nodes: Option<u32>,
    pub gres: Vec<String>,
    pub allow_accounts: Vec<String>,
    pub allow_qos: Vec<String>,
}

impl Partition {
    /// One-line summary of what this partition can do.
    pub fn limits(&self) -> String {
        let mut parts = Vec::new();
        if let Some(cpus) = self.node_cpus {
            parts.push(format!("{cpus} cpu/node"));
        }
        if let Some(mem) = self.max_mem_mb {
            parts.push(format!("{} /node", format_mem(mem)));
        }
        parts.push(match self.max_time {
            Some(minutes) => format!("max {}", format_time(minutes)),
            None => "no time limit".to_owned(),
        });
        if let Some(minutes) = self.default_time {
            parts.push(format!("default {}", format_time(minutes)));
        }
        if let Some(nodes) = self.total_nodes {
            parts.push(format!("{nodes} nodes"));
        }
        if !self.gres.is_empty() {
            parts.push(self.gres.join(","));
        }
        if !self.state.is_empty() && !self.state.eq_ignore_ascii_case("UP") {
            parts.push(self.state.to_lowercase());
        }
        parts.join(" · ")
    }
}

/// Everything auto-detected about the scheduler, or why nothing was.
#[derive(Debug, Clone, Default)]
pub struct Cluster {
    pub partitions: Vec<Partition>,
    pub accounts: Vec<String>,
    pub qos: Vec<String>,
    pub detected: bool,
    pub note: Option<String>,
}

impl Cluster {
    /// Ask Slurm what exists. Never fails: an undetectable cluster just means
    /// the form offers free text instead of pick lists.
    pub fn detect() -> Self {
        let mut cluster = Cluster::default();

        let Some(raw) = capture("scontrol", &["show", "partition", "--oneliner"]) else {
            cluster.note = Some(
                "no Slurm detected — fields are free text and submitting will likely fail".into(),
            );
            return cluster;
        };

        cluster.partitions = raw.lines().filter_map(parse_partition).collect();
        cluster.detected = !cluster.partitions.is_empty();

        // `scontrol` reports partition totals; `sinfo` reports per-node
        // figures, which is what a job is actually limited by.
        if let Some(info) = capture("sinfo", &["--noheader", "--format=%R|%c|%m|%G"]) {
            for line in info.lines() {
                let fields: Vec<&str> = line.split('|').map(str::trim).collect();
                let [name, cpus, mem, gres] = fields[..] else {
                    continue;
                };
                let Some(partition) = cluster
                    .partitions
                    .iter_mut()
                    .find(|p| p.name == name.trim_end_matches('*'))
                else {
                    continue;
                };
                if let Ok(cpus) = cpus.parse::<u32>() {
                    partition.node_cpus = Some(partition.node_cpus.unwrap_or(0).max(cpus));
                }
                if let Some(mb) = parse_mem(mem) {
                    partition.max_mem_mb = Some(partition.max_mem_mb.unwrap_or(0).max(mb));
                }
                if gres != "(null)"
                    && !gres.is_empty()
                    && !partition.gres.contains(&gres.to_owned())
                {
                    partition.gres.push(gres.to_owned());
                }
            }
        }

        cluster.accounts = detect_accounts();
        cluster.qos = detect_qos();

        // A partition may restrict which of them are usable.
        for partition in &cluster.partitions {
            for account in &partition.allow_accounts {
                if !cluster.accounts.contains(account) {
                    cluster.accounts.push(account.clone());
                }
            }
        }

        if !cluster.detected {
            cluster.note = Some("Slurm answered, but reported no partitions".into());
        }
        cluster
    }

    pub fn partition(&self, name: &str) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.name == name)
    }

    pub fn default_partition(&self) -> Option<&Partition> {
        self.partitions
            .iter()
            .find(|p| p.is_default)
            .or_else(|| self.partitions.first())
    }

    pub fn partition_names(&self) -> Vec<String> {
        self.partitions.iter().map(|p| p.name.clone()).collect()
    }

    /// The values a field can be picked from, empty when it is free text.
    pub fn choices(&self, field: Field) -> Vec<String> {
        match field {
            Field::Partition => self.partition_names(),
            Field::Account => self.accounts.clone(),
            Field::Qos => self.qos.clone(),
            _ => Vec::new(),
        }
    }
}

fn detect_accounts() -> Vec<String> {
    let user = std::env::var("USER").unwrap_or_default();
    if user.is_empty() {
        return Vec::new();
    }
    let raw = capture(
        "sacctmgr",
        &[
            "--noheader",
            "--parsable2",
            "show",
            "associations",
            &format!("user={user}"),
            "format=account",
        ],
    );
    dedup(raw.unwrap_or_default().lines().map(str::trim))
}

fn detect_qos() -> Vec<String> {
    let raw = capture(
        "sacctmgr",
        &["--noheader", "--parsable2", "show", "qos", "format=name"],
    );
    dedup(raw.unwrap_or_default().lines().map(str::trim))
}

fn dedup<'a>(items: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if !item.is_empty() && item != "(null)" && !out.iter().any(|x| x == item) {
            out.push(item.to_owned());
        }
    }
    out
}

/// `PartitionName=x Key=Value …` — one line per partition.
fn parse_partition(line: &str) -> Option<Partition> {
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for token in line.split_whitespace() {
        if let Some((key, value)) = token.split_once('=') {
            fields.insert(key, value);
        }
    }
    let name = fields.get("PartitionName")?;

    Some(Partition {
        name: (*name).to_owned(),
        is_default: fields.get("Default").is_some_and(|v| *v == "YES"),
        state: fields.get("State").unwrap_or(&"").to_string(),
        max_time: fields.get("MaxTime").and_then(|v| parse_time(v)),
        default_time: fields.get("DefaultTime").and_then(|v| parse_time(v)),
        max_mem_mb: fields.get("MaxMemPerNode").and_then(|v| parse_mem(v)),
        node_cpus: fields
            .get("MaxCPUsPerNode")
            .and_then(|v| v.parse().ok())
            .or_else(|| fields.get("TotalCPUs").and_then(|v| v.parse().ok())),
        total_nodes: fields.get("TotalNodes").and_then(|v| v.parse().ok()),
        gres: Vec::new(),
        allow_accounts: list_field(fields.get("AllowAccounts").copied()),
        allow_qos: list_field(fields.get("AllowQos").copied()),
    })
}

fn list_field(value: Option<&str>) -> Vec<String> {
    match value {
        Some(v) if v != "ALL" && v != "N/A" && !v.is_empty() => {
            v.split(',').map(str::to_owned).collect()
        }
        _ => Vec::new(),
    }
}

/// Run a command, giving up rather than hanging when the controller is down.
fn capture(program: &str, args: &[&str]) -> Option<String> {
    let program = program.to_owned();
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let result = Command::new(&program).args(&args).output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(QUERY_TIMEOUT) {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
    }
}
