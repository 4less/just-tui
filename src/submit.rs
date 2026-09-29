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

// ---------------------------------------------------------------------------
// Chains
// ---------------------------------------------------------------------------

/// One link of a chain as the chooser shows it: the recipe, whether it is
/// going out this time, and the settings it would go out with.
pub struct ChainRow {
    pub link: crate::chain::Link,
    pub checked: bool,
    /// What the config chain supplied, so the row can say where its settings
    /// came from.
    pub resolved: Resolved,
    /// What it will be submitted with: resolved, then edited in the form.
    pub settings: Settings,
    /// The form was opened and accepted for this row.
    pub edited: bool,
    /// Directory the job runs in.
    pub base: PathBuf,
    /// Directory of the module the recipe lives in, for the form's saves.
    pub scope_dir: PathBuf,
}

impl ChainRow {
    pub fn new(
        link: crate::chain::Link,
        resolved: Resolved,
        base: PathBuf,
        scope_dir: PathBuf,
    ) -> Self {
        let mut settings = resolved.settings.clone();
        // `align: (fetch "hg38")` calls fetch with an argument; that is what
        // fetch's job runs with, whatever a config says.
        if !link.args.is_empty() {
            settings.args = link.args.clone();
        }
        Self {
            link,
            checked: true,
            resolved,
            settings,
            edited: false,
            base,
            scope_dir,
        }
    }

    /// Whether the form has to be opened for this row before it can go out.
    pub fn needs_form(&self) -> bool {
        self.checked && !self.edited && !self.settings.configured()
    }

    /// Where the settings stand: `edited`, `config`, `last run`, or nothing.
    pub fn status(&self) -> &'static str {
        if self.edited {
            return "edited";
        }
        if self.resolved.sources.iter().any(|s| s != "last run") {
            return "config";
        }
        if self.resolved.sources.iter().any(|s| s == "last run") {
            return "last run";
        }
        "no settings"
    }
}

/// What confirming the chooser does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// One Slurm job per checked row, chained with `--dependency`.
    Submit,
    /// One `just --no-deps` per checked row, here, in order.
    Run {
        /// Arguments typed for the head recipe.
        extra: String,
        dry: bool,
    },
}

/// The chooser: every link of a chain, which ones go out, and where in the
/// walk through their forms the reader is.
pub struct ChainForm {
    /// The recipe `s` or Enter was pressed on.
    pub head: String,
    pub purpose: Purpose,
    pub rows: Vec<ChainRow>,
    pub cursor: usize,
    /// The row whose submit form is open, while one is.
    pub step: Option<usize>,
}

impl ChainForm {
    pub fn new(head: String, purpose: Purpose, rows: Vec<ChainRow>, skipped: &[String]) -> Self {
        let mut form = Self {
            head,
            purpose,
            rows,
            cursor: 0,
            step: None,
        };
        for row in &mut form.rows {
            if skipped.iter().any(|name| *name == row.link.label()) {
                row.checked = false;
            }
        }
        form.cursor = form.rows.len().saturating_sub(1);
        form
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.rows.len() as isize;
        if count > 0 {
            self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
        }
    }

    pub fn toggle(&mut self) {
        if let Some(row) = self.rows.get_mut(self.cursor) {
            row.checked = !row.checked;
        }
    }

    pub fn set_all(&mut self, checked: bool) {
        for row in &mut self.rows {
            row.checked = checked;
        }
    }

    pub fn checked(&self) -> usize {
        self.rows.iter().filter(|row| row.checked).count()
    }

    /// Rows that still need the form, from `from` on.
    pub fn next_needing_form(&self, from: usize) -> Option<usize> {
        (from..self.rows.len()).find(|&i| self.rows[i].needs_form())
    }

    pub fn needing_form(&self) -> usize {
        self.rows.iter().filter(|row| row.needs_form()).count()
    }

    /// The checked rows this one waits on. An unchecked upstream is declared
    /// done, so it is simply not waited for.
    pub fn waits_on(&self, index: usize) -> Vec<usize> {
        self.rows[index]
            .link
            .after
            .iter()
            .copied()
            .filter(|&upstream| self.rows[upstream].checked)
            .collect()
    }

    /// Labels of the unchecked rows, for the state file.
    pub fn skipped(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| !row.checked)
            .map(|row| row.link.label())
            .collect()
    }

    /// `waits on tree, s00_fetch`, naming rows by their recipe.
    pub fn waits_label(&self, index: usize) -> String {
        let names: Vec<&str> = self
            .waits_on(index)
            .into_iter()
            .map(|i| leaf(&self.rows[i].link.namepath))
            .collect();
        match names.is_empty() {
            true => String::new(),
            false => format!("waits on {}", names.join(", ")),
        }
    }

    /// Placeholder job ids for a preview, before anything is submitted.
    pub fn placeholder_ids(&self, index: usize) -> Vec<String> {
        self.waits_on(index)
            .into_iter()
            .map(|i| format!("<{}>", leaf(&self.rows[i].link.namepath)))
            .collect()
    }

    pub fn is_run(&self) -> bool {
        matches!(self.purpose, Purpose::Run { .. })
    }

    /// The `just` invocations a run comes to: one per checked row, in order,
    /// each with `--no-deps` since the rows before it are its dependencies.
    /// `prefix` pins `just` to the right file, as a single run does.
    pub fn run_commands(&self, prefix: &[String]) -> Vec<Vec<String>> {
        let Purpose::Run { extra, dry } = &self.purpose else {
            return Vec::new();
        };
        self.rows
            .iter()
            .filter(|row| row.checked)
            .map(|row| {
                let mut args = prefix.to_vec();
                args.push("--no-deps".to_owned());
                if *dry {
                    args.push("--dry-run".to_owned());
                }
                args.push(row.link.namepath.clone());
                args.extend(row.link.argv.iter().cloned());
                if row.link.namepath == self.head && row.link.argv.is_empty() {
                    args.extend(crate::app::split_args(extra));
                }
                args
            })
            .collect()
    }

    /// Adopt what the form ended with for the row whose step it was.
    pub fn accept(&mut self, settings: Settings) {
        if let Some(row) = self.step.and_then(|i| self.rows.get_mut(i)) {
            row.settings = settings;
            row.edited = true;
        }
        self.step = None;
    }
}

/// The recipe name without its module path.
pub fn leaf(namepath: &str) -> &str {
    namepath.rsplit("::").next().unwrap_or(namepath)
}
