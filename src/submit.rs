//! State of the Slurm submit form. Everything here is pure form behaviour:
//! moving between fields, editing them, and reporting where values came from.
//! Reading configs and calling `sbatch` stay in [`crate::app`].

use std::path::{Path, PathBuf};

use crate::batch::{self, Plan};
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
    /// Where the caret sits in the current field, counted in characters.
    /// Always within the field: moving between fields puts it at the end.
    pub cursor: usize,
    /// Directory of the module this recipe lives in.
    pub scope_dir: PathBuf,
    /// Directory the job runs in, and under which `logs/` is written.
    pub base: PathBuf,
    /// Config scopes this recipe answers to, for the edit chooser.
    pub scopes: Vec<(&'static str, PathBuf)>,
    pub scope_index: usize,
    /// What `each` expands to, or why it does not. Recomputed when the fields
    /// it depends on change, since globbing on every frame would be wasteful.
    pub plan: Option<Plan>,
    pub plan_error: Option<String>,
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

        let mut form = Self {
            namepath,
            settings: resolved.settings.clone(),
            resolved,
            field: 0,
            cursor: 0,
            scope_dir,
            base,
            scopes,
            scope_index: 0,
            plan: None,
            plan_error: None,
        };
        form.refresh_plan();
        form
    }

    /// Re-expand `each`. Cheap when it is empty, which is the common case.
    pub fn refresh_plan(&mut self) {
        match batch::plan(&self.base, &self.namepath, &self.settings) {
            Ok(plan) => {
                self.plan = plan;
                self.plan_error = None;
            }
            Err(problem) => {
                self.plan = None;
                self.plan_error = Some(problem);
            }
        }
    }

    /// The manifest and task count to submit with, if this is an expansion.
    pub fn batch(&self) -> Option<(&Path, usize)> {
        self.plan
            .as_ref()
            .map(|plan| (plan.manifest.as_path(), plan.count()))
    }

    /// The field the cursor is on.
    pub fn current(&self) -> Field {
        slurm::FIELDS[self.field]
    }

    /// Adopt freshly resolved settings, keeping the cursor where it was.
    pub fn reset_to(&mut self, resolved: Resolved) {
        self.settings = resolved.settings.clone();
        self.resolved = resolved;
        self.clamp_cursor();
        self.refresh_plan();
    }

    /// Adopt settings from somewhere else — a past submission, say.
    pub fn adopt(&mut self, settings: Settings) {
        self.settings = settings;
        self.clamp_cursor();
        self.refresh_plan();
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

    /// Move between fields, wrapping at both ends. The caret lands at the end
    /// of whatever it arrives in, which is where typing continues from.
    pub fn move_field(&mut self, delta: isize) {
        let count = slurm::FIELDS.len() as isize;
        self.field = (self.field as isize + delta).rem_euclid(count) as usize;
        self.cursor = self.width();
    }

    /// Characters in the field the caret is in.
    fn width(&self) -> usize {
        self.settings.get(self.current()).chars().count()
    }

    /// Byte offset of the caret, for splitting the string around it.
    fn offset(&self) -> usize {
        let value = self.settings.get(self.current());
        value
            .char_indices()
            .nth(self.cursor)
            .map_or(value.len(), |(at, _)| at)
    }

    /// Move the caret within the field, stopping at either end.
    pub fn move_cursor(&mut self, delta: isize) {
        let limit = self.width() as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, limit) as usize;
    }

    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor = self.width();
    }

    /// Keep the caret inside a value that changed underneath it.
    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.width());
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

    /// Type a character in at the caret.
    pub fn push_char(&mut self, c: char) {
        let (field, at) = (self.current(), self.offset());
        self.settings.get_mut(field).insert(at, c);
        self.cursor += 1;
        self.refresh_plan();
    }

    /// Backspace: remove the character before the caret.
    pub fn pop_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let (field, at) = (self.current(), self.offset());
        self.settings.get_mut(field).remove(at);
        self.refresh_plan();
    }

    /// Delete: remove the character the caret is on.
    pub fn delete_char(&mut self) {
        if self.cursor >= self.width() {
            return;
        }
        let (field, at) = (self.current(), self.offset());
        self.settings.get_mut(field).remove(at);
        self.refresh_plan();
    }

    pub fn clear_field(&mut self) {
        let field = self.current();
        self.settings.get_mut(field).clear();
        self.cursor = 0;
        self.refresh_plan();
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
        self.cursor_end();
        self.refresh_plan();
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
