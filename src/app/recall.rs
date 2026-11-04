//! Reading past submissions back: the history picker, and loading one of its
//! records into the submit form. Opened from the form, it also offers the
//! cluster presets, starting points read off the partitions, and what other
//! recipes were submitted with or set to.

use super::{App, Mode};
use crate::history::Record;
use crate::recent::allocation as resources;
use crate::slurm::{Field, Settings};
use crate::tree::Kind;

/// How many other recipes' settings the form's picker offers.
const OTHERS: usize = 30;

/// One row of the picker.
#[derive(Debug, Clone)]
pub enum Choice<'a> {
    /// A submission of the recipe in the form, or of any recipe when the
    /// list is not limited to one.
    Past(&'a Record),
    /// A named preset from the global library.
    Preset(&'a str, &'a Settings),
    /// A partition's own defaults, as Slurm reports them.
    Cluster(String, Box<Settings>),
    /// What another recipe was submitted with or set to: only its
    /// allocation carries.
    Other(Other<'a>),
}

/// Another recipe's allocation, from the history or the recent list.
#[derive(Debug, Clone, Copy)]
pub struct Other<'a> {
    pub namepath: &'a str,
    pub when: &'a str,
    /// The job it went out as; empty when it was only set.
    pub job_id: &'a str,
    pub settings: &'a Settings,
    at: u64,
}

impl<'a> Choice<'a> {
    fn settings(&self) -> &Settings {
        match self {
            Choice::Past(record) => &record.settings,
            Choice::Preset(_, settings) => settings,
            Choice::Cluster(_, settings) => settings,
            Choice::Other(other) => other.settings,
        }
    }
}

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
        if self.presets.dir.is_some() {
            self.load_presets(self.presets.dir.clone());
        }
        if self.history_entries(namepath.as_deref()).is_empty() {
            self.error(match &namepath {
                Some(_) => {
                    "nothing to offer yet: no past settings, presets or partitions".to_owned()
                }
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

    /// What the picker lists. Limited to one recipe — `F6` in the form —
    /// that is its own past runs, then the presets, then other recipes'
    /// settings, newest first and each allocation once.
    pub fn history_entries(&self, namepath: Option<&str>) -> Vec<Choice<'_>> {
        let Some(name) = namepath else {
            return self.history.recent().map(Choice::Past).collect();
        };
        let mut entries: Vec<Choice> = self
            .history
            .for_recipe(name)
            .into_iter()
            .map(Choice::Past)
            .collect();
        entries.extend(
            self.presets
                .ordered()
                .into_iter()
                .map(|(name, settings)| Choice::Preset(name, settings)),
        );

        if let Some(cluster) = self.cluster.as_ref() {
            entries.extend(
                cluster
                    .profiles()
                    .into_iter()
                    .map(|(name, settings)| Choice::Cluster(name, Box::new(settings))),
            );
        }

        // Submitted or only set, newest first, whichever list it is in.
        let mut others: Vec<Other> = self
            .history
            .recent()
            .map(|record| Other {
                namepath: &record.namepath,
                when: &record.when,
                job_id: &record.job_id,
                settings: &record.settings,
                at: record.at,
            })
            .chain(self.recent.entries().iter().map(|entry| Other {
                namepath: &entry.namepath,
                when: &entry.when,
                job_id: &entry.job_id,
                settings: &entry.settings,
                at: entry.at,
            }))
            .filter(|other| other.namepath != name)
            .collect();
        others.sort_by_key(|other| std::cmp::Reverse(other.at));

        let mut seen: Vec<Settings> = entries
            .iter()
            .map(|choice| resources(choice.settings()))
            .collect();
        let others = others
            .into_iter()
            .filter(|other| {
                let allocation = resources(other.settings);
                let fresh = allocation.configured() && !seen.contains(&allocation);
                if fresh {
                    seen.push(allocation);
                }
                fresh
            })
            .take(OTHERS)
            .map(Choice::Other);
        entries.extend(others);
        entries
    }

    /// The rows the open picker lists.
    pub fn picked_entries(&self) -> Vec<Choice<'_>> {
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

    /// Load the highlighted row into the submit form. A past run of its own
    /// recipe replaces the form whole; a preset or another recipe's settings
    /// only change the allocation, keeping this recipe's arguments.
    pub fn apply_history(&mut self) {
        let Some(choice) = self
            .picker
            .as_ref()
            .and_then(|pick| self.picked_entries().get(pick.index).cloned())
        else {
            return;
        };
        // Another recipe's allocation comes whole; a preset only sets what
        // it names, so `[gpu]` can add a GPU to the rest.
        let whole = matches!(choice, Choice::Past(_) | Choice::Other(_));
        let (record, carried, message) = match choice {
            Choice::Past(record) => (
                Some(record.clone()),
                record.settings.clone(),
                format!(
                    "loaded the settings job {} ran with ({})",
                    record.job_id, record.when
                ),
            ),
            Choice::Preset(name, settings) => {
                (None, settings.clone(), format!("applied the {name} preset"))
            }
            Choice::Cluster(name, settings) => (
                None,
                *settings,
                format!("took the {name} partition's defaults"),
            ),
            Choice::Other(other) => (
                None,
                resources(other.settings),
                match other.job_id.is_empty() {
                    true => format!("took the settings {} was set to", other.namepath),
                    false => format!("took the settings {} ran with", other.namepath),
                },
            ),
        };
        self.picker = None;

        let Some(record) = record else {
            self.mode = Mode::Submit;
            if let Some(form) = self.form.as_mut() {
                let mut settings = form.settings.clone();
                for field in crate::slurm::FIELDS {
                    let value = carried.get(field);
                    let carries = !matches!(field, Field::Name | Field::Args | Field::Each);
                    if carries && (whole || !value.is_empty()) {
                        *settings.get_mut(field) = value.to_owned();
                    }
                }
                form.adopt(settings);
            }
            self.info(message);
            return;
        };

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
            self.open_submit_single();
        }

        self.mode = Mode::Submit;
        if let Some(form) = self.form.as_mut() {
            form.adopt(carried);
        }
        self.info(message);
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
