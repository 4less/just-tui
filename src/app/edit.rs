//! Editing the selected recipe in the source pane, and saving it back only
//! once `just` still accepts the file.

use crate::editor::Editor;
use crate::input::{KeyCode, KeyEvent, KeyModifiers};

use super::{Action, App, Mode, Pane, load_again};

impl App {
    /// Start editing the selected recipe where it is shown.
    pub(super) fn open_editor(&mut self) {
        if cfg!(target_arch = "wasm32") {
            self.error("the demo has no files to save to");
            return;
        }
        let Some(node) = self.selected() else {
            self.error("select a recipe to edit");
            return;
        };
        let Some(info) = node.info.as_ref() else {
            self.error("only recipes can be edited here — o opens the file in $EDITOR");
            return;
        };
        let Some(path) = info.source.clone().filter(|_| info.exact) else {
            self.error("this recipe could not be found in its file — o opens it in $EDITOR");
            return;
        };
        self.editor = Some(Editor::new(
            path,
            node.namepath.clone(),
            info.start_line,
            &info.code,
        ));
        self.focus = Pane::Code;
        self.mode = Mode::Edit;
    }

    pub(super) fn handle_edit_key(&mut self, key: KeyEvent) -> Action {
        let Some(editor) = self.editor.as_mut() else {
            self.mode = Mode::Normal;
            return Action::None;
        };
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if control => self.save_editor(),
            // Leaving with Ctrl-c would lose the edit; it asks like Esc does.
            KeyCode::Esc => self.leave_editor(),
            KeyCode::Char('c') if control => self.leave_editor(),
            KeyCode::Char('z') if control => editor.undo(),
            KeyCode::Char('y') if control => editor.redo(),
            KeyCode::Char('k') if control => editor.delete_line(),
            KeyCode::Char('a') if control => editor.home(),
            KeyCode::Char('e') if control => editor.end(),
            KeyCode::Char('d') if control => editor.move_rows(editor.height as isize / 2),
            KeyCode::Char('u') if control => editor.move_rows(-(editor.height as isize / 2)),
            KeyCode::Char(_) if control => {}
            KeyCode::Char(c) => editor.insert_char(c),
            KeyCode::Enter => editor.newline(),
            KeyCode::Tab => editor.indent(),
            KeyCode::BackTab => editor.dedent(),
            KeyCode::Backspace => editor.backspace(),
            KeyCode::Delete => editor.delete(),
            KeyCode::Left => editor.move_left(),
            KeyCode::Right => editor.move_right(),
            KeyCode::Up => editor.move_rows(-1),
            KeyCode::Down => editor.move_rows(1),
            KeyCode::PageUp => editor.move_rows(-(editor.height.max(2) as isize - 1)),
            KeyCode::PageDown => editor.move_rows(editor.height.max(2) as isize - 1),
            KeyCode::Home => editor.home(),
            KeyCode::End => editor.end(),
            _ => {}
        }
        if let Some(editor) = self.editor.as_mut() {
            editor.follow();
        }
        Action::None
    }

    /// Esc: straight out when nothing changed, otherwise only on a second Esc.
    fn leave_editor(&mut self) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.dirty() && !editor.confirm_discard {
            editor.confirm_discard = true;
            self.error("unsaved changes — Esc again to discard, Ctrl-s to save");
            return;
        }
        self.editor = None;
        self.mode = Mode::Normal;
    }

    /// Write the edit into its file, check `just` still reads the file, and
    /// put the old text back if it does not.
    pub(super) fn save_editor(&mut self) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        if !editor.dirty() {
            self.editor = None;
            self.mode = Mode::Normal;
            self.info("no changes");
            return;
        }
        let path = editor.path.clone();
        let namepath = editor.namepath.clone();
        let before = match crate::world::read(&path) {
            Ok(text) => text,
            Err(err) => {
                self.error(format!("could not read {}: {err}", path.display()));
                return;
            }
        };
        let after = match editor.splice(&before) {
            Ok(text) => text,
            Err(problem) => {
                self.error(problem);
                return;
            }
        };
        if let Err(err) = crate::world::write(&path, &after) {
            self.error(format!("could not write {}: {err}", path.display()));
            return;
        }

        if let Err(err) = load_again(self.selected_source()) {
            // Keep the edit open so the mistake can be fixed, and leave the
            // file as it was, so nothing else ever sees it broken.
            let restored = crate::world::write(&path, &before);
            let mut message = format!("not saved, just rejects it: {}", first_line(&err));
            if restored.is_err() {
                message = format!("{message} — and the old text could not be put back");
            }
            self.error(message);
            return;
        }

        self.editor = None;
        self.mode = Mode::Normal;
        self.reload();
        self.info(format!("saved {namepath}"));
    }
}

/// `just`'s complaint without the source excerpt it draws underneath.
fn first_line(err: &anyhow::Error) -> String {
    let text = format!("{err:#}");
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("it failed without saying why")
        .to_owned()
}
