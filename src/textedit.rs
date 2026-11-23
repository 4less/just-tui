//! One line of text being typed, with a caret: what the run form's rows and
//! a variable typed by hand share. The caret counts characters, not bytes.

use crate::input::{KeyCode, KeyEvent, KeyModifiers};

/// Characters in `text`.
pub fn width(text: &str) -> usize {
    text.chars().count()
}

/// Byte offset of character `caret`, or the end.
fn offset(text: &str, caret: usize) -> usize {
    text.char_indices()
        .nth(caret)
        .map_or(text.len(), |(at, _)| at)
}

/// Apply an editing key to `text`. Returns whether the key was one: the
/// caret moves (←→, Home/End, ^a/^e), text goes in at it or comes out
/// around it (Backspace, Delete, ^w a word, ^u everything).
pub fn handle(text: &mut String, caret: &mut usize, key: KeyEvent) -> bool {
    *caret = (*caret).min(width(text));
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Left if control => *caret = word_start(text, *caret),
        KeyCode::Right if control => *caret = word_end(text, *caret),
        KeyCode::Left => *caret = caret.saturating_sub(1),
        KeyCode::Right => *caret = (*caret + 1).min(width(text)),
        KeyCode::Home => *caret = 0,
        KeyCode::End => *caret = width(text),
        KeyCode::Char('a') if control => *caret = 0,
        KeyCode::Char('e') if control => *caret = width(text),
        KeyCode::Char('u') if control => {
            text.clear();
            *caret = 0;
        }
        KeyCode::Char('w') if control => {
            let start = word_start(text, *caret);
            text.replace_range(offset(text, start)..offset(text, *caret), "");
            *caret = start;
        }
        KeyCode::Char(_) if control => return false,
        KeyCode::Char(c) => {
            text.insert(offset(text, *caret), c);
            *caret += 1;
        }
        KeyCode::Backspace if *caret > 0 => {
            *caret -= 1;
            text.remove(offset(text, *caret));
        }
        KeyCode::Delete if *caret < width(text) => {
            text.remove(offset(text, *caret));
        }
        KeyCode::Backspace | KeyCode::Delete => {}
        _ => return false,
    }
    true
}

/// Where the word before the caret starts, skipping spaces first.
fn word_start(text: &str, caret: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut at = caret.min(chars.len());
    while at > 0 && chars[at - 1].is_whitespace() {
        at -= 1;
    }
    while at > 0 && !chars[at - 1].is_whitespace() {
        at -= 1;
    }
    at
}

/// Where the word after the caret ends, skipping spaces first.
fn word_end(text: &str, caret: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut at = caret.min(chars.len());
    while at < chars.len() && chars[at].is_whitespace() {
        at += 1;
    }
    while at < chars.len() && !chars[at].is_whitespace() {
        at += 1;
    }
    at
}
