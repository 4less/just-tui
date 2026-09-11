//! Rendering a frame to plain text, for documentation and for checking the
//! layout without a terminal. Drives `--snapshot` and `--keys`.

use std::io::{self, Write};

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::ui;

const DEFAULT_SIZE: (u16, u16) = (120, 40);

/// Draw one frame into an off-screen buffer and print it.
pub fn render(app: &mut App, size: &str) -> Result<()> {
    let (width, height) = parse_size(size);

    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| ui::draw(frame, app))?;
    let buffer = terminal.backend().buffer().clone();

    let mut out = io::stdout().lock();
    for y in 0..height {
        let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
}

/// `120x40`, falling back to a sensible default.
fn parse_size(size: &str) -> (u16, u16) {
    size.split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)))
        .unwrap_or(DEFAULT_SIZE)
}

/// Replay keystrokes so a snapshot can show any screen. Named keys go in
/// brackets: `--keys "s[right][down]12G"`.
pub fn press(app: &mut App, keys: &str) {
    let mut chars = keys.chars();
    while let Some(c) = chars.next() {
        let code = match c {
            '[' => match named_key(&mut chars) {
                Some(code) => code,
                None => continue,
            },
            '\n' => KeyCode::Enter,
            '\t' => KeyCode::Tab,
            other => KeyCode::Char(other),
        };
        let _ = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

/// Read up to the closing bracket and translate the name inside it.
fn named_key(chars: &mut impl Iterator<Item = char>) -> Option<KeyCode> {
    let name: String = chars.take_while(|c| *c != ']').collect();
    Some(match name.to_ascii_lowercase().as_str() {
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        other => KeyCode::F(other.strip_prefix('f')?.parse().ok()?),
    })
}
