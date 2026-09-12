//! Turning settings into an `sbatch` invocation, and checking them first.

use std::path::{Path, PathBuf};

use super::{Cluster, FIELDS, Field, Settings, format_mem, format_time, parse_mem, parse_time};

/// Longest generated job name. Slurm truncates in its own displays long before
/// this, but a name built from a dozen arguments is unreadable anyway.
const NAME_MAX: usize = 96;

/// `force=1 n=10` reads as `force-1-n-10`: safe in a file name, and still
/// recognisable in `squeue`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dashed = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            out.push(c);
            dashed = false;
        } else if !dashed {
            out.push('-');
            dashed = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn clip(mut name: String) -> String {
    if name.chars().count() > NAME_MAX {
        name = name.chars().take(NAME_MAX).collect();
        while name.ends_with('-') {
            name.pop();
        }
    }
    name
}

/// What `--job-name` gets: whatever was typed in the form, or the recipe path
/// with its arguments baked in, so two runs of one recipe stay apart.
pub fn job_name(namepath: &str, settings: &Settings) -> String {
    if !settings.name.trim().is_empty() {
        return clip(slug(settings.name.trim()));
    }
    let mut name = slug(&namepath.replace("::", "-"));
    // An expansion gives every task different arguments, so baking the
    // template into the name would say nothing about any of them.
    let args = match is_expansion(settings) {
        true => String::new(),
        false => slug(settings.args.trim()),
    };
    if !args.is_empty() {
        name.push('-');
        name.push_str(&args);
    }
    clip(name)
}

/// Whether this submission is a job array: an explicit `--array`, or an
/// `each` that expands into one.
fn is_expansion(settings: &Settings) -> bool {
    !settings.array.trim().is_empty() || !settings.each.trim().is_empty()
}

/// The log file stem. The module path is already in the directory, so only the
/// recipe name and its arguments are needed here.
pub fn log_stem(namepath: &str, settings: &Settings) -> String {
    if !settings.name.trim().is_empty() {
        return clip(slug(settings.name.trim()));
    }
    let leaf = namepath.rsplit("::").next().unwrap_or("job");
    let mut stem = slug(leaf);
    let args = match is_expansion(settings) {
        true => String::new(),
        false => slug(settings.args.trim()),
    };
    if !args.is_empty() {
        stem.push('-');
        stem.push_str(&args);
    }
    clip(stem)
}

/// `logs/<module path>/<recipe>-<args>-%j.out`, mirroring the module tree
/// under the base directory of the root justfile.
pub fn log_paths(namepath: &str, settings: &Settings) -> (PathBuf, PathBuf) {
    let mut path = PathBuf::from("logs");
    let parts: Vec<&str> = namepath.split("::").collect();
    for module in &parts[..parts.len().saturating_sub(1)] {
        path.push(module);
    }
    // Array tasks need the array id as well, or every task writes to one file.
    let stem = match is_expansion(settings) {
        true => format!("{}-%A_%a", log_stem(namepath, settings)),
        false => format!("{}-%j", log_stem(namepath, settings)),
    };
    (
        path.join(format!("{stem}.out")),
        path.join(format!("{stem}.err")),
    )
}

/// The log paths with Slurm's patterns filled in, for a job that now has an
/// id. `%a` stays a glob: only the running task knows its own number.
pub fn resolved_log_paths(namepath: &str, settings: &Settings, job_id: &str) -> (String, String) {
    let (out, err) = log_paths(namepath, settings);
    let fill = |p: PathBuf| {
        p.display()
            .to_string()
            .replace("%j", job_id)
            .replace("%A", job_id)
            .replace("%a", "*")
    };
    (fill(out), fill(err))
}

/// The full `sbatch` argument list for a recipe.
pub fn build_command(
    base: &Path,
    namepath: &str,
    settings: &Settings,
    batch: Option<(&Path, usize)>,
) -> Vec<String> {
    let (out, err) = log_paths(namepath, settings);
    let mut args = vec![
        "--parsable".to_owned(),
        format!("--chdir={}", base.display()),
        format!("--job-name={}", job_name(namepath, settings)),
        format!("--output={}", out.display()),
        format!("--error={}", err.display()),
    ];

    for field in FIELDS {
        let value = settings.get(field);
        if value.is_empty() {
            continue;
        }
        match field.flag() {
            // An expansion sets the range itself; anything typed in the array
            // field is then only good for a `%n` throttle.
            Some(_) if field == Field::Array && batch.is_some() => {}
            Some(flag) => args.push(format!("{flag}={value}")),
            None if field == Field::Extra => {
                args.extend(value.split_whitespace().map(str::to_owned));
            }
            None => {}
        }
    }
    if let Some((_, count)) = batch {
        args.push(format!("--array={}", array_range(settings, count)));
    }

    args.push("--wrap".to_owned());
    args.push(wrap_command(
        namepath,
        settings,
        batch.map(|(path, _)| path),
    ));
    args
}

/// `0-36`, carrying over a `%n` throttle if one was typed in the array field.
fn array_range(settings: &Settings, count: usize) -> String {
    let range = format!("0-{}", count.saturating_sub(1));
    match settings.array.trim().split_once('%') {
        Some((_, throttle)) if !throttle.is_empty() => format!("{range}%{throttle}"),
        _ => range,
    }
}

/// What the job actually runs. An expansion reads its own line of the
/// manifest; `$(…)` is left for the shell sbatch runs this under, so the task
/// id is resolved on the node rather than here.
fn wrap_command(namepath: &str, settings: &Settings, manifest: Option<&Path>) -> String {
    if let Some(manifest) = manifest {
        return format!(
            "just {namepath} $(sed -n \"$((SLURM_ARRAY_TASK_ID+1))p\" {})",
            manifest.display()
        );
    }
    let mut command = format!("just {namepath}");
    if !settings.args.trim().is_empty() {
        command.push(' ');
        command.push_str(settings.args.trim());
    }
    command
}

/// The same command, trimmed to what a reader cares about: the bookkeeping
/// flags are shown elsewhere in the form.
pub fn preview_command(
    base: &Path,
    namepath: &str,
    settings: &Settings,
    batch: Option<(&Path, usize)>,
) -> String {
    let skip = [
        "--parsable",
        "--chdir=",
        "--job-name=",
        "--output=",
        "--error=",
    ];
    let shown: Vec<String> = build_command(base, namepath, settings, batch)
        .into_iter()
        .filter(|arg| !skip.iter().any(|prefix| arg.starts_with(prefix)))
        .collect();

    let mut out = String::from("sbatch");
    let mut iter = shown.into_iter().peekable();
    while let Some(arg) = iter.next() {
        out.push(' ');
        if arg == "--wrap" {
            let command = iter.next().unwrap_or_default();
            out.push_str(&format!("--wrap \"{command}\""));
        } else {
            out.push_str(&arg);
        }
    }
    out
}

/// Directory the log files will land in, so it can be created first.
pub fn log_dir(base: &Path, namepath: &str, settings: &Settings) -> PathBuf {
    let (out, _) = log_paths(namepath, settings);
    base.join(out.parent().unwrap_or(Path::new("logs")))
}

pub enum Submission {
    Ok { job_id: String },
    Failed { message: String },
}

/// Create the log directory, then hand the job to sbatch.
pub fn submit(
    base: &Path,
    namepath: &str,
    settings: &Settings,
    batch: Option<(&Path, usize)>,
) -> Submission {
    let dir = log_dir(base, namepath, settings);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return Submission::Failed {
            message: format!("could not create {}: {err}", dir.display()),
        };
    }

    let args = build_command(base, namepath, settings, batch);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();

    match super::cluster::capture_in("sbatch", &borrowed, base) {
        // `--parsable` prints `jobid` or `jobid;cluster`.
        Ok(text) => Submission::Ok {
            job_id: text
                .trim()
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_owned(),
        },
        Err(message) => Submission::Failed { message },
    }
}

/// Complaints about the current settings, checked against the partition they
/// would run on. Advisory only — sbatch has the final say.
pub fn warnings(cluster: &Cluster, settings: &Settings) -> Vec<String> {
    if !cluster.detected {
        return Vec::new();
    }

    // An empty partition field means the cluster's own default is used.
    let partition = match settings.partition.as_str() {
        "" => cluster.default_partition(),
        name => cluster.partition(name),
    };
    let Some(partition) = partition else {
        return vec![format!(
            "partition `{}` is not one of: {}",
            settings.partition,
            cluster.partition_names().join(", ")
        )];
    };

    let mut out = Vec::new();
    if let (Some(want), Some(limit)) = (parse_mem(&settings.mem), partition.max_mem_mb)
        && want > limit
    {
        out.push(format!(
            "mem {} exceeds {} on {}",
            format_mem(want),
            format_mem(limit),
            partition.name
        ));
    }
    if let (Ok(want), Some(limit)) = (settings.cpus.parse::<u32>(), partition.node_cpus)
        && want > limit
    {
        out.push(format!(
            "{want} cpus exceeds {limit} per node on {}",
            partition.name
        ));
    }
    if let (Some(want), Some(limit)) = (parse_time(&settings.time), partition.max_time)
        && want > limit
    {
        out.push(format!(
            "time {} exceeds {} on {}",
            format_time(want),
            format_time(limit),
            partition.name
        ));
    }
    if !settings.qos.is_empty()
        && !partition.allow_qos.is_empty()
        && !partition.allow_qos.contains(&settings.qos)
    {
        out.push(format!(
            "qos `{}` is not allowed on {}",
            settings.qos, partition.name
        ));
    }
    out
}
