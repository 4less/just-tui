//! What the browser answers with.
//!
//! There is no `just` here, no Slurm, and no disk, so every call that would
//! reach one is answered from the strings below — in the exact format the
//! real parsers already read. The interface is not simulated: it is the
//! application, reading canned output instead of a cluster's.

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// `just --dump --dump-format json` for a small bioinformatics-flavoured
/// justfile with a module, a group, an alias and a private recipe.
pub const DUMP: &str = include_str!("demo/justfile.json");

/// The justfile itself, so the source pane has real text to show.
const JUSTFILE: &str = include_str!("demo/justfile.txt");

const RUN_JUST: &str = include_str!("demo/run.just");

/// `squeue -u you --noheader --format=%i|%j|%T|%P|%V|%S|%M|%R|%Z|%C|%m`
const SQUEUE: &str = include_str!("demo/squeue.txt");

/// `sacct -u you -X --noheader --parsable2 …`
const SACCT: &str = include_str!("demo/sacct.txt");

/// `sstat --noheader --parsable2 --format=JobID,MaxRSS,AveCPU -j …`
const SSTAT: &str = include_str!("demo/sstat.txt");

const PARTITIONS: &str = include_str!("demo/partitions.txt");
const SINFO: &str = include_str!("demo/sinfo.txt");
const QOS: &str = include_str!("demo/qos.txt");
const ACCOUNTS: &str = include_str!("demo/accounts.txt");

const LOG_FAILED: &str = include_str!("demo/failed.err");
const LOG_RUNNING: &str = include_str!("demo/running.out");

/// Answer a read of the cluster.
pub fn capture(program: &str, args: &[&str]) -> Option<String> {
    let joined = args.join(" ");
    match program {
        "squeue" => Some(SQUEUE.to_owned()),
        "sacct" => Some(SACCT.to_owned()),
        "sstat" => Some(SSTAT.to_owned()),
        "sacctmgr" if joined.contains("qos") => Some(QOS.to_owned()),
        "sacctmgr" => Some(ACCOUNTS.to_owned()),
        "sinfo" => Some(SINFO.to_owned()),
        "scontrol" if joined.contains("partition") => Some(PARTITIONS.to_owned()),
        // `scontrol show job` is what finds a live job's log. Letting it fail
        // sends the search down the same directory walk a real cluster uses.
        _ => None,
    }
}

/// Nothing is really submitted; the id is the one a queue would hand back.
pub fn capture_in(program: &str, _args: &[&str]) -> std::result::Result<String, String> {
    match program {
        "sbatch" => Ok("23475207".to_owned()),
        other => Err(format!("{other} is not available in the browser demo")),
    }
}

pub fn run(program: &str, _args: &[&str]) -> std::result::Result<(), String> {
    match program {
        "scancel" => Ok(()),
        other => Err(format!("{other} is not available in the browser demo")),
    }
}

/// A handful of files: the justfile the tree is built from, and two logs.
pub fn read(path: &Path) -> Result<String> {
    let name = path.to_string_lossy();
    let text = if name.ends_with("run.just") {
        RUN_JUST
    } else if name.ends_with("justfile") {
        JUSTFILE
    } else if name.ends_with("-23474876.err") || name.ends_with("-23474876.out") {
        LOG_FAILED
    } else if name.contains("23474919") {
        LOG_RUNNING
    } else {
        return Err(Error::new(ErrorKind::NotFound, format!("no {name} here")));
    };
    Ok(text.to_owned())
}

/// The log directory, as a real one would look after these jobs ran.
pub fn read_dir(path: &Path) -> Vec<(PathBuf, bool)> {
    let name = path.to_string_lossy().replace('\\', "/");
    let under = |dir: &str| name.ends_with(dir) || name.ends_with(&format!("{dir}/"));

    if under("logs") {
        return vec![(path.join("level3"), true)];
    }
    if under("logs/level3") {
        return vec![(path.join("run"), true)];
    }
    if under("logs/level3/run") {
        return [
            "s01_msa-tgt_filt_peel-23474919.out",
            "s01_msa-tgt_filt_peel-23474919.err",
            "s01_msa-nonfocal_tgt-23474876.out",
            "s01_msa-nonfocal_tgt-23474876.err",
            "align-index-23473080.out",
        ]
        .iter()
        .map(|file| (path.join(file), false))
        .collect();
    }
    Vec::new()
}

pub fn exists(path: &Path) -> bool {
    read(path).is_ok()
}

/// A fixed moment, so the demo reads the same whenever it is opened.
pub fn timestamp() -> Option<String> {
    Some("2026-09-12T09:14:03".to_owned())
}
