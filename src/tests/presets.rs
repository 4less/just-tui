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

/// Move the picker's cursor to the first row `wanted` picks out.
fn pick(app: &mut App, wanted: impl Fn(&Choice) -> bool) {
    let index = app
        .picked_entries()
        .iter()
        .position(wanted)
        .expect("the row is offered");
    app.picker.as_mut().unwrap().index = index;
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
            Choice::Cluster(name, _) => format!("cluster {name}"),
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
    pick(&mut app, |choice| matches!(choice, Choice::Other(_)));
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

fn presets_app(name: &str) -> (App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("just-tui-f9-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(PRESETS_NAME), "# mine\n[bigmem]\nmem = 512G\n").unwrap();
    let mut app = fixture_app();
    app.load_presets(Some(dir.clone()));
    form_on(&mut app, "hello");
    (app, dir)
}

#[test]
fn f9_lists_saved_presets_then_partitions_not_saved_yet() {
    let (mut app, dir) = presets_app("list");
    press(&mut app, KeyCode::F(9));
    assert_eq!(app.mode, Mode::Presets);
    let rows: Vec<(String, bool)> = app
        .preset_rows()
        .into_iter()
        .map(|row| (row.name, row.inferred))
        .collect();
    assert_eq!(
        rows,
        [("bigmem".to_owned(), false), ("short".to_owned(), true)]
    );

    // Enter applies the saved one, setting only what it names.
    app.form.as_mut().unwrap().settings.time = "01:00:00".into();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Submit);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.mem, "512G");
    assert_eq!(form.settings.time, "01:00:00");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn i_saves_what_slurm_suggests_without_touching_the_rest() {
    let (mut app, dir) = presets_app("infer");
    press(&mut app, KeyCode::F(9));
    press(&mut app, KeyCode::Char('i'));
    let (message, _) = app.status.clone().unwrap();
    assert!(
        message.starts_with("this one is saved already"),
        "{message}"
    );
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('i'));

    let text = std::fs::read_to_string(dir.join(PRESETS_NAME)).unwrap();
    assert!(text.starts_with("# mine\n[bigmem]\nmem = 512G\n"), "{text}");
    assert!(text.contains("[short]\npartition  = short\n"), "{text}");
    assert!(app.preset_rows().iter().all(|row| !row.inferred));
    press(&mut app, KeyCode::Char('I'));
    assert!(
        app.status
            .as_ref()
            .unwrap()
            .0
            .starts_with("nothing to infer")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e_opens_the_file_at_the_preset_creating_it_if_needed() {
    let (mut app, dir) = presets_app("edit");
    press(&mut app, KeyCode::F(9));
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    let crate::app::Action::Edit { path, line } = action else {
        panic!("an edit");
    };
    assert_eq!(path, dir.join(PRESETS_NAME));
    assert_eq!(line, 2, "the [bigmem] section");

    std::fs::remove_file(dir.join(PRESETS_NAME)).unwrap();
    app.load_presets(Some(dir.clone()));
    let action = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert!(matches!(action, crate::app::Action::Edit { line: 1, .. }));
    let text = std::fs::read_to_string(dir.join(PRESETS_NAME)).unwrap();
    assert!(text.starts_with("# just-tui cluster presets"));
    let _ = std::fs::remove_dir_all(&dir);
}
