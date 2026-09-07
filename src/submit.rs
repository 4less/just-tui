//! State of the Slurm submit form. Everything here is pure form behaviour:
//! moving between fields, editing them, and reporting where values came from.
//! Reading configs and calling `sbatch` stay in [`crate::app`].

use std::path::{Path, PathBuf};

use crate::config::Resolved;
use crate::slurm::{self, Cluster, Field, Settings};

/// Which `.just-tui-cluster-config` a save from the form writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveScope {
    /// Defaults for every recipe in the recipe's own module.
    Module,
    /// Defaults for the whole project.
    Project,
    /// A `[namepath]` section for this recipe alone.
    Recipe,
}

impl SaveScope {
    /// How the scope is named in status messages.
    pub fn label(self) -> &'static str {
        match self {
            SaveScope::Module => "module",
            SaveScope::Project => "project",
            SaveScope::Recipe => "this recipe",
        }
    }
}

/// One recipe's pending submission: the values, where they came from, and
/// which field the cursor is on.
pub struct SubmitForm {
    /// Recipe being submitted, as `module::name`.
    pub namepath: String,
    /// Current values, starting from [`Self::resolved`] and edited from there.
    pub settings: Settings,
    /// What the config chain supplied, kept so edits can be told apart.
    pub resolved: Resolved,
    /// Index into [`slurm::FIELDS`].
    pub field: usize,
    /// Directory of the module this recipe lives in.
    pub scope_dir: PathBuf,
    /// Directory the job runs in, and under which `logs/` is written.
    pub base: PathBuf,
    /// Config scopes this recipe answers to, for the edit chooser.
    pub scopes: Vec<(&'static str, PathBuf)>,
    pub scope_index: usize,
}

impl SubmitForm {
    /// Start from what the config chain resolved to.
    pub fn new(namepath: String, resolved: Resolved, base: PathBuf, scope_dir: PathBuf) -> Self {
        let mut scopes = vec![("module", scope_dir.clone()), ("project", base.clone())];
        if let Some(home) = std::env::var_os("HOME") {
            scopes.push(("home", PathBuf::from(home)));
        }
        // A recipe at the project root has no separate module scope.
        scopes.dedup_by(|a, b| a.1 == b.1);

        Self {
            namepath,
            settings: resolved.settings.clone(),
            resolved,
            field: 0,
            scope_dir,
            base,
            scopes,
            scope_index: 0,
        }
    }

    /// The field the cursor is on.
    pub fn current(&self) -> Field {
        slurm::FIELDS[self.field]
    }

    /// Array jobs need per-task log names.
    pub fn is_array(&self) -> bool {
        !self.settings.array.is_empty()
    }

    /// Adopt freshly resolved settings, keeping the cursor where it was.
    pub fn reset_to(&mut self, resolved: Resolved) {
        self.settings = resolved.settings.clone();
        self.resolved = resolved;
    }

    /// Where the selected field's value came from, if it was not typed here.
    pub fn origin(&self) -> Option<&str> {
        let field = self.current();
        if self.settings.get(field) == self.resolved.settings.get(field) {
            self.resolved.origin(field)
        } else {
            Some("edited here")
        }
    }

    /// One line naming every file that contributed a value.
    pub fn provenance(&self) -> String {
        match self.resolved.sources.is_empty() {
            true => "nothing prefilled — submitting records it in the state file".to_owned(),
            false => format!("from {}", self.resolved.sources.join(" · ")),
        }
    }

    /// Move the cursor, wrapping at both ends.
    pub fn move_field(&mut self, delta: isize) {
        let count = slurm::FIELDS.len() as isize;
        self.field = (self.field as isize + delta).rem_euclid(count) as usize;
    }

    /// Move within the config-scope chooser, wrapping at both ends.
    pub fn move_scope(&mut self, delta: isize) {
        let count = self.scopes.len() as isize;
        self.scope_index = (self.scope_index as isize + delta).rem_euclid(count) as usize;
    }

    /// Directory of the config scope currently highlighted.
    pub fn selected_scope(&self) -> PathBuf {
        self.scopes[self.scope_index].1.clone()
    }

    pub fn push_char(&mut self, c: char) {
        let field = self.current();
        self.settings.get_mut(field).push(c);
    }

    pub fn pop_char(&mut self) {
        let field = self.current();
        self.settings.get_mut(field).pop();
    }

    pub fn clear_field(&mut self) {
        let field = self.current();
        self.settings.get_mut(field).clear();
    }

    /// Step a pick-list field through the values the cluster reported.
    /// Fields without choices are left alone.
    pub fn cycle(&mut self, cluster: &Cluster, delta: isize) {
        let field = self.current();
        let choices = cluster.choices(field);
        if choices.is_empty() {
            return;
        }
        // A leading empty entry means "do not pass the flag", and must stay
        // reachable however many values the cluster reported.
        let mut options = vec![String::new()];
        options.extend(choices);

        let current = self.settings.get(field);
        let position = options.iter().position(|c| c == current).unwrap_or(0) as isize;
        let next = (position + delta).rem_euclid(options.len() as isize) as usize;
        *self.settings.get_mut(field) = options[next].clone();
    }

    /// The directory and optional recipe section a save should write to.
    pub fn save_target(&self, scope: SaveScope, project: &Path) -> (PathBuf, Option<String>) {
        match scope {
            SaveScope::Module => (self.scope_dir.clone(), None),
            SaveScope::Project => (project.to_path_buf(), None),
            SaveScope::Recipe => (self.scope_dir.clone(), Some(self.namepath.clone())),
        }
    }
}
