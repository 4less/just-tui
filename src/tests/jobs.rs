//! Reading `squeue` and `sacct`, and the record kept of every submission.

use std::path::PathBuf;

use crate::history::{History, Record};
use crate::slurm::{self, Settings};

const SQUEUE: &str = "\
4210|nightly-run|RUNNING|qib-compute|2026-09-09T09:00:00|2026-09-09T09:01:00|00:12:33|node07|/work/proj|64|128G
4211|greet-force-1|PENDING|qib-compute|2026-09-09T09:05:00|N/A|0:00|(Resources)|/work/proj|8|16G";

const SACCT: &str = "\
4180|build|COMPLETED|qib-compute|2026-09-08T20:00:00|2026-09-08T20:00:10|2026-09-08T20:04:00|00:03:50|0:0|node02|/work/proj|4|16G
4190|tree-force-1|FAILED|qib-compute|2026-09-08T22:00:00|2026-09-08T22:00:05|2026-09-08T22:00:40|00:00:35|1:0|node03|/work/proj|8|64Gn
4190.batch|batch|FAILED|qib-compute|2026-09-08T22:00:00|2026-09-08T22:00:05|2026-09-08T22:00:40|00:00:35|1:0|node03|/work/proj|8|64Gn
4211|greet-force-1|PENDING|qib-compute|2026-09-09T09:05:00|None|Unknown|00:00:00|0:0|None assigned|/work/proj|8|16G";

#[test]
fn merges_the_queue_with_the_accounting_database() {
    let list = slurm::merge_jobs(Some(SQUEUE), Some(SACCT), 7);
    let ids: Vec<&str> = list.jobs.iter().map(|job| job.id.as_str()).collect();

    // Newest first, job steps dropped, and 4211 counted once.
    assert_eq!(ids, ["4211", "4210", "4190", "4180"]);
    assert!(list.note.is_none());

    let pending = &list.jobs[0];
    assert!(pending.live, "squeue wins for a job still in the queue");
    assert!(pending.pending() && pending.active());
    assert_eq!(pending.nodes, "Resources", "the reason it is waiting");

    let failed = list.jobs.iter().find(|job| job.id == "4190").unwrap();
    assert!(failed.failed());
    assert_eq!(failed.exit_label(), "exit 1");
    assert_eq!(failed.work_dir, "/work/proj");
    assert_eq!(failed.cpus, 8);
    // `ReqMem` writes `64Gn` for per-node; the suffix is not part of the size.
    assert_eq!(failed.alloc_mem_mb, Some(64 * 1024));

    let done = list.jobs.iter().find(|job| job.id == "4180").unwrap();
    assert!(!done.failed());
    assert_eq!(done.exit_label(), "ok");
}

#[test]
fn a_cancelled_job_counts_as_failed() {
    let line = "4300|x|CANCELLED by 1001|p|s|s|e|00:01:00|0:0|node01|/work|2|8G";
    let list = slurm::merge_jobs(None, Some(line), 7);
    let job = &list.jobs[0];

    assert_eq!(job.state_word(), "CANCELLED", "the reason is not the state");
    assert!(job.failed());
}

#[test]
fn without_slurm_the_list_says_so() {
    let list = slurm::merge_jobs(None, None, 7);
    assert!(list.jobs.is_empty());
    assert!(list.note.unwrap().contains("no Slurm"));
}

#[test]
fn error_lines_are_picked_out_of_a_log() {
    assert!(slurm::is_error_line("Traceback (most recent call last):"));
    assert!(slurm::is_error_line(
        "slurmstepd: error: exceeded memory limit"
    ));
    assert!(slurm::is_error_line("bash: nope: command not found"));
    assert!(!slurm::is_error_line("processing chunk 4 of 10"));
}

#[test]
fn history_keeps_every_submission() {
    let dir = std::env::temp_dir().join(format!("just-tui-history-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut history = History::load(&dir);
    for (job_id, namepath, mem) in [
        ("4190", "level3::db::tree", "64G"),
        ("4200", "level3::db::tree", "128G"),
        ("4210", "build", "8G"),
    ] {
        history
            .append(Record {
                job_id: job_id.to_owned(),
                namepath: namepath.to_owned(),
                name: namepath.replace("::", "-"),
                at: 1_757_000_000,
                when: "2026-09-09T09:00:00".to_owned(),
                settings: Settings {
                    mem: mem.to_owned(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
    }

    // Re-read from disk: the file is the record, not the in-memory copy.
    let reloaded = History::load(&dir);
    let tree = reloaded.for_recipe("level3::db::tree");
    assert_eq!(tree.len(), 2, "one entry per submission, not per recipe");
    assert_eq!(tree[0].job_id, "4200", "newest first");
    assert_eq!(tree[0].settings.mem, "128G");
    assert_eq!(tree[1].settings.mem, "64G", "the older run is still there");

    // An array task is traced back to the allocation that was submitted.
    assert_eq!(reloaded.for_job("4200_7").unwrap().job_id, "4200");
    assert!(reloaded.for_job("9999").is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn logs_are_found_by_the_job_id_in_their_name() {
    let dir = std::env::temp_dir().join(format!("just-tui-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logs/demo")).unwrap();
    std::fs::write(dir.join("logs/demo/greet-4190.out"), "working\n").unwrap();
    std::fs::write(
        dir.join("logs/demo/greet-4190.err"),
        "step 1\nslurmstepd: error: out of memory\n",
    )
    .unwrap();
    // A different job's log must not be picked up.
    std::fs::write(dir.join("logs/demo/greet-4191.err"), "other\n").unwrap();

    let job = slurm::Job {
        id: "4190".to_owned(),
        state: "FAILED".to_owned(),
        work_dir: dir.display().to_string(),
        ..Default::default()
    };
    let logs = slurm::find_logs(&job, &[]);
    assert_eq!(logs.err, Some(dir.join("logs/demo/greet-4190.err")));
    assert_eq!(logs.out, Some(dir.join("logs/demo/greet-4190.out")));

    let (lines, truncated) = slurm::tail(logs.err.as_ref().unwrap(), 100);
    assert_eq!(lines.last().unwrap(), "slurmstepd: error: out of memory");
    assert!(!truncated);

    // A job nobody logged for reports why, rather than someone else's file.
    let missing = slurm::Job {
        id: "5000".to_owned(),
        work_dir: dir.display().to_string(),
        ..Default::default()
    };
    let logs = slurm::find_logs(&missing, &[PathBuf::from("/nonexistent")]);
    assert!(logs.out.is_none() && logs.err.is_none());
    assert!(logs.note.unwrap().contains("5000"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_running_clock_carries_on_between_refreshes() {
    let list = slurm::merge_jobs(Some(SQUEUE), None, 7);
    let running = list.jobs.iter().find(|job| job.id == "4210").unwrap();

    assert_eq!(running.elapsed_secs, 12 * 60 + 33);
    assert_eq!(running.elapsed_now(0), "00:12:33");
    assert_eq!(running.elapsed_now(7), "00:12:40", "seven seconds later");

    // A job that is not running has stopped counting.
    let pending = list.jobs.iter().find(|job| job.id == "4211").unwrap();
    assert_eq!(pending.elapsed_now(90), "0:00");
}

#[test]
fn usage_is_measured_against_what_was_allocated() {
    let list = slurm::merge_jobs(Some(SQUEUE), None, 7);
    let job = list.jobs.iter().find(|job| job.id == "4210").unwrap();

    // 64G used of 128G allocated, and 8 core-hours of the 13.4 reserved.
    let usage = slurm::Usage {
        max_rss_mb: Some(64 * 1024),
        cpu_secs: Some(12 * 60 + 33),
    };
    assert_eq!(usage.mem_percent(job), Some(50.0));

    let cpu = usage.cpu_percent(job).unwrap();
    assert!(
        (cpu - 100.0 / 64.0).abs() < 0.01,
        "one core busy out of 64 is ~1.6%, got {cpu}"
    );

    // Nothing measured yet, and nothing to divide by, both say so.
    assert_eq!(slurm::Usage::default().mem_percent(job), None);
    let pending = list.jobs.iter().find(|job| job.id == "4211").unwrap();
    assert_eq!(usage.cpu_percent(pending), None, "no elapsed time yet");
}
