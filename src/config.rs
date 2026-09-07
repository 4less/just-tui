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

use std::collections::BTreeMap;
use std::collections::HashMap;
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

/// One parsed config file.
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    pub path: PathBuf,
    /// Keys before any `[section]`, applying to every recipe in scope.
    pub defaults: Settings,
    /// `[namepath]` sections, applying to one recipe.
    pub recipes: BTreeMap<String, Settings>,
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
            let Some(field) = field_for_key(key.trim()) else {
                continue;
            };
            let value = value.trim().trim_matches(['"', '\'']).to_owned();
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
    pub fn origin(&self, field: Field) -> Option<&str> {
        self.origins.get(&field).map(String::as_str)
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
        let state_path = base.join(STATE_NAME);
        let state = std::fs::read_to_string(&state_path)
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
                std::fs::read_to_string(&path)
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
        // convenience, so any hand-written config overrides it.
        if let Some(last) = self.state.recipes.get(namepath).cloned() {
            overlay(&mut settings, &last, "last run", &mut origins);
            sources.push("last run".to_owned());
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
        let mut text = String::from(
            "# just-tui: settings each recipe was last submitted with.\n\
             # Written automatically — edit .just-tui-cluster-config instead.\n",
        );
        for (name, settings) in &self.state.recipes {
            let _ = writeln!(text, "\n[{name}]");
            write_settings(&mut text, settings);
        }
        std::fs::write(&self.state_path, text)
    }

    /// Path of a scope's config file, creating it from the template when it
    /// does not exist yet, so it can be opened in an editor.
    pub fn ensure_file(&mut self, dir: &Path) -> std::io::Result<PathBuf> {
        let path = dir.join(CONFIG_NAME);
        if !path.exists() {
            std::fs::write(&path, TEMPLATE)?;
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
        let mut file = std::fs::read_to_string(&path)
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

        std::fs::write(&path, file.render())?;
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
