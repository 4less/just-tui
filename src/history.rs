//! `.just-tui-cluster-history`: one line of JSON per submitted job, holding
//! the exact settings it went out with.
//!
//! `.just-tui-cluster-state` only remembers the *last* settings per recipe, and
//! any config file overrides it. This file forgets nothing, so a job in the
//! queue can be traced back to what asked for it, and an old configuration can
//! be loaded back into the form.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::slurm::Settings;

pub const HISTORY_NAME: &str = ".just-tui-cluster-history";

/// Records kept before the file is trimmed, and the slack allowed above it so
/// the rewrite does not happen on every submit.
const KEEP: usize = 500;
const TRIM_AT: usize = 600;

/// One submission, as it was made.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Record {
    pub job_id: String,
    /// The recipe, as `module::name`.
    pub namepath: String,
    /// The job name Slurm was given, generated or typed.
    pub name: String,
    /// Seconds since the epoch, for ordering.
    pub at: u64,
    /// The same moment in local time, for reading.
    pub when: String,
    /// Log paths with `%j` already filled in.
    pub out: String,
    pub err: String,
    /// The `sbatch …` line as it was shown in the form.
    pub command: String,
    pub settings: Settings,
}

impl Record {
    /// `demo::greet  force=1` — what the entry is, in one short phrase.
    pub fn label(&self) -> String {
        match self.settings.args.trim().is_empty() {
            true => self.namepath.clone(),
            false => format!("{} {}", self.namepath, self.settings.args.trim()),
        }
    }

    /// The settings summarised for a list: only what was actually asked for.
    pub fn asked_for(&self) -> String {
        let mut parts = Vec::new();
        for (label, value) in [
            ("", self.settings.partition.as_str()),
            ("", self.settings.qos.as_str()),
            ("cpu ", self.settings.cpus.as_str()),
            ("mem ", self.settings.mem.as_str()),
            ("time ", self.settings.time.as_str()),
            ("gpu ", self.settings.gpus.as_str()),
            ("array ", self.settings.array.as_str()),
        ] {
            if !value.is_empty() {
                parts.push(format!("{label}{value}"));
            }
        }
        parts.join(" · ")
    }
}

/// Every submission made from this project, oldest first on disk.
#[derive(Debug, Default)]
pub struct History {
    path: PathBuf,
    records: Vec<Record>,
}

impl History {
    /// The file lives beside the root justfile, like the state file.
    pub fn load(base: &Path) -> Self {
        let path = base.join(HISTORY_NAME);
        let records = std::fs::read_to_string(&path)
            .map(|text| parse(&text))
            .unwrap_or_default();
        Self { path, records }
    }

    /// Newest first — the order every list in the UI wants.
    pub fn recent(&self) -> impl Iterator<Item = &Record> {
        self.records.iter().rev()
    }

    /// Past submissions of one recipe, newest first.
    pub fn for_recipe(&self, namepath: &str) -> Vec<&Record> {
        self.recent().filter(|r| r.namepath == namepath).collect()
    }

    /// What a job in the queue was submitted with, if just-tui submitted it.
    /// Array tasks are matched to the allocation they belong to.
    pub fn for_job(&self, job_id: &str) -> Option<&Record> {
        let root = job_id.split('_').next().unwrap_or(job_id);
        self.recent()
            .find(|r| r.job_id == job_id || r.job_id == root)
    }

    /// Append one submission, trimming the file when it has grown too long.
    pub fn append(&mut self, record: Record) -> std::io::Result<()> {
        self.records.push(record);
        if self.records.len() > TRIM_AT {
            self.records.drain(..self.records.len() - KEEP);
            return self.rewrite();
        }
        let line = match serde_json::to_string(self.records.last().expect("just pushed")) {
            Ok(line) => line,
            Err(err) => return Err(std::io::Error::other(err)),
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")
    }

    fn rewrite(&self) -> std::io::Result<()> {
        let mut text = String::new();
        for record in &self.records {
            if let Ok(line) = serde_json::to_string(record) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        std::fs::write(&self.path, text)
    }

    /// Re-read the file, so a run from another window shows up.
    pub fn reload(&mut self) {
        self.records = std::fs::read_to_string(&self.path)
            .map(|text| parse(&text))
            .unwrap_or_default();
    }
}

/// A damaged line is skipped rather than throwing the whole file away.
fn parse(text: &str) -> Vec<Record> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Seconds since the epoch, for ordering records.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The current local time, spelled the way `sacct` spells its timestamps.
/// `date` knows the timezone; working it out here would need a dependency.
pub fn timestamp() -> String {
    Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|stamp| !stamp.is_empty())
        .unwrap_or_else(|| format!("@{}", now()))
}
