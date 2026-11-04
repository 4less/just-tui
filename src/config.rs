//! `.just-tui-cluster-config` files: hand-written Slurm defaults, and the
//! machine-written record of what each recipe was last submitted with.
//!
//! ```text
//! # defaults for everything in this scope
//! partition = qib-compute
//! time      = 48:00:00
//!
//! [level3::db::tree-submit]   # one recipe, by its full path
//! mem  = 128G
//! time = 96:00:00
//! ```
//!
//! Files are read from the user's home, then the justfile base directory,
//! then every module directory from the outside in. Nearer wins, so a module
//! always overrides the project, which overrides the home file. Nothing is
//! inferred from the justfile itself.

use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::slurm::{FIELDS, Field, Settings};

/// Written into a config file that does not exist yet, so `$EDITOR` opens
/// something explanatory rather than a blank buffer.
pub const TEMPLATE: &str = "\
# just-tui Slurm defaults for this scope.
#
# Keys here apply to every recipe in scope. A .just-tui-cluster-config in a
# nested module overrides this file; a [recipe::path] section overrides both.
# Anything set here also overrides what a recipe was last submitted with.
#
# partition = general
# account   =
# time      = 24:00:00
# cpus      = 8
# mem       = 16G
#
# [module::recipe]
# mem = 128G
";

pub const CONFIG_NAME: &str = ".just-tui-cluster-config";
pub const STATE_NAME: &str = ".just-tui-cluster-state";
/// How the state file's settings are labelled in the form: submitted, or
/// only set in a form and left.
pub const LAST_RUN: &str = "last run";
pub const LAST_SET: &str = "last set";
/// Named settings in the global library, `~/.just-tui/cluster-presets`.
pub const PRESETS_NAME: &str = "cluster-presets";
/// Written into a presets file that does not exist yet, so `$EDITOR` opens
/// something explanatory rather than a blank buffer.
pub const PRESETS_TEMPLATE: &str = "\
# just-tui cluster presets, for every project. F9 in the submit form picks
# one; i there adds what Slurm says about a partition as a new section.
#
# [default] fills the form of a recipe that has never been submitted and
# that no .just-tui-cluster-config mentions. Any other section is a named
# preset that sets only the fields it names.
#
# [default]
# partition = general
# time      = 04:00:00
# mem       = 8G
#
# [gpu]
# partition = gpu
# gpus      = 1
";

/// The preset a recipe with no settings of its own starts from.
pub const DEFAULT_PRESET: &str = "default";
/// How a value the `[default]` preset supplied is labelled.
pub const PRESET_DEFAULT: &str = "preset default";

/// One parsed config file.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    pub path: PathBuf,
    /// Keys before any `[section]`, applying to every recipe in scope.
    pub defaults: Settings,
    /// `[namepath]` sections, applying to one recipe.
    pub recipes: BTreeMap<String, Settings>,
    /// `skip = a, b` under a section: the links of that recipe's chain that
    /// were left unchecked last time. Only the state file writes it.
    pub skips: BTreeMap<String, Vec<String>>,
    /// `unsent = yes` under a section: the settings were set in a form but
    /// not submitted. Only the state file writes it.
    pub unsent: BTreeSet<String>,
}

impl ConfigFile {
    pub fn parse(path: &Path, text: &str) -> Self {
        let mut file = ConfigFile {
            path: path.to_path_buf(),
            ..Default::default()
        };
        let mut section: Option<String> = None;

        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = Some(name.trim().to_owned());
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches(['"', '\'']).to_owned();
            if key.trim() == "skip" {
                if let Some(name) = &section {
                    file.skips.insert(name.clone(), split_list(&value));
                }
                continue;
            }
            if key.trim() == "unsent" {
                if let Some(name) = &section {
                    file.unsent.insert(name.clone());
                }
                continue;
            }
            let Some(field) = field_for_key(key.trim()) else {
                continue;
            };
            match &section {
                Some(name) => *file.recipes.entry(name.clone()).or_default().get_mut(field) = value,
                None => *file.defaults.get_mut(field) = value,
            }
        }
        file
    }

    /// Defaults, then any section matching this recipe.
    pub fn settings_for(&self, namepath: &str) -> Settings {
        let mut settings = self.defaults.clone();
        if let Some(specific) = self.recipes.get(namepath) {
            let mut ignored = Origins::new();
            overlay(&mut settings, specific, "", &mut ignored);
        }
        settings
    }

    pub fn render(&self) -> String {
        let mut out = String::from("# just-tui Slurm defaults\n\n");
        write_settings(&mut out, &self.defaults);
        for (name, settings) in &self.recipes {
            let _ = writeln!(out, "\n[{name}]");
            write_settings(&mut out, settings);
        }
        out
    }
}

/// `a, b, c` as its parts, blanks dropped.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn write_settings(out: &mut String, settings: &Settings) {
    for field in FIELDS {
        let value = settings.get(field);
        if !value.is_empty() {
            let _ = writeln!(out, "{:<10} = {value}", field.label());
        }
    }
}

/// Non-empty values from `over` replace those in `base`, recording where each
/// one came from.
fn overlay(base: &mut Settings, over: &Settings, from: &str, origins: &mut Origins) {
    for field in FIELDS {
        let value = over.get(field);
        if !value.is_empty() {
            *base.get_mut(field) = value.to_owned();
            origins.insert(field, from.to_owned());
        }
    }
}

pub type Origins = BTreeMap<Field, String>;

fn field_for_key(key: &str) -> Option<Field> {
    let key = key.trim().to_ascii_lowercase().replace(['-', '_'], "");
    FIELDS
        .into_iter()
        .find(|f| f.label() == key || alias(*f).contains(&key.as_str()))
}

/// Spellings accepted besides the field's own label.
fn alias(field: Field) -> &'static [&'static str] {
    match field {
        Field::Partition => &["queue"],
        Field::Cpus => &["cpuspertask", "cpu", "threads"],
        Field::Mem => &["memory"],
        Field::Time => &["walltime"],
        Field::Gpus => &["gres", "gpu"],
        Field::Extra => &["opts", "flags", "sbatch"],
        _ => &[],
    }
}

/// Where a setting came from, so the form can say why a field is filled in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub settings: Settings,
    /// Files consulted, weakest first.
    pub sources: Vec<String>,
    /// Which of them supplied each field.
    pub origins: Origins,
}

impl Resolved {
    /// Start from `settings` when nothing else had anything to say: no
    /// config file in scope, and no earlier submission of the recipe.
    pub fn fall_back_to(&mut self, settings: &Settings, label: &str) {
        if !self.sources.is_empty() {
            return;
        }
        overlay(&mut self.settings, settings, label, &mut self.origins);
        self.sources.push(label.to_owned());
    }

    pub fn origin(&self, field: Field) -> Option<&str> {
        self.origins.get(&field).map(String::as_str)
    }
}

/// Named settings to start a form from, kept once for every project:
///
/// ```text
/// [default]        # fills a recipe that has no settings of its own
/// partition = compute
/// time      = 04:00:00
///
/// [gpu]            # picked by hand, with F6 in the submit form
/// partition = gpu
/// gpus      = 1
/// ```
///
/// Keys before any section belong to `[default]`. A preset is about the
/// allocation, so a job name, arguments or task file in one are ignored.
#[derive(Debug, Clone, Default)]
pub struct Presets {
    /// The directory the file is read from, so a reload reads it again.
    pub dir: Option<PathBuf>,
    pub named: BTreeMap<String, Settings>,
}

impl Presets {
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        let path = dir.join(PRESETS_NAME);
        let mut named = BTreeMap::new();
        if let Ok(text) = crate::world::read(&path) {
            let file = ConfigFile::parse(&path, &text);
            if file.defaults != Settings::default() {
                named.insert(DEFAULT_PRESET.to_owned(), file.defaults.clone());
            }
            for (name, settings) in file.recipes {
                let mut merged = named.remove(&name).unwrap_or_default();
                let mut ignored = Origins::new();
                overlay(&mut merged, &settings, "", &mut ignored);
                named.insert(name, merged);
            }
        }
        for settings in named.values_mut() {
            for field in [Field::Name, Field::Args, Field::Each] {
                settings.get_mut(field).clear();
            }
        }
        Self {
            dir: Some(dir.to_path_buf()),
            named,
        }
    }

    /// Add sections to the end of `dir`'s presets file, leaving what is
    /// already in it untouched.
    pub fn append(dir: &Path, presets: &[(String, Settings)]) -> std::io::Result<()> {
        let path = dir.join(PRESETS_NAME);
        crate::world::create_dir_all(dir)?;
        let mut text = crate::world::read(&path).unwrap_or_else(|_| PRESETS_TEMPLATE.to_owned());
        for (name, settings) in presets {
            if !text.ends_with("\n\n") {
                text.push('\n');
            }
            let _ = writeln!(text, "[{name}]");
            write_settings(&mut text, settings);
        }
        crate::world::write(&path, &text)
    }

    pub fn fallback(&self) -> Option<&Settings> {
        self.named.get(DEFAULT_PRESET)
    }

    /// `default` first, then the rest by name.
    pub fn ordered(&self) -> Vec<(&str, &Settings)> {
        let mut presets: Vec<_> = self
            .named
            .iter()
            .map(|(name, settings)| (name.as_str(), settings))
            .collect();
        presets.sort_by_key(|(name, _)| *name != DEFAULT_PRESET);
        presets
    }
}

/// Loads and caches the config chain.
#[derive(Default)]
pub struct Configs {
    files: HashMap<PathBuf, Option<ConfigFile>>,
    state: ConfigFile,
    state_path: PathBuf,
    base: PathBuf,
}

impl Configs {
    /// `base` is the directory of the root justfile; the state file lives there.
    pub fn new(base: &Path) -> Self {
        Self::new_at(base, base)
    }

    /// Like [`Self::new`], with the state file in `state_dir` instead: the
    /// global library keeps what its recipes were submitted with.
    pub fn new_at(base: &Path, state_dir: &Path) -> Self {
        let state_path = state_dir.join(STATE_NAME);
        let state = crate::world::read(&state_path)
            .map(|text| ConfigFile::parse(&state_path, &text))
            .unwrap_or_else(|_| ConfigFile {
                path: state_path.clone(),
                ..Default::default()
            });
        Self {
            files: HashMap::new(),
            state,
            state_path,
            base: base.to_path_buf(),
        }
    }

    /// A short label for a config file: relative to the project when it sits
    /// inside it, `~`-shortened otherwise.
    fn label(&self, path: &Path) -> String {
        match path.strip_prefix(&self.base) {
            Ok(relative) => relative.display().to_string(),
            Err(_) => crate::ui::shorten_home(&path.display().to_string()),
        }
    }

    fn file(&mut self, dir: &Path) -> Option<&ConfigFile> {
        let path = dir.join(CONFIG_NAME);
        self.files
            .entry(path.clone())
            .or_insert_with(|| {
                crate::world::read(&path)
                    .ok()
                    .map(|text| ConfigFile::parse(&path, &text))
            })
            .as_ref()
    }

    /// Merge every scope for `namepath`. `dirs` runs outermost (home, base)
    /// to innermost (the recipe's own module), and later entries win.
    pub fn resolve(&mut self, dirs: &[PathBuf], namepath: &str) -> Resolved {
        let mut settings = Settings::default();
        let mut sources = Vec::new();
        let mut origins = Origins::new();

        // Weakest first: what the recipe was last submitted with is only a
        // convenience, so any hand-written config overrides it. Its resources
        // carry over; its arguments and task file do not: they say what one
        // submission ran, and carried over they turn the next, plain submit
        // into an array over a stale task list (or pass it stale arguments).
        if let Some(mut last) = self.state.recipes.get(namepath).cloned() {
            last.get_mut(Field::Each).clear();
            last.get_mut(Field::Args).clear();
            let label = match self.state.unsent.contains(namepath) {
                true => LAST_SET,
                false => LAST_RUN,
            };
            if last != Settings::default() {
                overlay(&mut settings, &last, label, &mut origins);
                sources.push(label.to_owned());
            }
        }

        for dir in dirs {
            let Some(file) = self.file(dir) else { continue };
            let path = file.path.clone();
            let scoped = file.settings_for(namepath);
            if scoped != Settings::default() {
                let label = self.label(&path);
                overlay(&mut settings, &scoped, &label, &mut origins);
                sources.push(label);
            }
        }

        Resolved {
            settings,
            sources,
            origins,
        }
    }

    /// Remember what a recipe was submitted with, for next time.
    pub fn remember(&mut self, namepath: &str, settings: &Settings) -> std::io::Result<()> {
        self.state
            .recipes
            .insert(namepath.to_owned(), settings.clone());
        self.state.unsent.remove(namepath);
        self.write_state()
    }

    /// Remember what a recipe's form was left holding, though it was not
    /// submitted: the next form for it starts from there all the same.
    pub fn remember_set(&mut self, namepath: &str, settings: &Settings) -> std::io::Result<()> {
        self.state
            .recipes
            .insert(namepath.to_owned(), settings.clone());
        self.state.unsent.insert(namepath.to_owned());
        self.write_state()
    }

    /// Which links of a recipe's chain were left unchecked last time.
    pub fn skipped(&self, namepath: &str) -> &[String] {
        self.state
            .skips
            .get(namepath)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Remember which links were left unchecked, so the next chooser opens
    /// the way this one was left.
    pub fn remember_skipped(&mut self, namepath: &str, skipped: &[String]) -> std::io::Result<()> {
        match skipped.is_empty() {
            true => self.state.skips.remove(namepath),
            false => self
                .state
                .skips
                .insert(namepath.to_owned(), skipped.to_vec()),
        };
        // A recipe that has never been submitted still needs a section for
        // the key to sit under.
        self.state.recipes.entry(namepath.to_owned()).or_default();
        self.write_state()
    }

    fn write_state(&self) -> std::io::Result<()> {
        let mut text = String::from(
            "# just-tui: settings each recipe was last submitted with, or set to.\n\
             # Written automatically — edit .just-tui-cluster-config instead.\n",
        );
        for (name, settings) in &self.state.recipes {
            let _ = writeln!(text, "\n[{name}]");
            write_settings(&mut text, settings);
            if let Some(skipped) = self.state.skips.get(name) {
                let _ = writeln!(text, "{:<10} = {}", "skip", skipped.join(", "));
            }
            if self.state.unsent.contains(name) {
                let _ = writeln!(text, "{:<10} = yes", "unsent");
            }
        }
        crate::world::write(&self.state_path, &text)
    }

    /// Path of a scope's config file, creating it from the template when it
    /// does not exist yet, so it can be opened in an editor.
    pub fn ensure_file(&mut self, dir: &Path) -> std::io::Result<PathBuf> {
        let path = dir.join(CONFIG_NAME);
        if !crate::world::exists(&path) {
            crate::world::write(&path, TEMPLATE)?;
            self.files.remove(&path);
        }
        Ok(path)
    }

    /// Forget every parsed file, so edits on disk are picked up.
    pub fn invalidate(&mut self) {
        self.files.clear();
    }

    /// Write these settings into `dir`'s `.just-tui-cluster-config`, either as
    /// the defaults for that scope or as one recipe's section.
    pub fn save_defaults(
        &mut self,
        dir: &Path,
        namepath: Option<&str>,
        settings: &Settings,
    ) -> std::io::Result<PathBuf> {
        let path = dir.join(CONFIG_NAME);
        let mut file = crate::world::read(&path)
            .map(|text| ConfigFile::parse(&path, &text))
            .unwrap_or_else(|_| ConfigFile {
                path: path.clone(),
                ..Default::default()
            });

        match namepath {
            Some(name) => {
                file.recipes.insert(name.to_owned(), settings.clone());
            }
            None => file.defaults = settings.clone(),
        }

        crate::world::write(&path, &file.render())?;
        self.files.insert(path.clone(), Some(file));
        Ok(path)
    }
}

/// The directory chain for a recipe: home, then base, then each module
/// directory from the outside in.
pub fn scope_dirs(base: &Path, module_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home));
    }
    dirs.push(base.to_path_buf());
    for dir in module_dirs {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }
    dirs.dedup();
    dirs
}
