//! Submitting recipes to Slurm: what the cluster offers, what a recipe asks
//! for, and the `sbatch` command that results.

mod cluster;
mod command;
mod jobs;
mod settings;
mod units;

pub use cluster::{Cluster, Partition};
pub use command::{
    Submission, job_name, log_paths, preview_command, resolved_log_paths, submit, warnings,
};
pub use jobs::{
    Job, JobList, Logs, Usage, cancel, fetch_usage, find_logs, is_error_line, refresh_queue, tail,
};
// Job listings are fetched by the app; re-exported under a name that says so.
pub use jobs::fetch as fetch_jobs;
// The pure half of the fetch, so the parsing can be tested without a cluster.
#[allow(unused_imports)]
pub use jobs::merge as merge_jobs;
// The full argument list every other form is derived from. Exported for tests
// and for callers who want the exact command rather than the display form.
#[allow(unused_imports)]
pub use command::build_command;
pub use settings::{FIELDS, Field, Settings};
pub use units::{format_mem, format_time, parse_mem, parse_time};
