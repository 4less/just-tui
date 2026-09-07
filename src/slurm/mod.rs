//! Submitting recipes to Slurm: what the cluster offers, what a recipe asks
//! for, and the `sbatch` command that results.

mod cluster;
mod command;
mod settings;
mod units;

pub use cluster::{Cluster, Partition};
pub use command::{Submission, log_paths, preview_command, submit, warnings};
// The full argument list every other form is derived from. Exported for tests
// and for callers who want the exact command rather than the display form.
#[allow(unused_imports)]
pub use command::build_command;
pub use settings::{FIELDS, Field, Settings};
pub use units::{format_mem, format_time, parse_mem, parse_time};
