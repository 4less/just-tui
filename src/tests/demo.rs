//! What the browser demo is served: the justfile it shows, and the logs
//! behind the jobs in its queue.

use std::path::Path;

use crate::slurm;
use crate::world::demo;

const BASE: &str = "/home/you/phix";

fn read(relative: &str) -> String {
    demo::read(&Path::new(BASE).join(relative)).expect("the demo has this file")
}

#[test]
fn the_justfile_is_a_real_dump_of_the_file_shown_beside_it() {
    let dump: crate::model::Justfile =
        serde_json::from_str(demo::DUMP).expect("a real `just --dump`");

    // The pipeline: fetch a reference, simulate reads, align them, count.
    assert!(dump.recipes.contains_key("fetch-reference"));
    assert!(dump.recipes.contains_key("simulate-reads"));
    assert!(dump.modules["align"].recipes.contains_key("reads"));

    // And the source pane has the text those recipes were dumped from.
    let justfile = read("justfile");
    assert!(justfile.contains("fetch-reference:"));
    assert!(read("align/align.just").contains("minibwa map"));
}

#[test]
fn a_job_s_two_logs_are_not_the_same_log() {
    // minimap2 writes its progress to stderr; samtools writes the flagstat
    // table to stdout. Both were captured by running the pipeline.
    let err = read("logs/align/reads-418823.err");
    let out = read("logs/align/reads-418823.out");

    assert!(err.contains("[M::worker_pipeline"), "minibwa's mapping log");
    assert!(
        out.contains("in total (QC-passed reads"),
        "the flagstat table"
    );
    assert_ne!(err, out, "stdout and stderr are different logs");

    // Downloading is the same story: curl's transfer table against what the
    // recipe echoed afterwards.
    let err = read("logs/fetch-reference-418805.err");
    let out = read("logs/fetch-reference-418805.out");
    assert!(err.contains("% Total"), "curl's own progress table");
    assert!(out.contains("5386 bp"), "what the recipe reported");
    assert_ne!(err, out);
}

#[test]
fn the_failed_job_says_why_it_failed() {
    let err = read("logs/align/reads-418820.err");
    assert!(
        err.contains("failed to load the index"),
        "minibwa's own words"
    );
    assert!(
        err.lines().any(slurm::is_error_line),
        "and the browser picks the line out"
    );
}

#[test]
fn the_queue_parses_into_the_jobs_it_describes() {
    let list = slurm::merge_jobs(
        demo::capture("squeue", &[]).as_deref(),
        demo::capture("sacct", &[]).as_deref(),
        7,
    );

    let states: Vec<&str> = list.jobs.iter().map(|job| job.state_word()).collect();
    assert!(states.contains(&"RUNNING"));
    assert!(states.contains(&"FAILED"));
    assert!(states.contains(&"OUT_OF_MEMORY"));
    assert!(list.note.is_none(), "nothing to apologise for");

    // The running job is using something, against what it asked for.
    let running = list.jobs.iter().find(|job| job.id == "418823").unwrap();
    let usage = slurm::fetch_usage_from(demo::capture("sstat", &[]).as_deref().unwrap_or(""));
    let used = usage.get("418823").expect("sstat reported it");
    assert!(used.mem_percent(running).is_some_and(|pct| pct > 0.0));
    assert!(used.cpu_percent(running).is_some_and(|pct| pct > 0.0));
}
