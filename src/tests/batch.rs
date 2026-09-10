//! Expanding one recipe over many inputs: globs, manifests, and the array
//! command that results.

use std::path::{Path, PathBuf};

use crate::batch;
use crate::slurm::{self, Settings};

/// A directory of inputs to glob over, named after the calling test so the
/// suite can run in parallel.
fn inputs(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("just-tui-batch-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/runs")).unwrap();
    for name in ["gamma.txt", "alpha.txt", "beta.txt"] {
        std::fs::write(dir.join("data").join(name), "x").unwrap();
    }
    std::fs::write(dir.join("data/notes.md"), "x").unwrap();
    std::fs::write(dir.join("data/.hidden.txt"), "x").unwrap();
    std::fs::write(dir.join("data/runs/one.txt"), "x").unwrap();
    dir
}

fn each(pattern: &str, args: &str) -> Settings {
    Settings {
        each: pattern.to_owned(),
        args: args.to_owned(),
        ..Default::default()
    }
}

#[test]
fn a_glob_becomes_one_line_per_match() {
    let dir = inputs("glob");
    let plan = batch::plan(&dir, "count", &each("data/*.txt", "{}"))
        .unwrap()
        .expect("an expansion");

    assert_eq!(
        plan.lines,
        ["data/alpha.txt", "data/beta.txt", "data/gamma.txt"],
        "sorted, so task numbers mean the same thing every time"
    );
    assert!(
        !plan.lines.iter().any(|line| line.contains("notes.md")),
        "the pattern is respected"
    );
    assert!(
        !plan.lines.iter().any(|line| line.contains("hidden")),
        "a dotfile is not matched by a pattern that does not ask for one"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wildcards_work_in_any_component() {
    let dir = inputs("nested");
    let plan = batch::plan(&dir, "count", &each("data/*/one.txt", "{}"))
        .unwrap()
        .expect("an expansion");
    assert_eq!(plan.lines, ["data/runs/one.txt"]);

    // A pattern nothing matches is a mistake worth reporting, not an empty
    // array submitted silently.
    let missing = batch::plan(&dir, "count", &each("data/*.fna", "{}"));
    assert!(missing.unwrap_err().contains("matched nothing"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_supplies_the_lines_instead() {
    let dir = inputs("file");
    std::fs::write(
        dir.join("runs.txt"),
        "# the arms worth running\n\ntgt_filt_peel\ncong_filt_peel force=1\n",
    )
    .unwrap();

    let plan = batch::plan(&dir, "run", &each("@runs.txt", ""))
        .unwrap()
        .expect("an expansion");
    assert_eq!(
        plan.lines,
        ["tgt_filt_peel", "cong_filt_peel force=1"],
        "blank lines and comments are not tasks"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_value_lands_where_the_placeholder_is() {
    let dir = inputs("placeholder");

    let with = batch::plan(&dir, "count", &each("data/*.txt", "input={} mode=peel"))
        .unwrap()
        .unwrap();
    assert_eq!(with.lines[0], "input=data/alpha.txt mode=peel");

    // No placeholder: the value is appended, which is how `just` takes a
    // positional parameter.
    let without = batch::plan(&dir, "count", &each("data/*.txt", "--verbose"))
        .unwrap()
        .unwrap();
    assert_eq!(without.lines[0], "--verbose data/alpha.txt");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_manifest_is_named_after_what_it_holds() {
    let dir = inputs("manifest");
    let settings = each("data/*.txt", "{}");

    let plan = batch::plan(&dir, "level3::run::count", &settings)
        .unwrap()
        .unwrap();
    assert!(
        plan.manifest.starts_with("logs/level3/run"),
        "it sits with the logs, got {}",
        plan.manifest.display()
    );

    // The same expansion names the same file, so resubmitting is harmless.
    let again = batch::plan(&dir, "level3::run::count", &settings)
        .unwrap()
        .unwrap();
    assert_eq!(plan.manifest, again.manifest);

    // A different one never overwrites a manifest an array may still be
    // reading its way through.
    std::fs::write(dir.join("data/delta.txt"), "x").unwrap();
    let changed = batch::plan(&dir, "level3::run::count", &settings)
        .unwrap()
        .unwrap();
    assert_ne!(plan.manifest, changed.manifest);

    // And it can be read back a task at a time.
    batch::write(&dir, &changed).unwrap();
    let path = changed.manifest.display().to_string();
    assert_eq!(batch::line(&dir, &path, 1).unwrap(), "data/beta.txt");
    assert_eq!(batch::line(&dir, &path, 99), None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_expansion_submits_as_one_array() {
    let settings = Settings {
        each: "data/*.txt".into(),
        args: "{}".into(),
        partition: "qib-compute".into(),
        // Only the throttle survives: the range comes from the expansion.
        array: "0-999%4".into(),
        ..Default::default()
    };
    let manifest = Path::new("logs/count-245df667.args");
    let args = slurm::build_command(Path::new("/work"), "count", &settings, Some((manifest, 37)));

    assert!(args.contains(&"--array=0-36%4".to_owned()), "got {args:?}");
    assert_eq!(
        args.iter().filter(|a| a.starts_with("--array")).count(),
        1,
        "the typed range does not survive alongside the expansion"
    );

    let wrap = args.iter().position(|a| a == "--wrap").unwrap();
    assert_eq!(
        args[wrap + 1],
        "just count $(sed -n \"$((SLURM_ARRAY_TASK_ID+1))p\" logs/count-245df667.args)",
        "the task resolves its own line, on the node"
    );

    // Every task writes its own log, and the name says nothing about
    // arguments that differ between them.
    let (out, _) = slurm::log_paths("count", &settings);
    assert_eq!(out.to_str().unwrap(), "logs/count-%A_%a.out");
    assert_eq!(slurm::job_name("count", &settings), "count");
}
