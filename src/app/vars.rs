//! Overriding variables: the list `V` opens for a module, and the directory
//! explorer that picks a path for one of them.

use std::path::PathBuf;

use super::{Action, App, Mode};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::just::Loaded;
use crate::model::Justfile;
use crate::tree::Kind;
use crate::vars::{Explorer, ExplorerFocus, Overrides, VarRow, VarsForm, join, variable_keys};
use serde_json::Value;

impl App {
    /// The overrides kept for a source: the global memory's for the global
    /// justfile, the project's otherwise.
    pub fn overrides_for(&self, source: &Loaded) -> &Overrides {
        match self.global_home(source) {
            Some(_) => &self.global_overrides,
            None => &self.overrides,
        }
    }

    /// Everything `just` is told before the recipe name: which justfile, and
    /// a `--set` for each override of a variable it declares.
    pub fn just_args(&self, source: &Loaded) -> Vec<String> {
        let mut args = crate::just::file_args(source);
        args.extend(
            self.overrides_for(source)
                .set_args(&variable_keys(&source.justfile)),
        );
        args
    }

    /// The variables of the module the selection is in.
    pub fn open_vars(&mut self) {
        let Some(mut id) = self.selected_id else {
            self.error("select a module or recipe first");
            return;
        };
        // From a recipe, an alias or a group, up to the module it is in.
        while self.tree.nodes[id].kind != Kind::Module {
            match self.tree.nodes[id].parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
        let node = &self.tree.nodes[id];
        let module = node.namepath.clone();
        let source = self
            .sources
            .get(node.root_index)
            .unwrap_or(&self.sources[0]);
        let Some(file) = module_file(&source.justfile, &module) else {
            return;
        };
        let overrides = self.overrides_for(source);
        let rows: Vec<VarRow> = file
            .assignments
            .iter()
            .map(|(name, assignment)| {
                let key = join(&module, name);
                VarRow {
                    overridden: overrides.get(&key).map(str::to_owned),
                    key,
                    name: name.clone(),
                    declared: crate::model::render_literal(&assignment.value),
                    literal: match &assignment.value {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    },
                }
            })
            .collect();
        if rows.is_empty() {
            let shown = if module.is_empty() {
                "this justfile"
            } else {
                &module
            };
            self.error(format!("{shown} declares no variables"));
            return;
        }
        // Relative values mean what they mean where the recipes run: the
        // module's own directory, or for global recipes the one just-tui was
        // started in.
        let base = match source.global {
            true => source.working_dir.clone(),
            false => node
                .source
                .as_ref()
                .and_then(|p| p.parent())
                .map_or_else(|| source.working_dir.clone(), PathBuf::from),
        };
        let global = self.global_home(source).is_some();
        self.vars = Some(VarsForm {
            module,
            rows,
            cursor: 0,
            base,
            typing: None,
            global,
        });
        self.mode = Mode::Vars;
    }

    pub(super) fn handle_vars_key(&mut self, key: KeyEvent) -> Action {
        let Some(form) = self.vars.as_mut() else {
            self.mode = Mode::Normal;
            return Action::None;
        };

        if let Some(typed) = form.typing.as_mut() {
            match key.code {
                KeyCode::Esc => form.typing = None,
                KeyCode::Enter => {
                    let value = typed.trim().to_owned();
                    form.typing = None;
                    self.set_override((!value.is_empty()).then_some(value));
                }
                KeyCode::Backspace => {
                    typed.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    typed.clear()
                }
                KeyCode::Char(c) => typed.push(c),
                _ => {}
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('V') => {
                self.vars = None;
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => form.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => form.move_cursor(1),
            KeyCode::Char('e') => {
                let current = form
                    .selected()
                    .map(|row| {
                        row.overridden
                            .clone()
                            .or_else(|| row.literal.clone())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                form.typing = Some(current);
            }
            KeyCode::Delete | KeyCode::Char('d') => self.set_override(None),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                let Some(row) = form.selected() else {
                    return Action::None;
                };
                let explorer = Explorer::open(
                    row.key.clone(),
                    form.start_dir(),
                    crate::vars::qwe_pins(),
                    form.global,
                );
                self.explorer = Some(explorer);
                self.mode = Mode::Explorer;
            }
            _ => {}
        }
        Action::None
    }

    pub(super) fn handle_explorer_key(&mut self, key: KeyEvent) -> Action {
        let Some(explorer) = self.explorer.as_mut() else {
            self.mode = Mode::Vars;
            return Action::None;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.explorer = None;
                self.mode = Mode::Vars;
            }
            KeyCode::Up | KeyCode::Char('k') => explorer.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => explorer.move_cursor(1),
            KeyCode::PageUp => explorer.move_cursor(-10),
            KeyCode::PageDown => explorer.move_cursor(10),
            KeyCode::Home | KeyCode::Char('g') => explorer.move_cursor(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => explorer.move_cursor(isize::MAX / 2),
            KeyCode::Tab | KeyCode::BackTab => explorer.toggle_focus(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                explorer.focus = ExplorerFocus::Listing;
                explorer.up();
            }
            KeyCode::Right | KeyCode::Char('l') => explorer.enter(),
            KeyCode::Enter => {
                // A directory is opened; a file, having nothing inside it,
                // is the choice.
                let on_file = explorer.focus == ExplorerFocus::Listing
                    && explorer.selected().is_some_and(|entry| !entry.is_dir);
                match on_file {
                    true => self.choose_path(None),
                    false => explorer.enter(),
                }
            }
            KeyCode::Char(' ') => self.choose_path(None),
            KeyCode::Char('c') => {
                let dir = explorer.dir.clone();
                self.choose_path(Some(dir));
            }
            KeyCode::Char('.') => explorer.toggle_hidden(),
            KeyCode::Char('~') => {
                if let Some(home) = std::env::var_os("HOME") {
                    explorer.focus = ExplorerFocus::Listing;
                    explorer.go(PathBuf::from(home));
                }
            }
            _ => {}
        }
        Action::None
    }

    /// Set the variable the explorer is open for, to `path` or what is
    /// highlighted, and go back to the list.
    fn choose_path(&mut self, path: Option<PathBuf>) {
        let Some(explorer) = self.explorer.take() else {
            return;
        };
        let chosen = path.unwrap_or_else(|| explorer.choice());
        self.mode = Mode::Vars;
        self.set_override(Some(chosen.display().to_string()));
    }

    /// Store, or with `None` drop, the override for the selected variable,
    /// and show it in the list.
    fn set_override(&mut self, value: Option<String>) {
        let Some(form) = self.vars.as_mut() else {
            return;
        };
        let Some(row) = form.rows.get_mut(form.cursor) else {
            return;
        };
        row.overridden = value.clone();
        let key = row.key.clone();
        let store = match form.global {
            true => &mut self.global_overrides,
            false => &mut self.overrides,
        };
        let saved = store.set(&key, value.clone());
        match (saved, value) {
            (Err(err), _) => self.error(format!("could not save the override: {err}")),
            (Ok(()), Some(value)) => {
                self.info(format!("{key} = {value} for every run and submission"))
            }
            (Ok(()), None) => self.info(format!("{key} back to the justfile's value")),
        }
    }
}

/// The justfile of the module at `path` (`sub::inner`), or the root for "".
fn module_file<'a>(root: &'a Justfile, path: &str) -> Option<&'a Justfile> {
    let mut file = root;
    for part in path.split("::").filter(|p| !p.is_empty()) {
        file = file.modules.get(part)?;
    }
    Some(file)
}
