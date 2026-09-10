//! Reading past submissions back: the history picker, and loading one of its
//! records into the submit form.

use super::{App, Mode};
use crate::history::Record;
use crate::tree::Kind;

/// Which past submissions are being browsed, and where the cursor is.
pub struct HistoryPick {
    /// The recipe the list is limited to, or `None` for every recipe.
    pub namepath: Option<String>,
    pub index: usize,
    /// Where the picker was opened from, so `Esc` goes back there.
    pub from: Mode,
}

impl App {
    /// Open the history list. `namepath` limits it to one recipe, which is
    /// what `F6` in the submit form wants.
    pub fn open_history(&mut self, namepath: Option<String>) {
        self.history.reload();
        if self.history_entries(namepath.as_deref()).is_empty() {
            self.error(match &namepath {
                Some(name) => format!("no recorded submissions of {name} yet"),
                None => "nothing submitted from here yet".to_owned(),
            });
            return;
        }
        self.picker = Some(HistoryPick {
            namepath,
            index: 0,
            from: self.mode,
        });
        self.mode = Mode::History;
    }

    /// Records the picker is showing, newest first.
    pub fn history_entries(&self, namepath: Option<&str>) -> Vec<&Record> {
        match namepath {
            Some(name) => self.history.for_recipe(name),
            None => self.history.recent().collect(),
        }
    }

    /// The records the open picker lists.
    pub fn picked_entries(&self) -> Vec<&Record> {
        match self.picker.as_ref() {
            Some(pick) => self.history_entries(pick.namepath.as_deref()),
            None => Vec::new(),
        }
    }

    pub fn move_history(&mut self, delta: isize) {
        let count = self.picked_entries().len() as isize;
        if count == 0 {
            return;
        }
        if let Some(pick) = self.picker.as_mut() {
            pick.index = (pick.index as isize + delta).rem_euclid(count) as usize;
        }
    }

    /// Load the highlighted record into the submit form, opening the form on
    /// its recipe first when the cursor is somewhere else.
    pub fn apply_history(&mut self) {
        let Some(record) = self
            .picker
            .as_ref()
            .and_then(|pick| self.picked_entries().get(pick.index).copied())
            .cloned()
        else {
            return;
        };
        self.picker = None;

        let already_open = self
            .form
            .as_ref()
            .is_some_and(|form| form.namepath == record.namepath);
        if !already_open {
            if !self.select_namepath(&record.namepath) {
                self.mode = Mode::Normal;
                self.error(format!(
                    "{} is not in this justfile any more",
                    record.namepath
                ));
                return;
            }
            self.open_submit();
        }

        self.mode = Mode::Submit;
        if let Some(form) = self.form.as_mut() {
            form.settings = record.settings.clone();
        }
        self.info(format!(
            "loaded the settings job {} ran with ({})",
            record.job_id, record.when
        ));
    }

    /// Move the explorer cursor to a recipe by its full path, in whichever
    /// source holds it.
    pub fn select_namepath(&mut self, namepath: &str) -> bool {
        let found = self
            .tree
            .nodes
            .iter()
            .position(|node| node.kind == Kind::Recipe && node.namepath == namepath);
        let Some(id) = found else { return false };

        self.tree.reveal(id);
        self.selected_id = Some(id);
        self.refresh_visible();
        true
    }
}
