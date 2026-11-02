//! Cluster presets from the global library, and taking another recipe's
//! settings from the form's picker.

use std::collections::BTreeMap;

use super::{fixture_app, test_cluster};
use crate::app::{App, Choice, Mode};
use crate::config::{PRESETS_NAME, Presets};
use crate::history::Record;
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::slurm::Settings;

fn settings(pairs: &[(&str, &str)]) -> Settings {
    let mut settings = Settings::default();
    for (key, value) in pairs {
        let field = match *key {
            "partition" => &mut settings.partition,
            "mem" => &mut settings.mem,
            "time" => &mut settings.time,
            "gpus" => &mut settings.gpus,
            "args" => &mut settings.args,
            "account" => &mut settings.account,
            _ => panic!("no field {key}"),
        };
        *field = (*value).to_owned();
    }
    settings
}

fn presets(named: &[(&str, Settings)]) -> Presets {
    Presets {
        dir: None,
        named: named
            .iter()
            .map(|(name, settings)| ((*name).to_owned(), settings.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn record(job: &str, namepath: &str, at: u64, settings: Settings) -> Record {
    Record {
        job_id: job.to_owned(),
        namepath: namepath.to_owned(),
        at,
        when: format!("2026-11-02T10:00:{at:02}"),
        settings,
        ..Default::default()
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// What F6 does, without re-reading a history file the test never wrote.
fn open_picker(app: &mut App) {
    app.picker = Some(crate::app::HistoryPick {
        namepath: app.form.as_ref().map(|form| form.namepath.clone()),
        index: 0,
        from: Mode::Submit,
    });
    app.mode = Mode::History;
}

fn form_on(app: &mut App, namepath: &str) {
    app.cluster = Some(test_cluster());
    assert!(app.select_namepath(namepath));
    app.open_submit_single();
    assert_eq!(app.mode, Mode::Submit);
}

#[test]
fn the_presets_file_is_read_with_default_first() {
    let dir = std::env::temp_dir().join(format!("just-tui-presets-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(PRESETS_NAME),
        "partition = compute\n\n[gpu]\ngpus = 1\nargs = x\n\n[bigmem]\nmem = 512G\n\n[default]\ntime = 04:00:00\n",
    )
    .unwrap();

    let presets = Presets::load(Some(&dir));
    let names: Vec<&str> = presets.ordered().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["default", "bigmem", "gpu"]);
    let default = presets.fallback().unwrap();
    assert_eq!(default.partition, "compute", "keys before a section");
    assert_eq!(default.time, "04:00:00", "and the [default] section");
    assert!(
        presets.named["gpu"].args.is_empty(),
        "a preset carries no arguments"
    );

    assert!(Presets::load(None).named.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recipe_with_nothing_of_its_own_starts_from_the_default_preset() {
    let mut app = fixture_app();
    app.presets = presets(&[("default", settings(&[("mem", "8G"), ("time", "04:00:00")]))]);
    form_on(&mut app, "post");
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.mem, "8G");
    assert_eq!(
        form.resolved.origin(crate::slurm::Field::Mem),
        Some("preset default")
    );
}

#[test]
fn a_recipe_submitted_before_ignores_the_default_preset() {
    let mut app = fixture_app();
    app.presets = presets(&[("default", settings(&[("mem", "8G")]))]);
    let dir = std::env::temp_dir().join(format!("just-tui-presets-last-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    app.configs = crate::config::Configs::new(&dir);
    app.configs
        .remember("post", &settings(&[("time", "01:00:00")]))
        .unwrap();

    form_on(&mut app, "post");
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.time, "01:00:00");
    assert!(
        form.settings.mem.is_empty(),
        "the preset is only a fallback"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn f6_lists_own_runs_then_presets_then_other_recipes_once_each() {
    let mut app = fixture_app();
    app.presets = presets(&[("gpu", settings(&[("gpus", "1")]))]);
    let big = settings(&[("partition", "long"), ("mem", "64G")]);
    app.history
        .push_for_test(record("1", "build", 1, big.clone()));
    app.history
        .push_for_test(record("2", "hello", 2, big.clone()));
    app.history
        .push_for_test(record("3", "hello", 3, settings(&[("args", "x")])));
    app.history
        .push_for_test(record("4", "post", 4, settings(&[("mem", "2G")])));
    app.history.push_for_test(record(
        "5",
        "hello",
        5,
        settings(&[("mem", "2G"), ("args", "y")]),
    ));

    let entries = app.history_entries(Some("post"));
    let rows: Vec<String> = entries
        .iter()
        .map(|choice| match choice {
            Choice::Past(r) => format!("past {}", r.job_id),
            Choice::Preset(name, _) => format!("preset {name}"),
            Choice::Other(r) => format!("other {}", r.job_id),
        })
        .collect();
    // 5 has post's own allocation; 3 asks for nothing; 1 repeats 2's.
    assert_eq!(rows, ["past 4", "preset gpu", "other 2"]);
}

#[test]
fn another_recipes_settings_keep_this_recipes_arguments() {
    let mut app = fixture_app();
    app.history.push_for_test(record(
        "7",
        "build",
        1,
        settings(&[("partition", "long"), ("mem", "64G"), ("args", "--release")]),
    ));
    form_on(&mut app, "hello");
    app.form.as_mut().unwrap().settings = settings(&[("args", "bob"), ("account", "lab")]);

    open_picker(&mut app);
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::Submit);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.namepath, "hello", "the form stays on its recipe");
    assert_eq!(form.settings.mem, "64G");
    assert_eq!(form.settings.args, "bob");
    assert!(
        form.settings.account.is_empty(),
        "the allocation comes whole"
    );
}

#[test]
fn a_preset_only_sets_what_it_names() {
    let mut app = fixture_app();
    app.presets = presets(&[("gpu", settings(&[("gpus", "1"), ("partition", "gpu")]))]);
    form_on(&mut app, "hello");
    app.form.as_mut().unwrap().settings = settings(&[("mem", "32G"), ("args", "bob")]);

    open_picker(&mut app);
    press(&mut app, KeyCode::Enter);

    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.gpus, "1");
    assert_eq!(form.settings.partition, "gpu");
    assert_eq!(form.settings.mem, "32G");
    assert_eq!(form.settings.args, "bob");
}
