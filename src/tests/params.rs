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
