//! Editing a recipe in the source pane: the buffer, the splice back into the
//! file, and saving through `just`.

use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::{App, Mode};
use crate::editor::{Editor, expand_tabs};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::just;
use crate::source::SourceCache;
use crate::ui;

const FILE: &str = "\
# Say hi
hello name='world':
    echo \"hi {{ name }}\"

# Build it
build:
    cargo build
";

fn editor(code: &str) -> Editor {
    Editor::new(PathBuf::from("justfile"), "hello".into(), 2, code)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_key(key(KeyCode::Char(c)));
    }
}

/// A project directory holding [`FILE`], loaded through `just` itself.
fn project(name: &str) -> (PathBuf, App) {
    let dir = std::env::temp_dir().join(format!("just-tui-edit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("justfile"), FILE).unwrap();
    let loaded = just::load(&dir, None).expect("just reads the fixture");
    let mut app = App::new(vec![loaded], SourceCache::default());
    assert!(app.select_namepath("hello"));
    (dir, app)
}

fn read(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("justfile")).unwrap()
}

#[test]
fn typing_and_newlines_keep_the_indentation() {
    let mut ed = editor("a:\n    echo 1");
    ed.move_rows(1);
    ed.end();
    ed.newline();
    for c in "echo 2".chars() {
        ed.insert_char(c);
    }
    assert_eq!(ed.text(), "a:\n    echo 1\n    echo 2");
    assert!(ed.dirty());
}

#[test]
fn backspace_joins_lines_and_delete_line_keeps_one() {
    let mut ed = editor("a:\n    b");
    ed.move_rows(1);
    ed.backspace();
    assert_eq!(ed.text(), "a:    b");
    ed.delete_line();
    assert_eq!(ed.lines, vec![String::new()]);
}

#[test]
fn undo_takes_back_a_run_of_typing_at_once() {
    let mut ed = editor("a:");
    ed.end();
    for c in " b c".chars() {
        ed.insert_char(c);
    }
    ed.newline();
    ed.undo();
    assert_eq!(ed.text(), "a: b c");
    ed.undo();
    assert_eq!(ed.text(), "a:");
    assert!(!ed.dirty());
    ed.redo();
    assert_eq!(ed.text(), "a: b c");
}

#[test]
fn tab_indents_with_what_the_body_already_uses() {
    let mut ed = editor("a:\n\techo");
    ed.move_rows(1);
    ed.home();
    ed.home();
    ed.indent();
    assert_eq!(ed.lines[1], "\t\techo");
    ed.dedent();
    assert_eq!(ed.lines[1], "\techo");

    let mut ed = editor("a:\n  echo");
    ed.move_rows(1);
    ed.dedent();
    assert_eq!(ed.lines[1], "echo");
}

#[test]
fn tabs_are_drawn_to_the_next_stop() {
    assert_eq!(expand_tabs("\tx"), "    x");
    assert_eq!(expand_tabs("ab\tx"), "ab  x");
    let mut ed = editor("\tx");
    ed.end();
    assert_eq!(ed.caret_column(), 5);
    ed.click(1, 0);
    assert_eq!(ed.col, 0, "a click inside a tab lands before it");
}

#[test]
fn the_edit_replaces_only_its_own_lines() {
    let mut ed = editor("hello name='world':\n    echo \"hi {{ name }}\"");
    ed.move_rows(1);
    ed.end();
    ed.newline();
    for c in "echo bye".chars() {
        ed.insert_char(c);
    }
    let out = ed.splice(FILE).unwrap();
    assert_eq!(
        out,
        FILE.replace("{{ name }}\"\n", "{{ name }}\"\n    echo bye\n")
    );
}

#[test]
fn a_file_changed_underneath_is_not_overwritten() {
    let ed = editor("hello name='world':\n    echo \"hi {{ name }}\"");
    let changed = FILE.replace("echo \"hi", "echo \"hello");
    assert!(ed.splice(&changed).is_err());
}

#[test]
fn saving_writes_the_recipe_and_reloads() {
    let (dir, mut app) = project("save");
    app.handle_key(key(KeyCode::Char('i')));
    assert_eq!(app.mode, Mode::Edit);

    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Enter));
    typed(&mut app, "echo bye");
    app.handle_key(ctrl('s'));

    assert_eq!(app.mode, Mode::Normal, "{:?}", app.status);
    assert!(read(&dir).contains("{{ name }}\"\n    echo bye\n\n# Build it"));
    let info = app.selected().and_then(|n| n.info.as_ref()).unwrap();
    assert!(
        info.code.ends_with("echo bye"),
        "the pane shows the new text"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_just_rejects_is_put_back() {
    let (dir, mut app) = project("reject");
    app.handle_key(key(KeyCode::Char('i')));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Home));
    app.handle_key(key(KeyCode::Home));
    // The header loses its colon, which just will not parse.
    app.handle_key(key(KeyCode::End));
    app.handle_key(key(KeyCode::Backspace));
    app.handle_key(ctrl('s'));

    assert_eq!(app.mode, Mode::Edit, "the edit stays open to be fixed");
    assert_eq!(read(&dir), FILE);
    let (message, _) = app.status.clone().unwrap();
    assert!(message.starts_with("not saved"), "{message}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsaved_changes_take_a_second_esc() {
    let (dir, mut app) = project("discard");
    app.handle_key(key(KeyCode::Char('i')));
    typed(&mut app, "x");
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Edit);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(read(&dir), FILE);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_pane_shows_the_edit_and_its_keys() {
    let (dir, mut app) = project("draw");
    app.handle_key(key(KeyCode::Char('i')));
    typed(&mut app, "# ");
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let screen: String = (0..20)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(screen.contains("editing justfile:1"), "{screen}");
    assert!(screen.contains("# # Say hi"), "{screen}");
    assert!(screen.contains("^s save"), "{screen}");
    assert!(screen.contains("modified"), "{screen}");
    let _ = std::fs::remove_dir_all(&dir);
}
