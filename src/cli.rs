//! The command line and the terminal event loop — the native front end.
//!
//! Kept in the library so `main.rs` is a stub that compiles to nothing under
//! WebAssembly, where there is no terminal to own and no arguments to parse.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};

use crate::app::{Action, App};
use crate::just::Loaded;
use crate::source::SourceCache;
use crate::{just, snapshot, terminal, tree, ui};

/// `0.2.0 (v0.2.0-4-g8a9c685, 2026-09-11)` — the release, then the commit it
/// was actually built from, which is what tells two installs apart.
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("JUST_TUI_COMMIT"),
    ", ",
    env!("JUST_TUI_DATE"),
    ")"
);

#[derive(Parser, Debug)]
#[command(
    name = "just-tui",
    version = VERSION,
    about = "Browse just recipes and modules with their source and documentation"
)]
struct Cli {
    /// Directory to search for a justfile (defaults to the current directory).
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,

    /// Use this justfile instead of searching.
    #[arg(short = 'f', long, value_name = "FILE")]
    justfile: Option<PathBuf>,

    /// Show private recipes (`_name` and `[private]`) from the start.
    #[arg(short = 'p', long)]
    private: bool,

    /// Do not load the global recipe library.
    #[arg(long)]
    no_global: bool,

    /// Directory holding global `.just` files (default: ~/.just-tui).
    #[arg(long, value_name = "DIR")]
    global_dir: Option<PathBuf>,

    /// List recipes to stdout and exit, without starting the TUI.
    #[arg(long)]
    list: bool,

    /// Render one frame as plain text and exit, e.g. `--snapshot 120x40`.
    #[arg(long, value_name = "WxH")]
    snapshot: Option<String>,

    /// Keys to press before taking the snapshot, e.g. `--keys "jjl"`.
    #[arg(long, value_name = "KEYS", requires = "snapshot")]
    keys: Option<String>,
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let (sources, warnings) = load_sources(&cli, &dir)?;
    let mut cache = SourceCache::default();

    if cli.list {
        for warning in &warnings {
            eprintln!("just-tui: {warning}");
        }
        return list_recipes(&sources, &mut cache, cli.private);
    }

    let mut app = App::new(sources, cache);
    if !cli.no_global {
        app.set_global_dir(just::global_dir(cli.global_dir.as_deref()));
    }
    // The terminal is about to be taken over, so stderr would be lost.
    if !warnings.is_empty() {
        // The status line holds one line; just's own errors run to several.
        let first_lines: Vec<&str> = warnings
            .iter()
            .map(|w| w.lines().next().unwrap_or_default())
            .collect();
        app.error(first_lines.join("; "));
    }
    app.show_private = cli.private;
    app.refresh_visible();

    if let Some(size) = &cli.snapshot {
        if let Some(keys) = &cli.keys {
            snapshot::press(&mut app, keys);
        }
        return snapshot::render(&mut app, size);
    }

    let mut terminal = terminal::enter()?;
    let result = run(&mut terminal, &mut app);
    terminal::leave();
    result
}

/// The project justfile first, then any global recipe files. Either may be
/// missing, but not both. Problems worth knowing about that did not stop the
/// load come back as warnings.
fn load_sources(cli: &Cli, dir: &Path) -> Result<(Vec<Loaded>, Vec<String>)> {
    // Otherwise `just` cannot even be started there, which reads as `just`
    // not being installed.
    if !dir.is_dir() {
        bail!("{} is not a directory", dir.display());
    }

    let mut sources = Vec::new();
    let mut warnings = Vec::new();

    match just::load(dir, cli.justfile.as_deref()) {
        Ok(project) => sources.push(project),
        Err(err) if cli.no_global => return Err(err),
        // No justfile here is the normal case for running global recipes;
        // the header says so. A justfile that is there but broken is not.
        Err(err) if just::is_missing_justfile(&err) => {}
        Err(err) => warnings.push(err.to_string()),
    }

    let global_dir = just::global_dir(cli.global_dir.as_deref());
    if !cli.no_global {
        let (globals, skipped) = just::load_globals(global_dir.as_deref(), dir);
        sources.extend(globals);
        warnings.extend(skipped);
    }

    if sources.is_empty() {
        let where_ = global_dir
            .map(|d| ui::shorten_home(&d.display().to_string()))
            .unwrap_or_else(|| "~/.just-tui".to_owned());
        let mut message = format!(
            "no justfile in {}, and no global recipes in {where_}",
            ui::shorten_home(&dir.display().to_string())
        );
        for warning in &warnings {
            message.push_str("\n  ");
            message.push_str(warning);
        }
        bail!(message);
    }
    Ok((sources, warnings))
}

/// `--list`: one line per recipe, global ones marked with `~`.
fn list_recipes(sources: &[Loaded], cache: &mut SourceCache, private: bool) -> Result<()> {
    let tree = tree::Tree::build(sources, cache);
    let mut out = io::stdout().lock();

    for node in &tree.nodes {
        if node.kind != tree::Kind::Recipe || (node.private && !private) {
            continue;
        }
        let signature = node
            .info
            .as_ref()
            .map(|info| info.recipe.signature())
            .unwrap_or_else(|| node.name.clone());
        let doc = node.summary.clone().unwrap_or_default();
        let mark = if node.global { "~ " } else { "  " };

        // A closed pipe (`| head`) is not an error worth reporting.
        if writeln!(out, "{mark}{signature:<44} {doc}").is_err() {
            break;
        }
    }
    Ok(())
}

/// Draw, wait for input, act. Actions that need the real terminal are carried
/// out here rather than inside [`App`].
fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        // Anything a background thread has finished is taken up before the
        // frame it belongs in is drawn.
        app.poll_background();
        app.poll_cluster();
        terminal.draw(|frame| ui::draw(frame, app))?;

        // While the job browser is open the clock has to move on its own, so
        // input is waited for with a timeout rather than blocked on.
        let event = match app.tick_interval() {
            Some(timeout) if !event::poll(timeout)? => {
                app.tick();
                continue;
            }
            _ => event::read()?,
        };

        let action = match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
            Event::Mouse(mouse) => {
                handle_mouse(app, mouse);
                Action::None
            }
            _ => Action::None,
        };

        match action {
            Action::None => {}
            Action::Quit => return Ok(()),
            Action::Run { commands, dir, dry } => {
                terminal::run_just(terminal, app, &commands, &dir, dry)?
            }
            Action::Edit { path, line } => terminal::edit(terminal, app, &path, line)?,
            Action::View { path } => terminal::view(terminal, app, &path)?,
            Action::Copy(text) => {
                terminal::copy_osc52(&text);
                app.info("recipe copied to clipboard");
            }
        }

        if app.quit {
            return Ok(());
        }
    }
}

/// Scrolling acts on whichever pane is under the pointer; a click focuses that
/// pane, and in the tree it also selects the row.
fn handle_mouse(app: &mut App, mouse: event::MouseEvent) {
    let (column, row) = (mouse.column, mouse.row);
    if let Some(pane) = app.panes.at(column, row) {
        app.focus = pane;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => app.scroll(3),
        MouseEventKind::ScrollUp => app.scroll(-3),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(row) = app.panes.tree_row(column, row) {
                app.select_visible(row);
            }
        }
        _ => {}
    }
}
