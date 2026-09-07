//! Everything that touches the real terminal: entering and leaving the
//! alternate screen, and the two actions that hand it over to another program.

use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::app::{App, Mode};

/// Take over the screen. Mouse capture is not part of `ratatui::init`.
pub fn enter() -> Result<DefaultTerminal> {
    let terminal = ratatui::try_init()?;
    let _ = execute!(io::stdout(), EnableMouseCapture);
    Ok(terminal)
}

/// Hand the screen back, leaving the cursor where the shell expects it.
pub fn leave() {
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
}

/// A fresh terminal starts with an empty back buffer, so the next draw
/// repaints everything. `Terminal::clear` is deliberately not used: it queries
/// the cursor position, which hangs on terminals that never answer.
fn re_enter(terminal: &mut DefaultTerminal) -> Result<()> {
    *terminal = enter()?;
    Ok(())
}

/// Drop out of the alternate screen, run `just`, then come back. Running
/// inline rather than capturing output keeps interactive recipes working.
pub fn run_just(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    args: &[String],
    dir: &Path,
    dry: bool,
) -> Result<()> {
    leave();

    let mut stdout = io::stdout();
    let _ = writeln!(
        stdout,
        "\n\x1b[1;34m❯\x1b[0m \x1b[1mjust {}\x1b[0m{}\n",
        args.join(" "),
        if dry { "  \x1b[2m(dry run)\x1b[0m" } else { "" }
    );
    let _ = stdout.flush();

    match Command::new("just").args(args).current_dir(dir).status() {
        Ok(status) if status.success() => {
            let _ = writeln!(stdout, "\n\x1b[32m✓ finished\x1b[0m");
            app.info("recipe finished");
        }
        Ok(status) => {
            let code = status.code().unwrap_or(-1);
            let _ = writeln!(stdout, "\n\x1b[31m✗ exit code {code}\x1b[0m");
            app.error(format!("recipe exited with {code}"));
        }
        Err(err) => {
            let _ = writeln!(stdout, "\n\x1b[31m✗ {err}\x1b[0m");
            app.error(format!("could not run just: {err}"));
        }
    }

    let _ = writeln!(stdout, "\x1b[2mpress any key to return\x1b[0m");
    let _ = stdout.flush();
    wait_for_key();

    re_enter(terminal)?;
    app.mode = Mode::Normal;
    Ok(())
}

/// Open a file in `$EDITOR` at a line, then reload what changed.
pub fn edit(terminal: &mut DefaultTerminal, app: &mut App, path: &Path, line: usize) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());

    leave();
    let mut command = Command::new(&editor);
    if opens_at_line(&editor) {
        command.arg(format!("+{line}"));
    }
    let status = command.arg(path).status();
    re_enter(terminal)?;

    match status {
        Ok(_) => app.reload(),
        Err(err) => app.error(format!("could not launch {editor}: {err}")),
    }
    Ok(())
}

/// Editors that understand `+42` as "start on line 42".
fn opens_at_line(editor: &str) -> bool {
    let name = Path::new(editor)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    matches!(name.as_str(), "vi" | "vim" | "nvim" | "nano" | "kak" | "hx")
}

/// Block until a key is pressed, without disturbing what is on screen.
fn wait_for_key() {
    let _ = enable_raw_mode();
    loop {
        match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let _ = disable_raw_mode();
}

/// Copy through the terminal itself, so it works over ssh too.
pub fn copy_osc52(text: &str) {
    let mut stdout = io::stdout();
    let _ = write!(stdout, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = stdout.flush();
}

/// Small enough not to be worth a dependency.
fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(match chunk.len() > 1 {
            true => TABLE[(n >> 6) as usize & 63] as char,
            false => '=',
        });
        out.push(match chunk.len() > 2 {
            true => TABLE[n as usize & 63] as char,
            false => '=',
        });
    }
    out
}
