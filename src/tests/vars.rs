//! Overriding variables: qwe's bookmarks, the overrides file, the explorer,
//! and the `--set` every run is given.

use std::path::{Path, PathBuf};

use crate::app::{Action, App, Mode};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::just;
use crate::model::Justfile;
use crate::source::SourceCache;
use crate::vars::{Explorer, Overrides, parse_qwe, variable_keys};

/// `data := "data"` at the top, `out := "results"` in module `sub`.
const DUMP: &str = r#"{
  "aliases": {}, "first": "show", "doc": null,
  "assignments": {"data": {"export": false, "private": false, "value": "data"}},
  "modules": {
    "sub": {
      "aliases": {}, "first": "show", "doc": null, "modules": {}, "settings": {},
      "assignments": {"out": {"export": false, "private": false, "value": "results"}},
      "recipes": {"show": {"attributes": [], "body": [["echo"]], "dependencies": [],
        "doc": null, "name": "show", "namepath": "sub::show", "parameters": [],
        "priors": 0, "private": false, "quiet": false, "shebang": false}},
      "source": "SUB"
    }
  },
  "recipes": {"show": {"attributes": [], "body": [["echo"]], "dependencies": [],
    "doc": null, "name": "show", "namepath": "show", "parameters": [],
    "priors": 0, "private": false, "quiet": false, "shebang": false}},
  "settings": {}, "source": "ROOT"
}"#;

/// A project on disk, so the explorer has directories to show and the
/// overrides somewhere to be written.
fn project(name: &str) -> (App, PathBuf) {
    let base = std::env::temp_dir().join(format!("just-tui-vars-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for dir in ["data/genomes", "sub/results", "elsewhere"] {
        std::fs::create_dir_all(base.join(dir)).unwrap();
    }
    let dump = DUMP
        .replace("ROOT", &base.join("justfile").display().to_string())
        .replace("SUB", &base.join("sub/mod.just").display().to_string());
    let justfile: Justfile = serde_json::from_str(&dump).unwrap();
    let source = just::Loaded {
        justfile,
        working_dir: base.clone(),
        path: Some(base.join("justfile")),
        explicit_file: None,
        label: "proj".to_owned(),
        global: false,
    };
    (App::new(vec![source], SourceCache::default()), base)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn press(app: &mut App, codes: &[KeyCode]) -> Action {
    let mut last = Action::None;
    for code in codes {
        last = app.handle_key(key(*code));
    }
    last
}

#[test]
fn qwe_bookmarks_are_read_with_home_filled_in() {
    let text =
        "scratch\t$HOME/scratch\nbroken line\ngenomes\t/data/genomes\n\nproj\t$HOME/git/proj\n";
    let pins = parse_qwe(text, Path::new("/home/u"));
    assert_eq!(
        pins,
        [
            ("genomes".to_owned(), PathBuf::from("/data/genomes")),
            ("proj".to_owned(), PathBuf::from("/home/u/git/proj")),
            ("scratch".to_owned(), PathBuf::from("/home/u/scratch")),
        ],
        "sorted, the malformed line skipped"
    );
}

#[test]
fn an_override_becomes_a_set_only_while_its_variable_exists() {
    let dir = std::env::temp_dir().join(format!("just-tui-overrides-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut overrides = Overrides::load(&dir);
    overrides
        .set("sub::out", Some("/scratch/run1".into()))
        .unwrap();
    overrides.set("gone", Some("/x".into())).unwrap();

    // Read back from disk, as the next start would.
    let overrides = Overrides::load(&dir);
    let known = ["data".to_owned(), "sub::out".to_owned()];
    assert_eq!(
        overrides.set_args(&known),
        ["--set", "sub::out", "/scratch/run1"],
        "a variable the justfile no longer has would make just refuse to run"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn variable_keys_reach_into_modules() {
    let justfile: Justfile =
        serde_json::from_str(&DUMP.replace("ROOT", "/j").replace("SUB", "/s")).unwrap();
    assert_eq!(variable_keys(&justfile), ["data", "sub::out"]);
}

#[test]
fn picking_a_directory_sets_the_variable_for_every_run() {
    let (mut app, base) = project("pick");
    assert!(app.select_namepath("show"));

    // From a recipe, V opens the variables of the module it is in.
    press(&mut app, &[KeyCode::Char('V')]);
    assert_eq!(app.mode, Mode::Vars);
    let form = app.vars.as_ref().unwrap();
    assert_eq!(form.rows[0].key, "data");

    // The explorer starts where the value points: `data`, under the justfile.
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(app.mode, Mode::Explorer);
    assert_eq!(app.explorer.as_ref().unwrap().dir, base.join("data"));

    // Up, onto `elsewhere`, and choose it.
    press(&mut app, &[KeyCode::Left]);
    let explorer = app.explorer.as_ref().unwrap();
    assert_eq!(explorer.dir, base);
    assert_eq!(
        explorer.selected().unwrap().name,
        "data",
        "back up, the cursor is on the directory just left"
    );
    press(&mut app, &[KeyCode::Down, KeyCode::Char(' ')]);
    assert_eq!(app.mode, Mode::Vars);
    let chosen = base.join("elsewhere").display().to_string();
    assert_eq!(app.overrides.get("data"), Some(chosen.as_str()));

    // Every run now carries it, before the recipe name.
    press(&mut app, &[KeyCode::Esc]);
    let Action::Run { commands, .. } = press(&mut app, &[KeyCode::Char('r')]) else {
        panic!("a run");
    };
    assert_eq!(commands[0], ["--set", "data", chosen.as_str(), "show"]);

    // `d` drops it again.
    press(
        &mut app,
        &[KeyCode::Char('V'), KeyCode::Char('d'), KeyCode::Esc],
    );
    assert_eq!(app.overrides.get("data"), None);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_module_variable_is_set_by_its_full_path() {
    let (mut app, base) = project("module");
    assert!(app.select_namepath("sub::show"));
    press(&mut app, &[KeyCode::Char('V')]);
    let form = app.vars.as_ref().unwrap();
    assert_eq!(form.rows[0].key, "sub::out");

    // Relative to the module's own directory, where its recipes run.
    press(&mut app, &[KeyCode::Enter]);
    assert_eq!(app.explorer.as_ref().unwrap().dir, base.join("sub/results"));

    // `c` takes the directory being shown; typing works as well.
    press(&mut app, &[KeyCode::Char('c')]);
    let shown = base.join("sub/results").display().to_string();
    assert_eq!(app.overrides.get("sub::out"), Some(shown.as_str()));
    press(&mut app, &[KeyCode::Char('e')]);
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "/scratch/x".chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
    press(&mut app, &[KeyCode::Enter, KeyCode::Esc]);
    assert_eq!(app.overrides.get("sub::out"), Some("/scratch/x"));

    let Action::Run { commands, .. } = press(&mut app, &[KeyCode::Char('r')]) else {
        panic!("a run");
    };
    assert_eq!(
        commands[0],
        ["--set", "sub::out", "/scratch/x", "sub::show"]
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_explorer_starts_at_the_nearest_directory_that_exists() {
    let (mut app, base) = project("nearest");
    app.overrides
        .set(
            "data",
            Some(base.join("data/genomes/not/yet").display().to_string()),
        )
        .unwrap();
    assert!(app.select_namepath("show"));
    press(&mut app, &[KeyCode::Char('V'), KeyCode::Enter]);
    assert_eq!(
        app.explorer.as_ref().unwrap().dir,
        base.join("data/genomes")
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn hidden_entries_show_only_when_asked() {
    let dir = std::env::temp_dir().join(format!("just-tui-hidden-{}", std::process::id()));
    std::fs::create_dir_all(dir.join(".cache")).unwrap();
    std::fs::create_dir_all(dir.join("seen")).unwrap();
    let mut explorer = Explorer::open("x".into(), dir.clone(), Vec::new(), false);
    let names = |e: &Explorer| e.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&explorer), ["seen"]);
    explorer.toggle_hidden();
    assert_eq!(names(&explorer), [".cache", "seen"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_submission_carries_the_override_into_the_job() {
    let (mut app, base) = project("submit");
    app.cluster = Some(super::test_cluster());
    app.overrides
        .set("data", Some("/scratch/my run".into()))
        .unwrap();
    assert!(app.select_namepath("show"));
    press(&mut app, &[KeyCode::Char('s')]);
    let form = app.form.as_ref().expect("submit form");
    assert_eq!(form.just_args, ["--set", "data", "/scratch/my run"]);

    let args = crate::slurm::build_command(
        &form.base,
        &form.logs,
        &form.just_args,
        &form.namepath,
        &form.settings,
        None,
        None,
    );
    assert_eq!(
        args.last().unwrap(),
        "just --set data '/scratch/my run' show",
        "quoted for the shell sbatch runs the job in"
    );
    let _ = std::fs::remove_dir_all(&base);
}
