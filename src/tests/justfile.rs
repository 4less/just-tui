//! Reading justfiles: the JSON dump, the explorer tree, source extraction,
//! and the search filter.

use super::{fixture, fixture_source, fixture_tree, global_source};
use crate::just;
use crate::model::render_literal;
use crate::source::SourceCache;
use crate::tree::{Filter, Kind, Tree};

#[test]
fn parses_a_dump() {
    let file = fixture();
    assert_eq!(file.recipes.len(), 3);
    assert_eq!(file.modules["sub"].recipes["deep"].namepath, "sub::deep");
    assert_eq!(file.assignments["target"].value, serde_json::json!("out"));
    assert_eq!(file.first.as_deref(), Some("hello"));
}

#[test]
fn recipe_metadata_is_read() {
    let file = fixture();
    let hello = &file.recipes["hello"];
    assert_eq!(hello.signature(), "hello name=\"world\" *extra");
    assert_eq!(hello.group().as_deref(), Some("greet"));
    assert_eq!(hello.body_text().trim(), "echo \"hi {{ name }}\"");

    let (before, after) = file.recipes["build"].split_dependencies();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].recipe, "hello");
    assert_eq!(after[0].recipe, "after");
}

#[test]
fn literals_keep_their_quotes() {
    assert_eq!(render_literal(&serde_json::json!("x")), "\"x\"");
    assert_eq!(
        render_literal(&serde_json::json!(["variable", "v"])),
        "v".to_owned()
    );
}

#[test]
fn tree_nests_modules_under_a_root() {
    let tree = fixture_tree();
    let root = &tree.nodes[tree.roots[0]];
    assert!(root.is_root);
    assert_eq!(tree.count_modules(), 1, "the root is not a module");
    assert_eq!(tree.count_recipes(), 4);

    let deep = tree.find_namepath(0, "sub::deep").expect("nested recipe");
    assert_eq!(tree.nodes[deep].depth, 2);
    assert_eq!(tree.nodes[deep].kind, Kind::Recipe);

    let hello = tree.find_namepath(0, "hello").expect("recipe");
    assert!(tree.nodes[hello].is_default, "`first` marks the default");
}

#[test]
fn private_recipes_are_hidden_until_asked_for() {
    let tree = fixture_tree();
    let names = |private| {
        tree.visible(private, &Filter::default())
            .into_iter()
            .map(|id| tree.nodes[id].name.clone())
            .collect::<Vec<_>>()
    };
    assert!(!names(false).contains(&"_hidden".to_owned()));
    assert!(names(true).contains(&"_hidden".to_owned()));
}

#[test]
fn filter_is_fuzzy_on_names_and_literal_on_docs() {
    let filter = Filter::new("bld");
    assert!(filter.matches("build"));
    assert!(!filter.matches("clean"));
    assert!(!filter.contains("Build it"), "docs are not fuzzy");
    assert!(Filter::new("build it").contains("Build it"));
    assert_eq!(Filter::new("bd").match_indices("build"), vec![0, 4]);
}

#[test]
fn filtering_keeps_matches_reachable() {
    let tree = fixture_tree();
    let visible: Vec<String> = tree
        .visible(false, &Filter::new("deep"))
        .into_iter()
        .map(|id| tree.nodes[id].name.clone())
        .collect();
    // Root, the module holding the match, then the match itself.
    assert_eq!(visible, vec!["does-not-exist", "sub", "deep"]);
}

#[test]
fn source_extraction_grabs_comments_and_body() {
    let dir = std::env::temp_dir().join("just-tui-source-test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("justfile");
    std::fs::write(
        &path,
        "x := \"1\"\n\n\
         # First doc line\n\
         # Second doc line\n\
         [group('g')]\n\
         build target=\"debug\":\n\
         \x20   cargo build\n\
         \n\
         \x20   echo done\n\
         \n\
         other:\n\
         \x20   echo other\n",
    )
    .unwrap();

    let mut cache = SourceCache::default();
    let snippet = cache.recipe(&path, "build").expect("recipe found");
    assert_eq!(snippet.start_line, 3);
    assert_eq!(
        snippet.doc.as_deref(),
        Some("First doc line\nSecond doc line")
    );
    assert!(snippet.code.starts_with("# First doc line"));
    assert!(snippet.code.contains("[group('g')]"));
    assert!(snippet.code.trim_end().ends_with("echo done"));
    assert!(!snippet.code.contains("other:"), "stops at the next recipe");

    // `x := "1"` must not be mistaken for a recipe called `x`.
    assert!(cache.recipe(&path, "x").is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_global_library_is_searchable_by_its_file_name() {
    let tree = Tree::build(
        &[fixture_source(), global_source()],
        &mut SourceCache::default(),
    );
    let visible = |needle: &str| {
        tree.visible(false, &Filter::new(needle))
            .into_iter()
            .map(|id| tree.nodes[id].name.clone())
            .collect::<Vec<_>>()
    };

    // `st` lives in git.just, so both `git` and `git st` should find it.
    assert_eq!(visible("git"), vec!["git", "st"]);
    assert_eq!(visible("gitst"), vec!["git", "st"]);
    // A project-only match leaves the global root out entirely.
    assert!(!visible("hello").contains(&"git".to_owned()));
}

#[test]
fn global_sources_become_extra_roots() {
    let tree = Tree::build(
        &[fixture_source(), global_source()],
        &mut SourceCache::default(),
    );
    assert_eq!(tree.roots.len(), 2);

    let project = &tree.nodes[tree.roots[0]];
    let global = &tree.nodes[tree.roots[1]];
    assert!(
        !project.global && project.expanded,
        "the project starts open"
    );
    assert!(
        global.global && !global.expanded,
        "a global library starts closed"
    );
    assert_eq!(global.name, "git");

    // The same recipe name in two sources resolves to the right one.
    let st = tree.find_namepath(1, "st").expect("global recipe");
    assert_eq!(tree.nodes[st].root_index, 1);
    assert!(tree.nodes[st].global);
    assert!(tree.find_namepath(0, "st").is_none());
}

#[test]
fn a_global_recipe_runs_in_the_current_directory() {
    let global = global_source();
    let args = just::file_args(&global);
    assert_eq!(
        args,
        vec![
            "--justfile",
            "/home/u/.justx/git.just",
            // Without this, `just` would run the recipe in ~/.justx.
            "--working-directory",
            "/tmp/cwd",
        ]
    );

    // A project source needs neither flag: just finds the file itself.
    assert!(just::file_args(&fixture_source()).is_empty());
}
