//! What a job asks for: one string per sbatch flag.

use serde::{Deserialize, Serialize};

/// Every field is a string; empty means "do not pass this flag at all", so a
/// justfile that leaves `slurm_account` blank submits without `--account`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Job name. Empty means one is generated from the recipe and its args.
    pub name: String,
    pub partition: String,
    pub account: String,
    pub qos: String,
    pub cpus: String,
    pub mem: String,
    pub time: String,
    pub nodes: String,
    pub gpus: String,
    pub array: String,
    pub extra: String,
    /// Arguments appended to the `just` invocation, not to sbatch.
    pub args: String,
    /// A glob, or `@file`, expanding the recipe into one array task per value.
    pub each: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Field {
    Name,
    Partition,
    Account,
    Qos,
    Cpus,
    Mem,
    Time,
    Nodes,
    Gpus,
    Array,
    Extra,
    Args,
    Each,
}

pub const FIELDS: [Field; 13] = [
    Field::Name,
    Field::Partition,
    Field::Account,
    Field::Qos,
    Field::Cpus,
    Field::Mem,
    Field::Time,
    Field::Nodes,
    Field::Gpus,
    Field::Array,
    Field::Extra,
    Field::Args,
    Field::Each,
];

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "name",
            Field::Partition => "partition",
            Field::Account => "account",
            Field::Qos => "qos",
            Field::Cpus => "cpus",
            Field::Mem => "mem",
            Field::Time => "time",
            Field::Nodes => "nodes",
            Field::Gpus => "gpus",
            Field::Array => "array",
            Field::Extra => "extra",
            Field::Args => "args",
            Field::Each => "each",
        }
    }

    /// The sbatch flag this field becomes, if any.
    pub fn flag(self) -> Option<&'static str> {
        Some(match self {
            Field::Partition => "--partition",
            Field::Account => "--account",
            Field::Qos => "--qos",
            Field::Cpus => "--cpus-per-task",
            Field::Mem => "--mem",
            Field::Time => "--time",
            Field::Nodes => "--nodes",
            Field::Gpus => "--gres",
            Field::Array => "--array",
            Field::Name | Field::Extra | Field::Args | Field::Each => return None,
        })
    }

    pub fn hint(self) -> &'static str {
        match self {
            Field::Name => "job name — empty means auto",
            Field::Partition => "queue to run in",
            Field::Account => "billing account",
            Field::Qos => "quality of service",
            Field::Cpus => "--cpus-per-task",
            Field::Mem => "per node, e.g. 64G",
            Field::Time => "walltime, e.g. 48:00:00",
            Field::Nodes => "node count",
            Field::Gpus => "--gres, e.g. gpu:2",
            Field::Array => "e.g. 0-31%4",
            Field::Extra => "any further sbatch flags",
            Field::Args => "arguments for the recipe",
            Field::Each => "one job per match: data/*.txt or @runs.txt",
        }
    }
}

impl Settings {
    /// `qib-compute · cpu 32 · mem 64G`: what was asked for, and nothing
    /// that was left to the defaults.
    pub fn asked_for(&self) -> String {
        let mut parts = Vec::new();
        for (label, value) in [
            ("", self.partition.as_str()),
            ("", self.qos.as_str()),
            ("cpu ", self.cpus.as_str()),
            ("mem ", self.mem.as_str()),
            ("time ", self.time.as_str()),
            ("gpu ", self.gpus.as_str()),
            ("array ", self.array.as_str()),
        ] {
            if !value.is_empty() {
                parts.push(format!("{label}{value}"));
            }
        }
        parts.join(" · ")
    }

    /// Whether anything about the allocation itself has been decided. A
    /// recipe with none of these set is one the form has to ask about.
    pub fn configured(&self) -> bool {
        [&self.partition, &self.cpus, &self.mem, &self.time]
            .iter()
            .any(|value| !value.trim().is_empty())
    }

    pub fn get(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Partition => &self.partition,
            Field::Account => &self.account,
            Field::Qos => &self.qos,
            Field::Cpus => &self.cpus,
            Field::Mem => &self.mem,
            Field::Time => &self.time,
            Field::Nodes => &self.nodes,
            Field::Gpus => &self.gpus,
            Field::Array => &self.array,
            Field::Extra => &self.extra,
            Field::Args => &self.args,
            Field::Each => &self.each,
        }
    }

    pub fn get_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Partition => &mut self.partition,
            Field::Account => &mut self.account,
            Field::Qos => &mut self.qos,
            Field::Cpus => &mut self.cpus,
            Field::Mem => &mut self.mem,
            Field::Time => &mut self.time,
            Field::Nodes => &mut self.nodes,
            Field::Gpus => &mut self.gpus,
            Field::Array => &mut self.array,
            Field::Extra => &mut self.extra,
            Field::Args => &mut self.args,
            Field::Each => &mut self.each,
        }
    }
}
