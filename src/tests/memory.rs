//! Where just-tui keeps what it remembers: a project's recipes beside its
//! justfile, global recipes in the global library.

use std::path::{Path, PathBuf};

use super::{fixture_source, global_source, test_cluster};
use crate::app::App;
use crate::history::{HISTORY_NAME, History, Record};
use crate::params::RUN_ARGS_NAME;
use crate::slurm::{self, Settings};
use crate::source::SourceCache;

/// A fresh, empty directory under the system's temp dir.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("just-tui-memory-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project in `project`, plus the `git` library from `library`, run from
/// the project.
fn app_with_library(project: &Path, library: &Path) -> App {
    let mut source = fixture_source();
    source.working_dir = project.to_path_buf();
    let mut global = global_source();
    global.working_dir = project.to_path_buf();
    global.explicit_file = Some(library.join("git.just"));
    let mut app = App::new(vec![source, global], SourceCache::default());
    app.set_global_dir(Some(library.to_path_buf()));
    app.cluster = Some(test_cluster());
    app
}

#[test]
fn a_global_recipe_logs_and_remembers_in_the_library() {
    let (project, library) = (scratch("proj"), scratch("lib"));
    let mut app = app_with_library(&project, &library);

    assert!(app.select_namepath("st"));
    app.open_submit();
    let form = app.form.as_ref().expect("form is open");
    assert!(form.global);
    assert_eq!(
        form.base, project,
        "the job still runs where just-tui started"
    );
    assert_eq!(form.logs, library);

    let args = slurm::build_command(
        &form.base,
        &form.logs,
        &form.just_args,
        &form.namepath,
        &form.settings,
        None,
        slurm::Waits::ALONE,
    );
    let out = format!("--output={}", library.join("logs/st-%j.out").display());
    assert!(args.contains(&out), "logs go to the library: {args:?}");

    // What it was submitted with goes to the library's state file.
    app.configs_for(true)
        .remember(
            "st",
            &Settings {
                mem: "4G".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(library.join(crate::config::STATE_NAME).exists());
    assert!(!project.join(crate::config::STATE_NAME).exists());

    // A project recipe is untouched: logs relative, under the project.
    app.form = None;
    app.mode = crate::app::Mode::Normal;
    assert!(app.select_namepath("post"));
    app.open_submit();
    let form = app.form.as_ref().unwrap();
    assert!(!form.global);
    assert_eq!(form.logs, project);
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&library);
}

#[test]
fn a_global_recipes_run_arguments_are_kept_in_the_library() {
    let (project, library) = (scratch("args-proj"), scratch("args-lib"));
    let mut app = app_with_library(&project, &library);

    assert!(app.select_namepath("st"));
    app.handle_key(crate::input::KeyEvent::new(
        crate::input::KeyCode::Char('a'),
        crate::input::KeyModifiers::empty(),
    ));
    app.handle_key(crate::input::KeyEvent::new(
        crate::input::KeyCode::Char('x'),
        crate::input::KeyModifiers::empty(),
    ));
    app.handle_key(crate::input::KeyEvent::new(
        crate::input::KeyCode::Enter,
        crate::input::KeyModifiers::empty(),
    ));

    assert!(library.join(RUN_ARGS_NAME).exists());
    assert!(!project.join(RUN_ARGS_NAME).exists());
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&library);
}

#[test]
fn history_reads_both_files_and_writes_each_record_to_its_own() {
    let (project, library) = (scratch("hist-proj"), scratch("hist-lib"));
    let mut history = History::load(&project).with_global(Some(&library));
    let record = |job_id: &str, at: u64, global: bool| Record {
        job_id: job_id.to_owned(),
        namepath: "st".to_owned(),
        at,
        global,
        ..Default::default()
    };
    history.append(record("1", 10, false)).unwrap();
    history.append(record("2", 20, true)).unwrap();
    history.append(record("3", 30, false)).unwrap();

    let in_file = |dir: &Path| std::fs::read_to_string(dir.join(HISTORY_NAME)).unwrap();
    assert!(in_file(&project).contains("\"job_id\":\"1\""));
    assert!(!in_file(&project).contains("\"job_id\":\"2\""));
    assert!(in_file(&library).contains("\"job_id\":\"2\""));

    // Read back merged, newest first, each knowing where it came from.
    let reloaded = History::load(&project).with_global(Some(&library));
    let ids: Vec<&str> = reloaded.recent().map(|r| r.job_id.as_str()).collect();
    assert_eq!(ids, ["3", "2", "1"]);
    assert!(reloaded.for_job("2").unwrap().global);

    // Without a library, a global record has nowhere else to go.
    let mut alone = History::load(&project);
    alone.append(record("4", 40, true)).unwrap();
    assert!(in_file(&project).contains("\"job_id\":\"4\""));
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&library);
}
