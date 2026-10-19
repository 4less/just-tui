//! Application state. Behaviour lives in the sibling modules:
//! [`navigate`] for the explorer, [`keys`] for input, and [`cluster`]
//! for the Slurm submit form.

mod cluster;
mod jobs;
mod keys;
mod navigate;
mod recall;

use std::path::PathBuf;

use ratatui::widgets::ListState;

use crate::config::Configs;
use crate::history::History;
use crate::just::{self, Loaded};
use crate::params::{ParamForm, RunArgs, quote};
use crate::slurm::Cluster;
use crate::source::SourceCache;
use crate::submit::{ChainForm, SubmitForm};
use crate::tree::{Filter, Kind, Tree};
use crate::ui::Panes;

pub use jobs::{JobsView, LogKind, PendingAction};
pub use recall::HistoryPick;

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
    /// Choosing which links of a dependency chain go out, before their forms.
    Chain,
    /// The chain's `sbatch` lines, waiting for a `y`.
    ConfirmChain,
    /// Choosing which `.just-tui-cluster-config` to open in an editor.
    ConfigPick,
    /// Browsing Slurm jobs and their logs.
    Jobs,
    /// Confirming something that touches the queue: a kill, a rerun, or both.
    ConfirmJob,
    /// Choosing which job states the list shows.
    JobFilter,
    /// Browsing past submissions, to load one back into the form.
    History,
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
    /// One `just` invocation per entry, run in order and stopped at the
    /// first failure. A recipe alone is one entry; a chain is several.
    Run {
        commands: Vec<Vec<String>>,
        dir: PathBuf,
        dry: bool,
    },
    Edit {
        path: PathBuf,
        line: usize,
    },
    /// Show a file without offering to reload the justfile afterwards.
    View {
        path: PathBuf,
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
    /// The run-with-arguments form, while it is open.
    pub params: Option<ParamForm>,
    /// The open form was asked for by a dry run.
    pub params_dry: bool,
    /// What each recipe's form held when it was last run.
    pub run_args: RunArgs,
    /// The global library's directory. Global recipes keep their state,
    /// run arguments, history and logs there, so the same memory follows
    /// them into every directory. `None` keeps them with the project's.
    pub global_dir: Option<PathBuf>,
    /// [`Self::configs`] with the global library's state file.
    pub global_configs: Configs,
    pub global_run_args: RunArgs,
    pub show_private: bool,
    /// Show `[group('x')]` folders inside modules that are divided by one.
    pub group_layer: bool,
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
    /// Detection in flight, so the form need not wait for it.
    pub detecting: Option<std::sync::mpsc::Receiver<Cluster>>,
    pub form: Option<SubmitForm>,
    /// The chain chooser, while a recipe with dependencies is being submitted.
    pub chain: Option<ChainForm>,
    /// Every job submitted from this project, with the settings it used.
    pub history: History,
    /// Built the first time the job browser is opened.
    pub jobs: Option<JobsView>,
    pub picker: Option<HistoryPick>,
    /// A kill or a rerun waiting to be confirmed.
    pub pending: Option<PendingAction>,
}

impl App {
    pub fn new(sources: Vec<Loaded>, mut cache: SourceCache) -> Self {
        let tree = Tree::build(&sources, &mut cache);
        let configs = Configs::new(&sources[0].working_dir);
        let history = History::load(&sources[0].working_dir);
        let run_args = RunArgs::load(&sources[0].working_dir);
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
            params: None,
            params_dry: false,
            run_args,
            global_dir: None,
            global_configs: Configs::default(),
            global_run_args: RunArgs::default(),
            show_private: false,
            group_layer: true,
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
            detecting: None,
            form: None,
            chain: None,
            history,
            jobs: None,
            picker: None,
            pending: None,
        };
        app.refresh_visible();
        app.select_first_recipe();
        app
    }

    /// The project source; config files and state live beside it.
    pub fn project(&self) -> &Loaded {
        &self.sources[0]
    }

    /// Keep global recipes' memory in `dir` from now on. Only worth doing
    /// when a global library is loaded.
    pub fn set_global_dir(&mut self, dir: Option<PathBuf>) {
        let base = self.project().working_dir.clone();
        self.global_dir = dir.filter(|_| self.sources.iter().any(|s| s.global));
        if let Some(dir) = &self.global_dir {
            // Only just's own global justfile may be loaded, with no library
            // directory yet for its memory to go in.
            let _ = crate::world::create_dir_all(dir);
            self.global_configs = Configs::new_at(&base, dir);
            self.global_run_args = RunArgs::load(dir);
        }
        self.history = History::load(&base).with_global(self.global_dir.as_deref());
    }

    /// The global library's directory, when this source's memory lives there
    /// rather than with the project.
    pub fn global_home(&self, source: &Loaded) -> Option<&PathBuf> {
        self.global_dir.as_ref().filter(|_| source.global)
    }

    /// The config chain and state for a recipe from the global library
    /// (`true`) or the project.
    pub fn configs_for(&mut self, global: bool) -> &mut Configs {
        match global && self.global_dir.is_some() {
            true => &mut self.global_configs,
            false => &mut self.configs,
        }
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
        self.tree = Tree::build_with(&self.sources, &mut self.cache, self.group_layer);
        self.refresh_visible();
        if let Some(id) = selected.and_then(|(root, path)| self.tree.find_namepath(root, &path)) {
            self.selected_id = Some(id);
            self.refresh_visible();
        }
        self.configs = Configs::new(&self.project().working_dir);
        self.run_args = RunArgs::load(&self.project().working_dir);
        self.set_global_dir(self.global_dir.clone());
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
            Kind::Module | Kind::Group => None,
        }
    }

    /// Run the selected recipe. One with dependencies goes through the chain
    /// chooser first, so the ones already done can be left out; the run then
    /// happens when the chooser is confirmed.
    /// Run the selected recipe, or open its argument form first when it
    /// declares parameters, defaults or not, so they can be seen and changed.
    pub(super) fn run_or_ask(&mut self, dry: bool) -> Action {
        match self.param_form() {
            Some(form) if !form.free => {
                self.open_params(form, dry);
                Action::None
            }
            _ => self.build_run("", dry),
        }
    }

    /// The argument form for the selected recipe, filled the way it was left
    /// last time. A recipe without parameters gets one free-text row.
    pub(super) fn param_form(&self) -> Option<ParamForm> {
        let target = self.run_target()?;
        let root = self.selected()?.root_index;
        let id = self.tree.find_namepath(root, &target)?;
        let recipe = &self.tree.nodes[id].info.as_ref()?.recipe;
        let mut form = match recipe.parameters.is_empty() {
            true => ParamForm::free_text(),
            false => ParamForm::from_recipe(recipe),
        };
        if let Some(values) = self
            .run_args_key()
            .and_then(|key| self.run_args_for_selected().get(&key))
        {
            form.restore(values);
        }
        Some(form)
    }

    pub(super) fn open_params(&mut self, form: ParamForm, dry: bool) {
        self.params = Some(form);
        self.params_dry = dry;
        self.mode = Mode::Args;
    }

    /// Run with what the form holds, remembering it for next time. A
    /// required row left empty keeps the form open.
    pub(super) fn run_params(&mut self) -> Action {
        let Some(form) = self.params.as_ref() else {
            return Action::None;
        };
        let args = match form.args() {
            Ok(args) => args,
            Err(problem) => {
                self.error(problem);
                return Action::None;
            }
        };
        let values = form.values.clone();
        self.params = None;
        self.mode = Mode::Normal;
        let global = self.global_home(self.selected_source()).is_some();
        let key = self.run_args_key();
        let store = match global {
            true => &mut self.global_run_args,
            false => &mut self.run_args,
        };
        if let Some(key) = key
            && let Err(err) = store.remember(&key, &values)
        {
            self.error(format!("could not save the arguments: {err}"));
        }
        let line: Vec<String> = args.iter().map(|a| quote(a)).collect();
        self.build_run(&line.join(" "), self.params_dry)
    }

    /// Where the selected recipe's run arguments are kept.
    fn run_args_for_selected(&self) -> &RunArgs {
        match self.global_home(self.selected_source()) {
            Some(_) => &self.global_run_args,
            None => &self.run_args,
        }
    }

    /// What the selected recipe's arguments are kept under. A global
    /// library's recipes are told apart from the project's by their file.
    fn run_args_key(&self) -> Option<String> {
        let target = self.run_target()?;
        let source = self.selected_source();
        Some(match source.global {
            true => format!("~{}::{target}", source.label),
            false => target,
        })
    }

    pub(super) fn build_run(&mut self, extra: &str, dry: bool) -> Action {
        let Some(target) = self.run_target() else {
            self.error("select a recipe to run");
            return Action::None;
        };
        if self.open_run_chain(extra, dry) {
            return Action::None;
        }
        let source = self.selected_source();
        let dir = source.working_dir.clone();
        let mut args = just::file_args(source);
        if dry {
            args.push("--dry-run".into());
        }
        args.push(target);
        args.extend(split_args(extra));
        Action::Run {
            commands: vec![args],
            dir,
            dry,
        }
    }
}

/// An argument line as the words a shell would make of it, so a value
/// quoted by the form arrives as one argument.
pub(crate) fn split_args(extra: &str) -> Vec<String> {
    crate::params::split_words(extra)
}
