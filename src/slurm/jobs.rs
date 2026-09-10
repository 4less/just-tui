//! Looking at jobs that were already submitted: what the queue says about
//! them, what the accounting database remembers, and the logs they left.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use std::collections::HashMap;

use super::cluster::capture;
use super::parse_mem;

/// Fields asked of `squeue`, in the order [`parse_squeue`] reads them.
const SQUEUE_FORMAT: &str = "--format=%i|%j|%T|%P|%V|%S|%M|%R|%Z|%C|%m";

/// Fields asked of `sacct`, in the order [`parse_sacct`] reads them.
const SACCT_FORMAT: &str = "--format=JobID,JobName,State,Partition,Submit,Start,End,Elapsed,\
                            ExitCode,NodeList,WorkDir,AllocCPUS,ReqMem";

/// Fields asked of `sstat`, in the order [`parse_usage`] reads them.
const SSTAT_FORMAT: &str = "--format=JobID,MaxRSS,AveCPU";

/// Longest tail read from a log file. A job that printed a gigabyte of
/// progress bars should still open instantly.
const TAIL_BYTES: u64 = 512 * 1024;

/// How deep under a working directory log files are looked for.
const WALK_DEPTH: usize = 6;

/// One job, however it was found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub state: String,
    pub partition: String,
    pub submit: String,
    pub start: String,
    pub end: String,
    pub elapsed: String,
    /// `0:0`, or the reason a pending job is waiting.
    pub exit: String,
    /// Nodes it ran on, or the reason it has not started.
    pub nodes: String,
    pub work_dir: String,
    /// Still known to the controller, so `scontrol` can be asked about it.
    pub live: bool,
    /// CPUs allocated to the job.
    pub cpus: u32,
    /// Memory allocated, in MB.
    pub alloc_mem_mb: Option<u64>,
    /// [`Self::elapsed`] in seconds, so the clock can be carried forward
    /// between refreshes rather than re-queried every second.
    pub elapsed_secs: u64,
}

/// What a running job is actually using, from `sstat`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// High-water memory across the job's tasks, in MB.
    pub max_rss_mb: Option<u64>,
    /// Averaged CPU time consumed, in seconds.
    pub cpu_secs: Option<u64>,
}

impl Usage {
    /// Memory used as a share of what was asked for.
    pub fn mem_percent(&self, job: &Job) -> Option<f64> {
        let (used, alloc) = (self.max_rss_mb?, job.alloc_mem_mb?);
        if alloc == 0 {
            return None;
        }
        Some(used as f64 / alloc as f64 * 100.0)
    }

    /// CPU time consumed against the CPU time reserved: 100% means every
    /// allocated core has been busy for the whole run.
    pub fn cpu_percent(&self, job: &Job) -> Option<f64> {
        let reserved = job.elapsed_secs * u64::from(job.cpus);
        if reserved == 0 {
            return None;
        }
        Some(self.cpu_secs? as f64 / reserved as f64 * 100.0)
    }
}

impl Job {
    /// The state word alone: `sacct` writes `CANCELLED by 1234`.
    pub fn state_word(&self) -> &str {
        self.state.split_whitespace().next().unwrap_or("")
    }

    pub fn pending(&self) -> bool {
        matches!(self.state_word(), "PENDING" | "CONFIGURING" | "REQUEUED")
    }

    pub fn active(&self) -> bool {
        self.pending()
            || matches!(
                self.state_word(),
                "RUNNING" | "COMPLETING" | "SUSPENDED" | "RESIZING"
            )
    }

    pub fn failed(&self) -> bool {
        matches!(
            self.state_word(),
            "FAILED"
                | "TIMEOUT"
                | "CANCELLED"
                | "NODE_FAIL"
                | "OUT_OF_MEMORY"
                | "BOOT_FAIL"
                | "DEADLINE"
                | "PREEMPTED"
                | "REVOKED"
        ) || self
            .exit_status()
            .is_some_and(|(code, signal)| code != 0 || signal != 0)
    }

    /// `1:0` split into an exit code and a signal.
    pub fn exit_status(&self) -> Option<(i32, i32)> {
        let (code, signal) = self.exit.split_once(':')?;
        Some((code.trim().parse().ok()?, signal.trim().parse().ok()?))
    }

    /// How the exit is worth showing: `ok`, `exit 1`, `signal 9`.
    pub fn exit_label(&self) -> String {
        match self.exit_status() {
            Some((0, 0)) => "ok".to_owned(),
            Some((code, 0)) => format!("exit {code}"),
            Some((0, signal)) => format!("signal {signal}"),
            Some((code, signal)) => format!("exit {code} sig {signal}"),
            None => String::new(),
        }
    }

    /// The clock as it stands `since` seconds after the listing was fetched.
    /// Only a running job's clock moves.
    pub fn elapsed_now(&self, since: u64) -> String {
        match self.state_word() {
            "RUNNING" => format_elapsed(self.elapsed_secs + since),
            _ => self.elapsed.clone(),
        }
    }

    /// Array jobs are `12345_7`; the allocation they belong to is `12345`.
    pub fn array_root(&self) -> &str {
        self.id.split('_').next().unwrap_or(&self.id)
    }

    /// Sort key: newest first, tasks of one array in order.
    pub fn order(&self) -> (u64, u64) {
        let root = self.array_root().parse().unwrap_or(0);
        let task = self
            .id
            .split_once('_')
            .and_then(|(_, t)| t.trim_matches(['[', ']']).parse().ok())
            .unwrap_or(0);
        (root, task)
    }
}

/// Everything one refresh found, and why it might have found nothing.
#[derive(Debug, Clone, Default)]
pub struct JobList {
    pub jobs: Vec<Job>,
    pub note: Option<String>,
}

/// Ask `squeue` for what is queued and `sacct` for what has finished within
/// the last `days`. Never fails: a machine without Slurm reports a note.
pub fn fetch(days: u32) -> JobList {
    let user = std::env::var("USER").unwrap_or_default();

    let queued = match user.is_empty() {
        true => capture("squeue", &["--me", "--noheader", SQUEUE_FORMAT]),
        false => capture("squeue", &["-u", &user, "--noheader", SQUEUE_FORMAT]),
    };
    let history = match user.is_empty() {
        true => None,
        false => capture(
            "sacct",
            &[
                "-u",
                &user,
                "-X",
                "--noheader",
                "--parsable2",
                "--starttime",
                &format!("now-{days}days"),
                SACCT_FORMAT,
            ],
        ),
    };

    merge(queued.as_deref(), history.as_deref(), days)
}

/// Fold the two listings into one, newest first. Split out from [`fetch`] so
/// it can be tested without a scheduler.
pub fn merge(queued: Option<&str>, history: Option<&str>, days: u32) -> JobList {
    let mut list = JobList::default();
    if queued.is_none() && history.is_none() {
        list.note = Some("no Slurm here — squeue and sacct did not answer".to_owned());
        return list;
    }

    if let Some(raw) = queued {
        list.jobs.extend(raw.lines().filter_map(parse_squeue));
    }
    if let Some(raw) = history {
        for job in raw.lines().filter_map(parse_sacct) {
            // squeue is the fresher source for anything still in the queue.
            if !list.jobs.iter().any(|existing| existing.id == job.id) {
                list.jobs.push(job);
            }
        }
    } else {
        list.note = Some("sacct unavailable — only queued jobs are listed".to_owned());
    }

    list.jobs.sort_by_key(|job| std::cmp::Reverse(job.order()));
    if list.jobs.is_empty() && list.note.is_none() {
        list.note = Some(format!("no jobs of yours in the last {days} days"));
    }
    list
}

fn parse_squeue(line: &str) -> Option<Job> {
    let fields: Vec<&str> = line.split('|').map(str::trim).collect();
    let [
        id,
        name,
        state,
        partition,
        submit,
        start,
        elapsed,
        reason,
        work_dir,
        cpus,
        mem,
    ] = fields[..]
    else {
        return None;
    };
    Some(Job {
        id: id.to_owned(),
        name: name.to_owned(),
        state: state.to_owned(),
        partition: partition.to_owned(),
        submit: submit.to_owned(),
        start: start.to_owned(),
        end: String::new(),
        elapsed: elapsed.to_owned(),
        exit: String::new(),
        nodes: reason.trim_matches(['(', ')']).to_owned(),
        work_dir: work_dir.to_owned(),
        live: true,
        cpus: cpus.parse().unwrap_or(0),
        alloc_mem_mb: parse_mem(mem),
        elapsed_secs: parse_elapsed(elapsed).unwrap_or(0),
    })
}

fn parse_sacct(line: &str) -> Option<Job> {
    let fields: Vec<&str> = line.split('|').map(str::trim).collect();
    let [
        id,
        name,
        state,
        partition,
        submit,
        start,
        end,
        elapsed,
        exit,
        nodes,
        work_dir,
        cpus,
        mem,
    ] = fields[..]
    else {
        return None;
    };
    // `-X` should keep steps out, but old sacct still reports `123.batch`.
    if id.contains('.') {
        return None;
    }
    Some(Job {
        id: id.to_owned(),
        name: name.to_owned(),
        state: state.to_owned(),
        partition: partition.to_owned(),
        submit: submit.to_owned(),
        start: start.to_owned(),
        end: end.to_owned(),
        elapsed: elapsed.to_owned(),
        exit: exit.to_owned(),
        nodes: nodes.to_owned(),
        work_dir: work_dir.to_owned(),
        live: false,
        cpus: cpus.parse().unwrap_or(0),
        // `ReqMem` may be per-cpu, written `4Gc`; the trailing letter is
        // dropped by parse_mem, which is close enough for a display column.
        alloc_mem_mb: parse_mem(mem.trim_end_matches(['c', 'n'])),
        elapsed_secs: parse_elapsed(elapsed).unwrap_or(0),
    })
}

/// Re-ask `squeue` alone. Finished jobs never change, so a refresh on a timer
/// has no reason to wake `sacct` as well.
pub fn refresh_queue() -> Option<Vec<Job>> {
    let user = std::env::var("USER").unwrap_or_default();
    let raw = match user.is_empty() {
        true => capture("squeue", &["--me", "--noheader", SQUEUE_FORMAT])?,
        false => capture("squeue", &["-u", &user, "--noheader", SQUEUE_FORMAT])?,
    };
    Some(raw.lines().filter_map(parse_squeue).collect())
}

/// What each running job is using, in one `sstat` call rather than one per
/// job. Only the batch step is asked for: that is where the work happens.
pub fn fetch_usage(ids: &[String]) -> HashMap<String, Usage> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let steps = ids
        .iter()
        .map(|id| format!("{id}.batch"))
        .collect::<Vec<_>>()
        .join(",");
    let Some(raw) = capture(
        "sstat",
        &["--noheader", "--parsable2", "-j", &steps, SSTAT_FORMAT],
    ) else {
        return HashMap::new();
    };
    raw.lines().filter_map(parse_usage).collect()
}

/// `12345.batch|4321108K|00:12:34`
fn parse_usage(line: &str) -> Option<(String, Usage)> {
    let fields: Vec<&str> = line.split('|').map(str::trim).collect();
    let [id, rss, cpu] = fields[..] else {
        return None;
    };
    let id = id.split('.').next()?.to_owned();
    Some((
        id,
        Usage {
            max_rss_mb: parse_rss(rss),
            cpu_secs: parse_elapsed(cpu),
        },
    ))
}

/// Slurm's elapsed notation, to the second: `MM:SS`, `HH:MM:SS`, `D-HH:MM:SS`.
/// [`super::parse_time`] rounds to whole minutes, which a ticking clock cannot.
pub fn parse_elapsed(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("UNKNOWN") || text == "N/A" {
        return None;
    }
    let (days, rest) = match text.split_once('-') {
        Some((days, rest)) => (days.parse::<u64>().ok()?, rest),
        None => (0, text),
    };
    // Sub-second precision is noise here.
    let rest = rest.split('.').next().unwrap_or(rest);
    let parts: Vec<u64> = rest
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<_>>()?;

    let seconds = match parts.as_slice() {
        [only] => *only,
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => return None,
    };
    Some(days * 86_400 + seconds)
}

/// `HH:MM:SS`, or `D-HH:MM:SS` once it runs past a day.
pub fn format_elapsed(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hours, minutes, seconds) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    match days {
        0 => format!("{hours:02}:{minutes:02}:{seconds:02}"),
        _ => format!("{days}-{hours:02}:{minutes:02}:{seconds:02}"),
    }
}

/// `sstat` reports memory as a number with a unit letter, and in kilobytes
/// when the letter is missing. Values like `1.5G` are common, so the number
/// is read as a float.
fn parse_rss(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, suffix) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let mb = match suffix.trim().to_ascii_uppercase().as_str() {
        "" | "K" | "KB" => number / 1024.0,
        "M" | "MB" => number,
        "G" | "GB" => number * 1024.0,
        "T" | "TB" => number * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(mb.round() as u64)
}

// ---------------------------------------------------------------------------
// Finding the logs
// ---------------------------------------------------------------------------

/// Where a job's output went, and how that was worked out.
#[derive(Debug, Clone, Default)]
pub struct Logs {
    pub out: Option<PathBuf>,
    pub err: Option<PathBuf>,
    pub note: Option<String>,
}

impl Logs {
    pub fn found(&self) -> bool {
        self.out.is_some() || self.err.is_some()
    }
}

/// Ask Slurm where the job writes, then fall back to searching the `logs/`
/// tree for a file ending in the job id. `hints` are extra roots to search —
/// the project directory, for a job whose working directory is gone.
pub fn find_logs(job: &Job, hints: &[PathBuf]) -> Logs {
    if job.live
        && let Some(logs) = from_scontrol(&job.id)
    {
        return logs;
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if !job.work_dir.is_empty() {
        roots.push(PathBuf::from(&job.work_dir));
    }
    roots.extend(hints.iter().cloned());

    for root in &roots {
        let found = search(root, job);
        if found.found() {
            return found;
        }
    }

    Logs {
        note: Some(match job.pending() {
            true => "not started yet — no log written".to_owned(),
            false => format!("no log file ending in -{} found", job.id),
        }),
        ..Default::default()
    }
}

/// `scontrol show job` knows the exact paths, but only while the controller
/// still remembers the job.
fn from_scontrol(id: &str) -> Option<Logs> {
    let raw = capture("scontrol", &["show", "job", id, "--oneliner"])?;
    let mut logs = Logs::default();
    for token in raw.split_whitespace() {
        match token.split_once('=') {
            Some(("StdOut", path)) if path != "(null)" => logs.out = Some(PathBuf::from(path)),
            Some(("StdErr", path)) if path != "(null)" => logs.err = Some(PathBuf::from(path)),
            _ => {}
        }
    }
    // A job that has not started has paths but no files behind them.
    logs.out = logs.out.filter(|p| p.exists());
    logs.err = logs.err.filter(|p| p.exists());
    logs.found().then_some(logs)
}

/// Walk `<root>/logs` (then `root` itself) for files whose name ends in the
/// job id, which is what the `-%j` and `-%A_%a` patterns leave behind.
fn search(root: &Path, job: &Job) -> Logs {
    let mut logs = Logs::default();
    let mut files = Vec::new();
    collect(&root.join("logs"), WALK_DEPTH, &mut files);
    if files.is_empty() {
        collect(root, 1, &mut files);
    }

    for path in files {
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if !stem.ends_with(&format!("-{}", job.id))
            && !(job.id != job.array_root() && stem.ends_with(&format!("-{}", job.array_root())))
        {
            continue;
        }
        match path.extension().and_then(|e| e.to_str()) {
            Some("out") => logs.out = Some(path),
            Some("err") => logs.err = Some(path),
            _ => {}
        }
    }
    logs
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() > 4096 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => collect(&path, depth - 1, out),
            _ => out.push(path),
        }
    }
}

// ---------------------------------------------------------------------------
// Reading them
// ---------------------------------------------------------------------------

/// The end of a log file, which is where a failure says why. Returns the lines
/// and whether anything was cut off the front.
pub fn tail(path: &Path, max_lines: usize) -> (Vec<String>, bool) {
    let Ok(mut file) = std::fs::File::open(path) else {
        return (vec![format!("could not open {}", path.display())], false);
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let from = size.saturating_sub(TAIL_BYTES);
    let mut truncated = from > 0;
    let _ = file.seek(SeekFrom::Start(from));

    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return (vec![format!("could not read {}", path.display())], false);
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    // A partial first line is noise, not content.
    if truncated && !lines.is_empty() {
        lines.remove(0);
    }
    if lines.len() > max_lines {
        lines.drain(..lines.len() - max_lines);
        truncated = true;
    }
    if lines.is_empty() {
        lines.push(String::from("(empty)"));
    }
    (lines, truncated)
}

/// Whether a log line looks like the thing the reader is hunting for.
pub fn is_error_line(line: &str) -> bool {
    const MARKERS: [&str; 10] = [
        "error",
        "fatal",
        "traceback",
        "exception",
        "panicked",
        "segmentation fault",
        "command not found",
        "oom-kill",
        "cancelled due to",
        "exceeded memory",
    ];
    let lower = line.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}
