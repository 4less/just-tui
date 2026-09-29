//! Dependency chains: the graph from the justfile, the chooser, and the
//! `sbatch` flags that order the jobs.

use std::path::Path;

use super::{fixture_app, test_cluster};
use crate::app::{App, Mode};
use crate::chain::Chain;
use crate::config::ConfigFile;
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::just;
use crate::model::Justfile;
use crate::slurm::{self, Settings};
use crate::source::SourceCache;
use crate::tree::Tree;

/// `align: (fetch "hg38") sub::index && report`, with `sub::index: tree`.
/// Just writes the bare `index` for the cross-module dependency, and the
/// literal argument as a plain string, exactly as `just --dump` does.
const PIPELINE: &str = r#"{
  "aliases": {}, "assignments": {}, "first": "align", "doc": null,
  "modules": {
    "sub": {
      "aliases": {}, "assignments": {}, "modules": {}, "first": "index",
      "recipes": {
        "index": {"attributes": [], "body": [["echo index"]],
          "dependencies": [{"arguments": [], "recipe": "tree"}], "doc": null,
          "name": "index", "namepath": "sub::index", "parameters": [], "priors": 1,
          "private": false, "quiet": false, "shebang": false},
        "tree": {"attributes": [], "body": [["echo tree"]], "dependencies": [], "doc": null,
          "name": "tree", "namepath": "sub::tree", "parameters": [], "priors": 0,
          "private": false, "quiet": false, "shebang": false}
      },
      "settings": {}, "source": "/tmp/does-not-exist/sub/mod.just"
    }
  },
  "recipes": {
    "align": {"attributes": [], "body": [["echo align"]],
      "dependencies": [{"arguments": ["hg38"], "recipe": "fetch"},
                       {"arguments": [], "recipe": "index"},
                       {"arguments": [], "recipe": "report"}],
      "doc": null, "name": "align", "namepath": "align", "parameters": [], "priors": 2,
      "private": false, "quiet": false, "shebang": false},
    "fetch": {"attributes": [], "body": [["echo fetch"]], "dependencies": [], "doc": null,
      "name": "fetch", "namepath": "fetch",
      "parameters": [{"default": "a", "export": false, "kind": "singular", "name": "x"}],
      "priors": 0, "private": false, "quiet": false, "shebang": false},
    "report": {"attributes": [], "body": [["echo report"]], "dependencies": [], "doc": null,
      "name": "report", "namepath": "report", "parameters": [], "priors": 0,
      "private": false, "quiet": false, "shebang": false},
    "solo": {"attributes": [], "body": [["echo solo"]], "dependencies": [], "doc": null,
      "name": "solo", "namepath": "solo", "parameters": [], "priors": 0,
      "private": false, "quiet": false, "shebang": false}
  },
  "settings": {}, "source": "/tmp/does-not-exist/justfile"
}"#;

fn pipeline() -> just::Loaded {
    let justfile: Justfile = serde_json::from_str(PIPELINE).expect("fixture parses");
    just::Loaded {
        justfile,
        working_dir: std::path::PathBuf::from("/tmp/does-not-exist"),
        path: Some(std::path::PathBuf::from("/tmp/does-not-exist/justfile")),
        explicit_file: None,
        label: "does-not-exist".to_owned(),
        global: false,
    }
}

fn pipeline_tree() -> Tree {
    Tree::build(&[pipeline()], &mut SourceCache::default())
}

fn labels(chain: &Chain) -> Vec<String> {
    chain.links.iter().map(|link| link.label()).collect()
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

#[test]
fn a_chain_orders_every_recipe_after_what_it_waits_on() {
    let tree = pipeline_tree();
    let align = tree.find_namepath(0, "align").unwrap();
    let chain = Chain::build(&tree, 0, align).expect("builds");

    assert_eq!(
        labels(&chain),
        ["fetch hg38", "sub::tree", "sub::index", "align", "report"],
        "priors first in the order written, the head, then the subsequent"
    );
    let position = |name: &str| chain.links.iter().position(|l| l.namepath == name).unwrap();
    let after = |name: &str| chain.links[position(name)].after.clone();
    assert_eq!(after("fetch"), Vec::<usize>::new());
    assert_eq!(after("sub::index"), vec![position("sub::tree")]);
    assert_eq!(
        after("align"),
        vec![position("fetch"), position("sub::index")]
    );
    assert_eq!(
        after("report"),
        vec![position("align")],
        "`&& report` runs after align, so it waits on align"
    );
}

#[test]
fn a_recipe_without_dependencies_is_a_chain_of_one() {
    let tree = pipeline_tree();
    let solo = tree.find_namepath(0, "solo").unwrap();
    let chain = Chain::build(&tree, 0, solo).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(labels(&chain), ["solo"]);
}

#[test]
fn a_dependency_with_a_literal_argument_carries_it_as_args() {
    let tree = pipeline_tree();
    let align = tree.find_namepath(0, "align").unwrap();
    let chain = Chain::build(&tree, 0, align).unwrap();
    let fetch = chain.links.iter().find(|l| l.namepath == "fetch").unwrap();
    assert_eq!(fetch.args, "hg38");
}

#[test]
fn a_dependency_just_cannot_name_is_an_error() {
    let mut tree = pipeline_tree();
    // Point `align` at something that is not there.
    let align = tree.find_namepath(0, "align").unwrap();
    let info = tree.nodes[align].info.as_mut().unwrap();
    info.recipe.dependencies[1].recipe = "missing".to_owned();
    let problem = Chain::build(&tree, 0, align).unwrap_err();
    assert!(problem.contains("missing"), "got {problem}");

    // And an argument that needs evaluating cannot travel as text.
    let mut tree = pipeline_tree();
    let align = tree.find_namepath(0, "align").unwrap();
    let info = tree.nodes[align].info.as_mut().unwrap();
    info.recipe.dependencies[0].arguments = vec![serde_json::json!(["variable", "genome"])];
    let problem = Chain::build(&tree, 0, align).unwrap_err();
    assert!(problem.contains("not a literal"), "got {problem}");
}

#[test]
fn a_chained_job_waits_with_afterok_and_runs_without_deps() {
    let settings = Settings {
        partition: "qib-compute".into(),
        ..Default::default()
    };
    let after = ["23474901".to_owned(), "23474903".to_owned()];
    let args = slurm::build_command(Path::new("/work"), "align", &settings, None, Some(&after));

    assert!(
        args.contains(&"--dependency=afterok:23474901:23474903".to_owned()),
        "got {args:?}"
    );
    assert!(
        args.contains(&"--kill-on-invalid-dep=yes".to_owned()),
        "a job that can never start is cancelled, not left pending"
    );
    let wrap = args.iter().position(|a| a == "--wrap").unwrap();
    assert_eq!(args[wrap + 1], "just --no-deps align");

    // The first link of a chain waits on nothing but still skips its deps.
    let first = slurm::build_command(Path::new("/work"), "fetch", &settings, None, Some(&[]));
    assert!(!first.iter().any(|a| a.starts_with("--dependency")));
    assert!(first.contains(&"just --no-deps fetch".to_owned()));

    // A job submitted alone is untouched.
    let alone = slurm::build_command(Path::new("/work"), "align", &settings, None, None);
    assert!(alone.contains(&"just align".to_owned()));
    assert!(!alone.iter().any(|a| a.contains("no-deps")));
}

#[test]
fn the_state_file_keeps_which_links_were_skipped() {
    let parsed = ConfigFile::parse(
        Path::new("state"),
        "[align]\nmem = 8G\nskip = fetch hg38, sub::tree\n\n[solo]\ncpus = 2\n",
    );
    assert_eq!(parsed.skips["align"], ["fetch hg38", "sub::tree"]);
    assert!(!parsed.skips.contains_key("solo"));
    assert_eq!(
        parsed.recipes["align"].mem, "8G",
        "skip does not disturb the settings"
    );
}

/// An app on the pipeline fixture, with `align` selected.
fn pipeline_app() -> App {
    let mut app = App::new(vec![pipeline()], SourceCache::default());
    app.cluster = Some(test_cluster());
    assert!(app.select_namepath("align"));
    app
}

#[test]
fn submitting_a_recipe_with_dependencies_opens_the_chooser() {
    let mut app = pipeline_app();
    app.open_submit();

    assert_eq!(app.mode, Mode::Chain);
    let chain = app.chain.as_ref().expect("chooser is open");
    assert_eq!(chain.head, "align");
    assert_eq!(chain.rows.len(), 5);
    assert!(
        chain.rows.iter().all(|row| row.checked),
        "everything starts checked"
    );
    assert_eq!(chain.rows[0].link.label(), "fetch hg38");
    assert_eq!(
        chain.rows[0].settings.args, "hg38",
        "the argument lands in args"
    );
    assert!(
        chain.rows.iter().all(|row| row.needs_form()),
        "nothing is configured yet"
    );

    // A recipe without dependencies still goes straight to the form.
    let mut app = pipeline_app();
    assert!(app.select_namepath("solo"));
    app.open_submit();
    assert_eq!(app.mode, Mode::Submit);
    assert!(app.chain.is_none());
}

#[test]
fn the_chooser_walks_the_rows_that_need_settings() {
    let mut app = pipeline_app();
    app.open_submit();

    // Unchecking a row drops it, and the rows after it stop waiting on it.
    app.handle_key(key(KeyCode::Char('n')));
    assert_eq!(app.chain.as_ref().unwrap().checked(), 0);
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.mode,
        Mode::Chain,
        "nothing checked, nothing to continue with"
    );
    app.handle_key(key(KeyCode::Char('a')));
    app.handle_key(key(KeyCode::Char('k')));
    app.handle_key(key(KeyCode::Char(' ')));
    {
        let chain = app.chain.as_ref().unwrap();
        let unchecked = chain.rows.iter().position(|row| !row.checked).unwrap();
        assert_eq!(chain.rows[unchecked].link.namepath, "align");
        let report = chain
            .rows
            .iter()
            .position(|row| row.link.namepath == "report")
            .unwrap();
        assert!(
            chain.waits_on(report).is_empty(),
            "an unchecked upstream is declared done"
        );
        assert_eq!(chain.skipped(), ["align"]);
    }

    // Enter opens the form for the first row without settings…
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Submit);
    let form = app.form.as_ref().expect("form for the first row");
    assert_eq!(form.namepath, "fetch");
    assert_eq!(form.settings.args, "hg38");
    assert_eq!(app.chain.as_ref().unwrap().step, Some(0));

    // …and Esc goes back to the chooser without losing anything.
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Chain);
    assert!(app.form.is_none());
    assert_eq!(app.chain.as_ref().unwrap().skipped(), ["align"]);

    // Accepting a form moves to the next row that needs one, and the last
    // acceptance lands on the confirmation with every row edited.
    app.handle_key(key(KeyCode::Enter));
    for expected in ["fetch", "sub::tree", "sub::index", "report"] {
        assert_eq!(app.mode, Mode::Submit);
        assert_eq!(app.form.as_ref().unwrap().namepath, expected);
        app.handle_key(key(KeyCode::Enter));
    }
    assert_eq!(app.mode, Mode::ConfirmChain);
    let chain = app.chain.as_ref().unwrap();
    assert_eq!(chain.rows.iter().filter(|row| row.edited).count(), 4);
    assert_eq!(chain.needing_form(), 0);

    // Anything but y goes back to the chooser.
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Chain);
}

#[test]
fn the_chooser_remembers_what_was_unchecked_last_time() {
    let mut app = pipeline_app();
    let skipped = ["sub::tree".to_owned()];
    // Writing the state file fails on a directory that does not exist; the
    // in-memory copy is what the chooser reads.
    let _ = app.configs.remember_skipped("align", &skipped);
    app.open_submit();
    let chain = app.chain.as_ref().unwrap();
    let tree = chain
        .rows
        .iter()
        .find(|row| row.link.namepath == "sub::tree")
        .unwrap();
    assert!(!tree.checked, "left the way it was");
    assert!(chain.rows.iter().filter(|row| row.checked).count() == 4);
}

#[test]
fn the_fixture_build_recipe_is_a_chain_too() {
    // `build: hello && post` in the shared fixture.
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit();
    assert_eq!(app.mode, Mode::Chain);
    let chain = app.chain.as_ref().unwrap();
    assert_eq!(
        chain
            .rows
            .iter()
            .map(|row| row.link.namepath.as_str())
            .collect::<Vec<_>>(),
        ["hello", "build", "post"]
    );
}

/// The screen as text, for checking what an overlay says.
fn screen(app: &mut App, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_chooser_and_the_confirmation_render() {
    let mut app = pipeline_app();
    app.open_submit();
    app.handle_key(key(KeyCode::Char('k')));
    app.handle_key(key(KeyCode::Char(' ')));
    let text = screen(&mut app, 100, 30);
    assert!(text.contains("Submit align and what it runs with"));
    assert!(
        text.contains("[x]  1  fetch hg38"),
        "rows are numbered in submission order"
    );
    assert!(
        text.contains("[ ]  4  align"),
        "the unchecked row shows as such"
    );
    assert!(
        text.contains("↳ after 1, 3"),
        "align waits on fetch and index"
    );
    assert!(text.contains("4 of 5 jobs"));
    assert!(text.contains("will ask"));

    // Give every row a setting so the walk goes straight to the confirmation.
    app.handle_key(key(KeyCode::Char('a')));
    for row in &mut app.chain.as_mut().unwrap().rows {
        row.settings.mem = "8G".into();
    }
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::ConfirmChain);
    let text = screen(&mut app, 120, 40);
    assert!(text.contains("Submit 5 jobs in this order?"));
    assert!(
        text.contains("afterok:<fetch>:<index>"),
        "placeholders stand in for job ids"
    );
    assert!(text.contains("just --no-deps fetch hg38"));

    // The form for one row names its place in the walk. The cursor is still
    // on align from the toggle above.
    app.handle_key(key(KeyCode::Esc));
    app.handle_key(key(KeyCode::Char('f')));
    assert_eq!(app.mode, Mode::Submit);
    let text = screen(&mut app, 100, 40);
    assert!(text.contains("step 4 of 5 · align · waits on fetch, index"));
    assert!(text.contains("afterok:<fetch>:<index>"));
    assert!(text.contains("done: fetch · tree · index    next: report"));
    assert!(text.contains("esc back to the chain"));
}
