//! What a job asks for: one string per sbatch flag.

/// Every field is a string; empty means "do not pass this flag at all", so a
/// justfile that leaves `slurm_account` blank submits without `--account`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Field {
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
}

pub const FIELDS: [Field; 11] = [
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
];

impl Field {
    pub fn label(self) -> &'static str {
        match self {
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
            Field::Extra | Field::Args => return None,
        })
    }

    pub fn hint(self) -> &'static str {
        match self {
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
        }
    }
}

impl Settings {
    pub fn get(&self, field: Field) -> &str {
        match field {
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
        }
    }

    pub fn get_mut(&mut self, field: Field) -> &mut String {
        match field {
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
        }
    }
}
