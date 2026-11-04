//! F9 in the submit form: the presets in one list — the ones saved in
//! `~/.just-tui/cluster-presets` and the ones read off the partitions — to
//! apply one, edit the file, or save what Slurm suggests into it.

use super::{Action, App, Mode};
use crate::config::{PRESETS_NAME, PRESETS_TEMPLATE, Presets};
use crate::input::{KeyCode, KeyEvent};
use crate::slurm::{FIELDS, Field, Settings};

/// One row of the presets window.
#[derive(Debug, Clone)]
pub struct PresetRow {
    pub name: String,
    pub settings: Settings,
    /// Read off a partition and not in the file yet.
    pub inferred: bool,
}

/// Where the cursor is in the presets window.
#[derive(Debug, Default)]
pub struct PresetPick {
    pub index: usize,
}

impl App {
    pub(super) fn open_presets(&mut self) {
        if self.presets.dir.is_some() {
            self.load_presets(self.presets.dir.clone());
        }
        self.preset_pick = Some(PresetPick::default());
        self.mode = Mode::Presets;
    }

    /// The saved presets, `default` first, then a partition's own defaults
    /// for every partition that has no preset of its name yet.
    pub fn preset_rows(&self) -> Vec<PresetRow> {
        let mut rows: Vec<PresetRow> = self
            .presets
            .ordered()
            .into_iter()
            .map(|(name, settings)| PresetRow {
                name: name.to_owned(),
                settings: settings.clone(),
                inferred: false,
            })
            .collect();
        if let Some(cluster) = self.cluster.as_ref() {
            for (name, settings) in cluster.profiles() {
                if !self.presets.named.contains_key(&name) {
                    rows.push(PresetRow {
                        name,
                        settings,
                        inferred: true,
                    });
                }
            }
        }
        rows
    }

    pub(super) fn handle_presets_key(&mut self, key: KeyEvent) -> Action {
        let count = self.preset_rows().len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_presets(),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.move_preset(1, count),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => self.move_preset(-1, count),
            KeyCode::Enter => self.apply_preset(),
            KeyCode::Char('i') => self.save_inferred(false),
            KeyCode::Char('I') => self.save_inferred(true),
            KeyCode::Char('e') => return self.edit_presets(),
            _ => {}
        }
        Action::None
    }

    fn close_presets(&mut self) {
        self.preset_pick = None;
        self.mode = Mode::Submit;
    }

    fn move_preset(&mut self, delta: isize, count: usize) {
        if let Some(pick) = self.preset_pick.as_mut()
            && count > 0
        {
            pick.index = (pick.index as isize + delta).rem_euclid(count as isize) as usize;
        }
    }

    fn selected_preset(&self) -> Option<PresetRow> {
        let index = self.preset_pick.as_ref()?.index;
        self.preset_rows().into_iter().nth(index)
    }

    /// Put the preset's fields into the form, leaving the ones it does not
    /// name as they are.
    fn apply_preset(&mut self) {
        let Some(row) = self.selected_preset() else {
            return;
        };
        self.close_presets();
        if let Some(form) = self.form.as_mut() {
            let mut settings = form.settings.clone();
            for field in FIELDS {
                let value = row.settings.get(field);
                let carries = !matches!(field, Field::Name | Field::Args | Field::Each);
                if carries && !value.is_empty() {
                    *settings.get_mut(field) = value.to_owned();
                }
            }
            form.adopt(settings);
        }
        self.info(format!("applied the {} preset", row.name));
    }

    /// `i` saves the inferred preset under the cursor into the file; `I`
    /// saves every inferred one. Sections are appended, so whatever was
    /// written there by hand stays as it was.
    fn save_inferred(&mut self, all: bool) {
        let rows: Vec<PresetRow> = match all {
            true => self
                .preset_rows()
                .into_iter()
                .filter(|r| r.inferred)
                .collect(),
            false => self
                .selected_preset()
                .filter(|r| r.inferred)
                .into_iter()
                .collect(),
        };
        if rows.is_empty() {
            self.error(match all {
                true => "nothing to infer: every partition already has a preset",
                false => "this one is saved already — i saves a row read off Slurm",
            });
            return;
        }
        let Some(dir) = self.presets.dir.clone() else {
            self.error("no ~/.just-tui to keep presets in");
            return;
        };
        match Presets::append(&dir, &rows_as_pairs(&rows)) {
            Ok(()) => {
                self.load_presets(Some(dir));
                let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
                self.info(format!("saved {} to {PRESETS_NAME}", names.join(", ")));
            }
            Err(err) => self.error(format!("could not save the presets: {err}")),
        }
    }

    /// Open the presets file in `$EDITOR`, at the selected preset's section,
    /// creating the file with an explanation when there is none yet.
    fn edit_presets(&mut self) -> Action {
        let Some(dir) = self.presets.dir.clone() else {
            self.error("no ~/.just-tui to keep presets in");
            return Action::None;
        };
        let path = dir.join(PRESETS_NAME);
        if !crate::world::exists(&path) {
            let created = crate::world::create_dir_all(&dir)
                .and_then(|()| crate::world::write(&path, PRESETS_TEMPLATE));
            if let Err(err) = created {
                self.error(format!("could not create {}: {err}", path.display()));
                return Action::None;
            }
        }
        let line = self
            .selected_preset()
            .filter(|row| !row.inferred)
            .and_then(|row| {
                let text = crate::world::read(&path).ok()?;
                let header = format!("[{}]", row.name);
                text.lines()
                    .position(|l| l.trim_start().starts_with(&header))
                    .map(|i| i + 1)
            })
            .unwrap_or(1);
        Action::Edit { path, line }
    }
}

fn rows_as_pairs(rows: &[PresetRow]) -> Vec<(String, Settings)> {
    rows.iter()
        .map(|row| (row.name.clone(), row.settings.clone()))
        .collect()
}
