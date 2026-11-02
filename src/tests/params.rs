//! The run-with-arguments form: when it opens, what it runs, and what it
//! remembers.

use std::path::PathBuf;

use crate::app::{Action, App, Mode};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::just;
use crate::model::Justfile;
use crate::source::SourceCache;

/// `greet who greeting="hi"`, and `plain` with no parameters.
const DUMP: &str = r#"{
  "aliases": {}, "assignments": {}, "first": "greet", "doc": null, "modules": {},
  "recipes": {
    "greet": {"attributes": [], "body": [["echo hi"]], "dependencies": [], "doc": null,
      "name": "greet", "namepath": "greet",
      "parameters": [{"default": null, "export": false, "kind": "singular", "name": "who"},
                     {"default": "hi", "export": false, "kind": "singular", "name": "greeting"}],
      "priors": 0, "private": false, "quiet": false, "shebang": false},
    "plain": {"attributes": [], "body": [["echo plain"]], "dependencies": [], "doc": null,
      "name": "plain", "namepath": "plain", "parameters": [], "priors": 0,
      "private": false, "quiet": false, "shebang": false}
  },
  "settings": {}, "source": "/tmp/does-not-exist/justfile"
}"#;

/// An app whose project lives in a fresh directory, so remembered
/// arguments have somewhere to be written.
fn app_in(dir: &str) -> (App, PathBuf) {
    let base = std::env::temp_dir().join(format!("just-tui-params-{dir}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let justfile: Justfile = serde_json::from_str(DUMP).unwrap();
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

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
}

#[test]
fn enter_asks_for_a_required_argument_and_remembers_it() {
    let (mut app, base) = app_in("enter");
    assert!(app.select_namepath("greet"));

    // Enter opens the form instead of running a recipe that would fail.
    assert!(matches!(app.handle_key(key(KeyCode::Enter)), Action::None));
    assert_eq!(app.mode, Mode::Args);

    // Left empty, the required row keeps the form open.
    assert!(matches!(app.handle_key(key(KeyCode::Enter)), Action::None));
    assert_eq!(app.mode, Mode::Args);

    type_text(&mut app, "the world");
    let Action::Run { commands, .. } = app.handle_key(key(KeyCode::Enter)) else {
        panic!("a run");
    };
    assert_eq!(
        commands[0],
        ["greet", "the world"],
        "one argument, space and all"
    );
    assert_eq!(app.mode, Mode::Normal);

    // The next time, the form opens the way it was left.
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.params.as_ref().unwrap().values, ["the world", ""]);

    // And so it does after a restart, from the file beside the justfile.
    let reopened = App::new(
        vec![just::Loaded {
            justfile: serde_json::from_str(DUMP).unwrap(),
            working_dir: base.clone(),
            path: Some(base.join("justfile")),
            explicit_file: None,
            label: "proj".to_owned(),
            global: false,
        }],
        SourceCache::default(),
    );
    assert_eq!(reopened.run_args.get("greet").unwrap(), ["the world", ""]);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_recipe_that_needs_nothing_runs_at_once_and_a_still_asks() {
    let (mut app, base) = app_in("plain");
    assert!(app.select_namepath("plain"));
    let Action::Run { commands, .. } = app.handle_key(key(KeyCode::Enter)) else {
        panic!("runs straight away");
    };
    assert_eq!(commands[0], ["plain"]);

    // `a` gives one free row, for whatever should follow the recipe name.
    app.handle_key(key(KeyCode::Char('a')));
    assert_eq!(app.mode, Mode::Args);
    type_text(&mut app, "x=1 'two words'");
    let Action::Run { commands, .. } = app.handle_key(key(KeyCode::Enter)) else {
        panic!("a run");
    };
    assert_eq!(commands[0], ["plain", "x=1", "two words"]);

    // Esc leaves without running.
    app.handle_key(key(KeyCode::Char('a')));
    assert!(matches!(app.handle_key(key(KeyCode::Esc)), Action::None));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.params.is_none());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_form_is_drawn_in_the_middle_of_the_screen() {
    let (mut app, base) = app_in("draw");
    assert!(app.select_namepath("greet"));
    app.handle_key(key(KeyCode::Enter));

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows: Vec<String> = (0..30)
        .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();

    let title = rows
        .iter()
        .position(|r| r.contains("Run greet"))
        .expect("form shown");
    assert!((8..20).contains(&title), "near the middle, got row {title}");
    assert!(
        rows.iter()
            .any(|r| r.contains("who") && r.contains("required"))
    );
    assert!(
        !rows[29].contains("just greet"),
        "not a prompt in the footer"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// A project whose recipes run in `sub/` (`set working-directory := "sub"`),
/// with `view path="data/default.sam"` and a file to pick in `sub/data/`.
fn picker_project(name: &str) -> (App, PathBuf) {
    let base = std::env::temp_dir().join(format!("just-tui-pick-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("sub/data")).unwrap();
    std::fs::create_dir_all(base.join("elsewhere")).unwrap();
    std::fs::write(base.join("sub/data/x.sam"), "").unwrap();
    std::fs::write(base.join("elsewhere/y.sam"), "").unwrap();
    let dump = DUMP
        .replace(
            r#"[{"default": null, "export": false, "kind": "singular", "name": "who"},
                     {"default": "hi", "export": false, "kind": "singular", "name": "greeting"}]"#,
            r#"[{"default": "data/default.sam", "export": false, "kind": "singular", "name": "path"}]"#,
        )
        .replace(r#""settings": {}"#, r#""settings": {"working_directory": "sub"}"#);
    let source = just::Loaded {
        justfile: serde_json::from_str(&dump).unwrap(),
        working_dir: base.clone(),
        path: Some(base.join("justfile")),
        explicit_file: None,
        label: "proj".to_owned(),
        global: false,
    };
    (App::new(vec![source], SourceCache::default()), base)
}

fn ctrl_o() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)
}

#[test]
fn ctrl_o_in_the_run_form_picks_a_path_relative_to_where_the_recipe_runs() {
    use crate::vars::PickFor;
    let (mut app, base) = picker_project("relative");
    let run_dir = base.join("sub");
    assert!(app.select_namepath("greet"));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Args);

    // An empty row starts the explorer where the recipe runs, not in the
    // directory of the justfile.
    app.handle_key(ctrl_o());
    assert_eq!(app.mode, Mode::Explorer);
    let explorer = app.explorer.as_mut().unwrap();
    assert_eq!(explorer.dir, run_dir);
    assert_eq!(
        explorer.target,
        PickFor::Param {
            row: 0,
            base: run_dir.clone()
        }
    );

    // Into data/, onto x.sam, choose it: back in the form, written relative.
    explorer.go(run_dir.join("data"));
    explorer.focus_on(&run_dir.join("data/x.sam"));
    app.handle_key(key(KeyCode::Char(' ')));
    assert_eq!(app.mode, Mode::Args);
    assert_eq!(app.params.as_ref().unwrap().values, ["data/x.sam"]);

    // Opening again lands on the file the row names; Esc leaves it as it was.
    app.handle_key(ctrl_o());
    let explorer = app.explorer.as_ref().unwrap();
    assert_eq!(explorer.dir, run_dir.join("data"));
    assert_eq!(explorer.selected().unwrap().name, "x.sam");
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Args, "esc goes back to the run form");
    assert_eq!(app.params.as_ref().unwrap().values, ["data/x.sam"]);

    let Action::Run { commands, .. } = app.handle_key(key(KeyCode::Enter)) else {
        panic!("a run");
    };
    assert_eq!(commands[0], ["greet", "data/x.sam"]);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_path_outside_the_run_directory_is_written_in_full() {
    let (mut app, base) = picker_project("outside");
    assert!(app.select_namepath("greet"));
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(ctrl_o());
    let explorer = app.explorer.as_mut().unwrap();
    explorer.go(base.join("elsewhere"));
    explorer.focus_on(&base.join("elsewhere/y.sam"));
    app.handle_key(key(KeyCode::Enter)); // Enter on a file chooses it too
    assert_eq!(
        app.params.as_ref().unwrap().values,
        [base.join("elsewhere/y.sam").display().to_string()]
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn picks_into_a_free_row_add_words_and_quote_spaces() {
    let (mut app, base) = picker_project("free");
    std::fs::write(base.join("sub/data/two words.sam"), "").unwrap();
    assert!(app.select_namepath("plain"));
    app.handle_key(key(KeyCode::Char('a')));
    type_text(&mut app, "x=1");
    for file in ["x.sam", "two words.sam"] {
        app.handle_key(ctrl_o());
        let explorer = app.explorer.as_mut().unwrap();
        explorer.go(base.join("sub/data"));
        explorer.focus_on(&base.join("sub/data").join(file));
        app.handle_key(key(KeyCode::Char(' ')));
    }
    assert_eq!(
        app.params.as_ref().unwrap().values,
        ["x=1 data/x.sam 'data/two words.sam'"]
    );
    let Action::Run { commands, .. } = app.handle_key(key(KeyCode::Enter)) else {
        panic!("a run");
    };
    assert_eq!(
        commands[0],
        ["plain", "x=1", "data/x.sam", "data/two words.sam"]
    );
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn lexical_paths_resolve_dots_without_the_disk() {
    use crate::params::lexical;
    use std::path::Path;
    assert_eq!(
        lexical(Path::new("/p/just/../data/./x")),
        Path::new("/p/data/x")
    );
    assert_eq!(lexical(Path::new("../a/../b")), Path::new("../b"));
}
