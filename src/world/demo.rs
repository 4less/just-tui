//! What the browser answers with.
//!
//! There is no `just` here, no Slurm, and no disk, so every call that would
//! reach one is answered from the strings below — in the exact format the
//! real parsers already read. The interface is not simulated: it is the
//! application, reading canned output instead of a cluster's.

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// `just --dump --dump-format json` for a four-step alignment pipeline over
/// the phiX174 genome: fetch the reference, simulate reads, align them, count
/// what mapped. A module, groups, an alias and a private recipe.
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

/// Real output, captured by running this pipeline: minimap2 writes its
/// progress to stderr while samtools writes the flagstat table to stdout, so
/// the two logs of one job genuinely differ.
const ALIGN_ERR: &str = include_str!("demo/align.err");
const ALIGN_OUT: &str = include_str!("demo/align.out");
/// The same alignment against reads that had not been simulated yet.
const FAILED_ERR: &str = include_str!("demo/failed.err");
/// curl's transfer table, and what the recipe echoed after it.
const FETCH_ERR: &str = include_str!("demo/fetch.err");
const FETCH_OUT: &str = include_str!("demo/fetch.out");

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

/// The justfiles the tree is built from, and one log per job that has one.
pub fn read(path: &Path) -> Result<String> {
    let name = path.to_string_lossy();
    let err = name.ends_with(".err");

    let text = match () {
        _ if name.ends_with("align.just") => RUN_JUST,
        _ if name.ends_with("justfile") => JUSTFILE,
        // The job that failed: the reads were not there yet.
        _ if name.contains("418820") => match err {
            true => FAILED_ERR,
            false => "",
        },
        _ if name.contains("418805") => match err {
            true => FETCH_ERR,
            false => FETCH_OUT,
        },
        // Every other alignment, running or finished.
        _ if name.contains("418823") || name.contains("418799") => match err {
            true => ALIGN_ERR,
            false => ALIGN_OUT,
        },
        _ => return Err(Error::new(ErrorKind::NotFound, format!("no {name} here"))),
    };
    Ok(text.to_owned())
}

/// The log directory, as a real one would look after these jobs ran.
pub fn read_dir(path: &Path) -> Vec<(PathBuf, bool)> {
    let name = path.to_string_lossy().replace('\\', "/");
    let under = |dir: &str| name.ends_with(dir) || name.ends_with(&format!("{dir}/"));

    if under("logs/align") {
        return [
            "reads-sr-418823.out",
            "reads-sr-418823.err",
            "reads-sr-418820.out",
            "reads-sr-418820.err",
            "reads-map-ont-418799.out",
            "reads-map-ont-418799.err",
        ]
        .iter()
        .map(|file| (path.join(file), false))
        .collect();
    }
    if under("logs") {
        return [
            ("align", true),
            ("fetch-reference-418805.out", false),
            ("fetch-reference-418805.err", false),
            ("simulate-reads-4000-418812.out", false),
        ]
        .iter()
        .map(|(file, dir)| (path.join(file), *dir))
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
