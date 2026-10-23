//! Overriding a module's variables: the overrides just-tui remembers, the
//! list a module's variables are edited in, and the directory explorer a
//! path is picked with.
//!
//! An override never touches the justfile. It is handed to `just` as
//! `--set <path> <value>` on every run and submission, exactly as it could be
//! typed in a shell: `just --set sub::out /scratch/run1 sub::align`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const OVERRIDES_NAME: &str = ".just-tui-overrides";

/// The values set for variables, by full path (`data`, `sub::out`). Kept
/// beside the root justfile, like the other state, as one JSON object.
#[derive(Debug, Default)]
pub struct Overrides {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl Overrides {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(OVERRIDES_NAME);
        let values = crate::world::read(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, values }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// Set, or with `None` drop, the override for one variable.
    pub fn set(&mut self, key: &str, value: Option<String>) -> std::io::Result<()> {
        match value {
            Some(value) => self.values.insert(key.to_owned(), value),
            None => self.values.remove(key),
        };
        let text = serde_json::to_string_pretty(&self.values).unwrap_or_default();
        crate::world::write(&self.path, &text)
    }

    /// `--set key value` for every override whose variable `known` still
    /// declares. One for a variable that has since been removed would make
    /// `just` refuse to run at all.
    pub fn set_args(&self, known: &[String]) -> Vec<String> {
        let mut args = Vec::new();
        for (key, value) in &self.values {
            if known.iter().any(|k| k == key) {
                args.extend(["--set".to_owned(), key.clone(), value.clone()]);
            }
        }
        args
    }
}

/// Every variable a justfile declares, its modules' included, by the path
/// `--set` takes.
pub fn variable_keys(file: &crate::model::Justfile) -> Vec<String> {
    fn walk(file: &crate::model::Justfile, prefix: &str, out: &mut Vec<String>) {
        for name in file.assignments.keys() {
            out.push(join(prefix, name));
        }
        for (name, module) in &file.modules {
            walk(module, &join(prefix, name), out);
        }
    }
    let mut out = Vec::new();
    walk(file, "", &mut out);
    out
}

/// `prefix::name`, or `name` at the top.
pub fn join(prefix: &str, name: &str) -> String {
    match prefix.is_empty() {
        true => name.to_owned(),
        false => format!("{prefix}::{name}"),
    }
}

/// One row of the variables list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarRow {
    /// The path `--set` takes: `sub::out`.
    pub key: String,
    pub name: String,
    /// As the justfile writes it, for display: `"results"`.
    pub declared: String,
    /// The plain string, when the value is a string literal.
    pub literal: Option<String>,
    pub overridden: Option<String>,
}

/// The variables of one module, while they are being edited.
#[derive(Debug, Clone)]
pub struct VarsForm {
    /// The module's path, empty for the root.
    pub module: String,
    pub rows: Vec<VarRow>,
    pub cursor: usize,
    /// Directory relative values are taken from: where the module's recipes
    /// run. The explorer starts here when a value is not a path it can find.
    pub base: PathBuf,
    /// A value being typed by hand, in place of the selected row's.
    pub typing: Option<String>,
    pub global: bool,
}

impl VarsForm {
    pub fn selected(&self) -> Option<&VarRow> {
        self.rows.get(self.cursor)
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.rows.len() as isize;
        if count > 0 {
            self.cursor = (self.cursor as isize + delta).rem_euclid(count) as usize;
        }
    }

    /// Where the explorer opens for the selected row: the value in force,
    /// if it names a directory, or the nearest one above it that exists, or
    /// failing that the module's own directory.
    pub fn start_dir(&self) -> PathBuf {
        let value = self
            .selected()
            .and_then(|row| row.overridden.clone().or_else(|| row.literal.clone()));
        let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
            return self.base.clone();
        };
        let mut path = expand_home(&value);
        if path.is_relative() {
            path = self.base.join(path);
        }
        let mut current = Some(path.as_path());
        while let Some(dir) = current {
            if is_dir(dir) {
                return dir.to_path_buf();
            }
            current = dir.parent();
        }
        self.base.clone()
    }
}

/// `~/x` as a path under the home directory.
fn expand_home(value: &str) -> PathBuf {
    match (value.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ if value == "~" => std::env::var_os("HOME").map_or_else(|| value.into(), PathBuf::from),
        _ => PathBuf::from(value),
    }
}

fn is_dir(path: &Path) -> bool {
    crate::world::is_dir(path)
}

/// One entry in the explorer's listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerFocus {
    Listing,
    Pins,
}

/// The full-screen directory explorer, picking a value for one variable.
#[derive(Debug, Clone)]
pub struct Explorer {
    /// The variable being set, as `--set` takes it.
    pub key: String,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub show_hidden: bool,
    /// Bookmarked directories (qwe's), when there are any.
    pub pins: Vec<(String, PathBuf)>,
    pub pin_cursor: usize,
    pub focus: ExplorerFocus,
    /// The global justfile's variables are stored with the global memory.
    pub global: bool,
}

impl Explorer {
    pub fn open(key: String, dir: PathBuf, pins: Vec<(String, PathBuf)>, global: bool) -> Self {
        let mut explorer = Self {
            key,
            dir: PathBuf::new(),
            entries: Vec::new(),
            cursor: 0,
            show_hidden: false,
            pins,
            pin_cursor: 0,
            focus: ExplorerFocus::Listing,
            global,
        };
        explorer.go(dir);
        explorer
    }

    /// Show `dir`, directories first, each group by name.
    pub fn go(&mut self, dir: PathBuf) {
        let mut entries: Vec<Entry> = crate::world::read_dir(&dir)
            .into_iter()
            .filter_map(|(path, is_dir)| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                (self.show_hidden || !name.starts_with('.')).then_some(Entry { path, name, is_dir })
            })
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        // Coming up out of a directory, the cursor lands on the one just left.
        let came_from = self.dir.clone();
        self.cursor = entries
            .iter()
            .position(|entry| entry.path == came_from)
            .unwrap_or(0);
        self.entries = entries;
        self.dir = dir;
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn move_cursor(&mut self, delta: isize) {
        match self.focus {
            ExplorerFocus::Listing => {
                let last = self.entries.len().saturating_sub(1) as isize;
                self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
            }
            ExplorerFocus::Pins => {
                let last = self.pins.len().saturating_sub(1) as isize;
                self.pin_cursor = (self.pin_cursor as isize + delta).clamp(0, last) as usize;
            }
        }
    }

    /// Into the highlighted directory, or to the highlighted pin.
    pub fn enter(&mut self) {
        match self.focus {
            ExplorerFocus::Listing => {
                if let Some(entry) = self.selected().filter(|e| e.is_dir).cloned() {
                    self.go(entry.path);
                }
            }
            ExplorerFocus::Pins => {
                if let Some((_, dir)) = self.pins.get(self.pin_cursor).cloned() {
                    self.focus = ExplorerFocus::Listing;
                    self.go(dir);
                }
            }
        }
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            self.go(parent);
        }
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let dir = self.dir.clone();
        let keep = self.selected().map(|e| e.path.clone());
        self.go(dir);
        if let Some(keep) = keep
            && let Some(index) = self.entries.iter().position(|e| e.path == keep)
        {
            self.cursor = index;
        }
    }

    pub fn toggle_focus(&mut self) {
        if !self.pins.is_empty() {
            self.focus = match self.focus {
                ExplorerFocus::Listing => ExplorerFocus::Pins,
                ExplorerFocus::Pins => ExplorerFocus::Listing,
            };
        }
    }

    /// What choosing now would set: the highlighted entry, or the directory
    /// being shown when nothing is highlighted.
    pub fn choice(&self) -> PathBuf {
        match self.focus {
            ExplorerFocus::Pins => self
                .pins
                .get(self.pin_cursor)
                .map_or_else(|| self.dir.clone(), |(_, dir)| dir.clone()),
            ExplorerFocus::Listing => self
                .selected()
                .map_or_else(|| self.dir.clone(), |e| e.path.clone()),
        }
    }
}

/// qwe's bookmarks (<https://github.com/olafurw/qwe>), when it is in use:
/// `~/.qwe.data`, one `tag<TAB>directory` line each, with the home directory
/// written as a literal `$HOME`. Bookmarks whose directory is gone are left
/// out.
pub fn qwe_pins() -> Vec<(String, PathBuf)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let Ok(text) = crate::world::read(&home.join(".qwe.data")) else {
        return Vec::new();
    };
    parse_qwe(&text, &home)
        .into_iter()
        .filter(|(_, dir)| is_dir(dir))
        .collect()
}

/// [`qwe_pins`] without the file system, for tests.
pub fn parse_qwe(text: &str, home: &Path) -> Vec<(String, PathBuf)> {
    let home = home.display().to_string();
    let mut pins: Vec<(String, PathBuf)> = text
        .lines()
        .filter_map(|line| {
            let (tag, dir) = line.split_once('\t')?;
            let dir = dir.trim().replacen("$HOME", &home, 1);
            (!tag.trim().is_empty() && !dir.is_empty())
                .then(|| (tag.trim().to_owned(), PathBuf::from(dir)))
        })
        .collect();
    pins.sort();
    pins.dedup();
    pins
}
