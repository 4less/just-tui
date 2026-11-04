//! `~/.just-tui/.just-tui-cluster-recent`: the allocations last asked for in
//! any form, submitted or not, newest first and each one once. The submit
//! form's `F6` offers them to every recipe, in every project.
//!
//! Only the allocation is kept — partition, time, memory and the rest. A job
//! name, arguments or a task file belong to the recipe they were typed for.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::slurm::{Field, Settings};

pub const RECENT_NAME: &str = ".just-tui-cluster-recent";

/// Allocations kept.
const KEEP: usize = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Entry {
    /// The recipe the allocation was set for, as `module::name`.
    pub namepath: String,
    /// Seconds since the epoch, for ordering against the history.
    pub at: u64,
    /// The same moment in local time, for reading.
    pub when: String,
    /// The job it went out as; empty when it was set and not submitted.
    pub job_id: String,
    pub settings: Settings,
}

#[derive(Debug, Default)]
pub struct Recent {
    /// `None` keeps the list in memory only, as tests and the demo do.
    path: Option<PathBuf>,
    entries: Vec<Entry>,
}

/// The allocation alone: what may carry from one recipe to another.
pub fn allocation(settings: &Settings) -> Settings {
    let mut settings = settings.clone();
    for field in [Field::Name, Field::Args, Field::Each] {
        settings.get_mut(field).clear();
    }
    settings
}

impl Recent {
    /// Read the list kept in `dir`, the global library.
    pub fn load(dir: Option<&Path>) -> Self {
        let path = dir.map(|dir| dir.join(RECENT_NAME));
        let entries = path
            .as_deref()
            .and_then(|path| crate::world::read(path).ok())
            .map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect()
            })
            .unwrap_or_default();
        Self { path, entries }
    }

    /// Newest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Put an allocation at the top, dropping an older copy of it. One that
    /// asks for nothing is not worth offering.
    pub fn add(
        &mut self,
        namepath: &str,
        settings: &Settings,
        job_id: &str,
    ) -> std::io::Result<()> {
        let settings = allocation(settings);
        if settings == Settings::default() {
            return Ok(());
        }
        self.entries.retain(|entry| entry.settings != settings);
        self.entries.insert(
            0,
            Entry {
                namepath: namepath.to_owned(),
                at: crate::history::now(),
                when: crate::history::timestamp(),
                job_id: job_id.to_owned(),
                settings,
            },
        );
        self.entries.truncate(KEEP);
        self.write()
    }

    fn write(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            crate::world::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for entry in &self.entries {
            if let Ok(line) = serde_json::to_string(entry) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        crate::world::write(path, &text)
    }
}
