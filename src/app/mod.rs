//! Application state. Behaviour lives in the sibling modules:
//! [`navigate`] for the explorer, [`keys`] for input, and [`cluster`]
//! for the Slurm submit form.

mod cluster;
mod keys;
mod navigate;

use std::path::PathBuf;

use ratatui::widgets::ListState;

use crate::config::Configs;
use crate::just::{self, Loaded};
use crate::slurm::Cluster;
use crate::source::SourceCache;
use crate::submit::SubmitForm;
use crate::tree::{Filter, Kind, Tree};
use crate::ui::Panes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Tree,
    Doc,
    Code,
}

impl Pane {
    pub fn next(self) -> Self {
        match self {
            Pane::Tree => Pane::Doc,
            Pane::Doc => Pane::Code,
            Pane::Code => Pane::Tree,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Pane::Tree => Pane::Code,
            Pane::Doc => Pane::Tree,
            Pane::Code => Pane::Doc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Args,
    Help,
    Submit,
    /// Choosing which `.just-tui-cluster-config` to open in an editor.
    ConfigPick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

/// Work the event loop has to do outside the alternate screen.
pub enum Action {
    None,
    Quit,
    Run {
        args: Vec<String>,
        dir: PathBuf,
        dry: bool,
    },
    Edit {
        path: PathBuf,
        line: usize,
    },
    Copy(String),
}

pub struct App {
    /// The project justfile first, then any global recipe files.
    pub sources: Vec<Loaded>,
    pub tree: Tree,
    pub cache: SourceCache,
    pub visible: Vec<usize>,
    pub list_state: ListState,
    pub selected_id: Option<usize>,
    pub focus: Pane,
    pub mode: Mode,
    pub search: String,
    pub args_input: String,
    pub show_private: bool,
    pub doc_scroll: u16,
    pub code_scroll: u16,
    pub code_height: u16,
    pub doc_height: u16,
    pub code_lines: usize,
    pub doc_lines: usize,
    pub zoom: bool,
    pub wrap_code: bool,
    pub vertical_split: bool,
    pub status: Option<(String, StatusKind)>,
    pub quit: bool,
    /// Where each pane was drawn last frame, for mouse hit-testing.
    pub panes: Panes,
    pub configs: Configs,
    /// Detected lazily, the first time the submit form is opened.
    pub cluster: Option<Cluster>,
    pub form: Option<SubmitForm>,
}

impl App {
    pub fn new(sources: Vec<Loaded>, mut cache: SourceCache) -> Self {
        let tree = Tree::build(&sources, &mut cache);
        let configs = Configs::new(&sources[0].working_dir);
        let mut app = Self {
            sources,
            tree,
            cache,
            visible: Vec::new(),
            list_state: ListState::default(),
            selected_id: None,
            focus: Pane::Tree,
            mode: Mode::Normal,
            search: String::new(),
            args_input: String::new(),
            show_private: false,
            doc_scroll: 0,
            code_scroll: 0,
            code_height: 10,
            doc_height: 10,
            code_lines: 0,
            doc_lines: 0,
            zoom: false,
            wrap_code: false,
            vertical_split: true,
            status: None,
            quit: false,
            panes: Panes::default(),
            configs,
            cluster: None,
            form: None,
        };
        app.refresh_visible();
        app.select_first_recipe();
        app
    }

    /// The project source; config files and state live beside it.
    pub fn project(&self) -> &Loaded {
        &self.sources[0]
    }

    /// The source the selected node came from.
    pub fn selected_source(&self) -> &Loaded {
        let index = self.selected().map_or(0, |n| n.root_index);
        self.sources.get(index).unwrap_or(&self.sources[0])
    }

    pub fn filter(&self) -> Filter {
        Filter::new(&self.search)
    }

    /// Select a row by its position in the visible list, for mouse clicks.
    /// Jump to the next or previous recipe, skipping module headers.
    pub fn reload(&mut self) {
        let selected = self.selected().map(|n| (n.root_index, n.namepath.clone()));

        let mut reloaded = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            match just::load(&source.working_dir, source.explicit_file.as_deref()) {
                Ok(mut fresh) => {
                    fresh.label = source.label.clone();
                    fresh.global = source.global;
                    fresh.working_dir = source.working_dir.clone();
                    reloaded.push(fresh);
                }
                Err(err) => {
                    self.error(format!("reload failed: {err}"));
                    return;
                }
            }
        }

        self.cache.invalidate();
        self.sources = reloaded;
        self.tree = Tree::build(&self.sources, &mut self.cache);
        self.refresh_visible();
        if let Some(id) = selected.and_then(|(root, path)| self.tree.find_namepath(root, &path)) {
            self.selected_id = Some(id);
            self.refresh_visible();
        }
        self.configs = Configs::new(&self.project().working_dir);
        self.refresh_form();
        self.info("reloaded");
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), StatusKind::Info));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), StatusKind::Error));
    }

    /// The target a run should use: aliases resolve to the recipe they point at.
    pub fn run_target(&self) -> Option<String> {
        let node = self.selected()?;
        match node.kind {
            Kind::Recipe => Some(node.namepath.clone()),
            Kind::Alias => Some(node.namepath.clone()),
            Kind::Module => None,
        }
    }

    pub(super) fn build_run(&mut self, extra: &str, dry: bool) -> Action {
        let Some(target) = self.run_target() else {
            self.error("select a recipe to run");
            return Action::None;
        };
        let source = self.selected_source();
        let dir = source.working_dir.clone();
        let mut args = just::file_args(source);
        if dry {
            args.push("--dry-run".into());
        }
        args.push(target);
        args.extend(
            extra
                .split_whitespace()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
        Action::Run { args, dir, dry }
    }
}
