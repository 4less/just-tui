//! Test fixtures shared by the suites below.

mod app;
mod cluster;
mod highlight;
mod jobs;
mod justfile;

use std::path::PathBuf;

use crate::app::App;
use crate::just;
use crate::model::Justfile;
use crate::slurm::{Cluster, Partition};
use crate::source::SourceCache;
use crate::tree::Tree;

const DUMP: &str = r#"{
  "aliases": {"b": {"attributes": [], "name": "b", "target": "build"}},
  "assignments": {"target": {"export": false, "name": "target", "private": false, "value": "out"}},
  "first": "hello",
  "doc": null,
  "modules": {
    "sub": {
      "aliases": {}, "assignments": {}, "modules": {}, "first": "deep",
      "recipes": {"deep": {"attributes": [], "body": [["echo deep"]], "dependencies": [],
        "doc": "nested recipe", "name": "deep", "namepath": "sub::deep", "parameters": [],
        "priors": 0, "private": false, "quiet": false, "shebang": false}},
      "settings": {}, "source": "/tmp/does-not-exist/sub/mod.just"
    }
  },
  "recipes": {
    "_hidden": {"attributes": [], "body": [["echo x"]], "dependencies": [], "doc": null,
      "name": "_hidden", "namepath": "_hidden", "parameters": [], "priors": 0,
      "private": true, "quiet": false, "shebang": false},
    "build": {"attributes": [], "body": [["cargo build"]],
      "dependencies": [{"arguments": [], "recipe": "hello"}, {"arguments": [], "recipe": "after"}],
      "doc": "Build it", "name": "build", "namepath": "build", "parameters": [],
      "priors": 1, "private": false, "quiet": false, "shebang": false},
    "hello": {"attributes": [{"group": "greet"}], "body": [["echo \"hi ", [["variable", "name"]], "\""]],
      "dependencies": [], "doc": "Say hi", "name": "hello", "namepath": "hello",
      "parameters": [{"default": "world", "export": false, "kind": "singular", "name": "name"},
                     {"default": null, "export": false, "kind": "star", "name": "extra"}],
      "priors": 0, "private": false, "quiet": false, "shebang": false}
  },
  "settings": {"export": true},
  "source": "/tmp/does-not-exist/justfile"
}"#;

pub fn fixture() -> Justfile {
    serde_json::from_str(DUMP).expect("fixture parses")
}

pub fn fixture_source() -> just::Loaded {
    just::Loaded {
        justfile: fixture(),
        working_dir: PathBuf::from("/tmp/does-not-exist"),
        path: Some(PathBuf::from("/tmp/does-not-exist/justfile")),
        explicit_file: None,
        label: "does-not-exist".to_owned(),
        global: false,
    }
}

/// A second source standing in for `~/.justx/git.just`.
pub fn global_source() -> just::Loaded {
    let mut file = Justfile::default();
    file.recipes.insert(
        "st".to_owned(),
        serde_json::from_str(
            r#"{"attributes":[],"body":[["git status"]],"dependencies":[],"doc":"Show status",
                "name":"st","namepath":"st","parameters":[],"priors":0,"private":false,
                "quiet":false,"shebang":false}"#,
        )
        .unwrap(),
    );
    just::Loaded {
        justfile: file,
        working_dir: PathBuf::from("/tmp/cwd"),
        path: Some(PathBuf::from("/home/u/.justx/git.just")),
        explicit_file: Some(PathBuf::from("/home/u/.justx/git.just")),
        label: "git".to_owned(),
        global: true,
    }
}

pub fn fixture_app() -> App {
    App::new(vec![fixture_source()], SourceCache::default())
}

pub fn fixture_tree() -> Tree {
    Tree::build(&[fixture_source()], &mut SourceCache::default())
}

/// A one-partition cluster with tight limits, for warning checks.
pub fn test_cluster() -> Cluster {
    Cluster {
        partitions: vec![Partition {
            name: "short".into(),
            is_default: true,
            max_time: Some(120),
            max_mem_mb: Some(64 * 1024),
            node_cpus: Some(16),
            ..Default::default()
        }],
        detected: true,
        ..Default::default()
    }
}
