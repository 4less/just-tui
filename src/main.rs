//! just-tui — a file-explorer style browser for `just` recipes.

mod app;
mod config;
mod highlight;
mod just;
mod model;
mod slurm;
mod snapshot;
mod source;
mod submit;
mod terminal;
#[cfg(test)]
mod tests;
mod theme;
mod tree;
mod ui;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseButton, MouseEventKind};

use crate::app::{Action, App};
use crate::just::Loaded;
use crate::source::SourceCache;

#[derive(Parser, Debug)]
#[command(
    name = "just-tui",
    version,
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

    /// Directory holding global `.just` files (default: ~/.justx).
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = cli
        .dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let sources = load_sources(&cli, &dir)?;
    let mut cache = SourceCache::default();

    if cli.list {
        return list_recipes(&sources, &mut cache, cli.private);
    }

    let mut app = App::new(sources, cache);
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
/// missing, but not both.
fn load_sources(cli: &Cli, dir: &Path) -> Result<Vec<Loaded>> {
    let mut sources = Vec::new();

    match just::load(dir, cli.justfile.as_deref()) {
        Ok(project) => sources.push(project),
        Err(err) if cli.no_global => return Err(err),
        // Without a project justfile the global library is still worth showing.
        Err(err) => eprintln!("just-tui: {err}"),
    }

    if !cli.no_global {
        let global_dir = just::global_dir(cli.global_dir.as_deref());
        sources.extend(just::load_globals(global_dir.as_deref(), dir));
    }

    if sources.is_empty() {
        bail!("no justfile here, and no global recipes in ~/.justx");
    }
    Ok(sources)
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
        terminal.draw(|frame| ui::draw(frame, app))?;

        let action = match event::read()? {
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
            Action::Run { args, dir, dry } => terminal::run_just(terminal, app, &args, &dir, dry)?,
            Action::Edit { path, line } => terminal::edit(terminal, app, &path, line)?,
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
