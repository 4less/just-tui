//! Rendering. One module per pane, plus the shared chrome and helpers.

mod code;
mod doc;
mod help;
mod jobs;
mod recall;
mod submit;
mod tree;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::{App, Mode, Pane, StatusKind};
use crate::theme;

/// Where each pane ended up, so mouse clicks can be routed to it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panes {
    pub tree: Rect,
    pub doc: Rect,
    pub code: Rect,
}

impl Panes {
    /// The pane covering a screen position, if any.
    pub fn at(&self, column: u16, row: u16) -> Option<Pane> {
        for (rect, pane) in [
            (self.tree, Pane::Tree),
            (self.doc, Pane::Doc),
            (self.code, Pane::Code),
        ] {
            if rect.contains((column, row).into()) {
                return Some(pane);
            }
        }
        None
    }

    /// The row index inside the tree pane's list, if the click landed there.
    pub fn tree_row(&self, column: u16, row: u16) -> Option<usize> {
        if self.at(column, row) != Some(Pane::Tree) {
            return None;
        }
        // The first row of the list sits just inside the top border.
        row.checked_sub(self.tree.y + 1).map(usize::from)
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, app, header);
    draw_body(frame, app, body);
    draw_footer(frame, app, footer);

    // Overlays, outermost last. The submit form stays behind the pickers it
    // opens, so its values are still readable while one is chosen.
    let over_submit = app
        .picker
        .as_ref()
        .is_some_and(|pick| pick.from == Mode::Submit);
    if matches!(app.mode, Mode::Submit | Mode::ConfigPick) || over_submit {
        submit::draw(frame, app, frame.area());
    }
    if app.mode == Mode::ConfigPick {
        submit::draw_config_pick(frame, app, frame.area());
    }
    if matches!(app.mode, Mode::Jobs | Mode::CancelJob) {
        // Stop one row short of the bottom: that line carries the status
        // messages the browser itself reports, and a full-screen overlay
        // would swallow them.
        let mut area = frame.area();
        area.height = area.height.saturating_sub(1);
        jobs::draw(frame, app, area);
    }
    if app.mode == Mode::CancelJob {
        jobs::draw_confirm(frame, app, frame.area());
    }
    if app.mode == Mode::History {
        recall::draw(frame, app, frame.area());
    }
    if app.mode == Mode::Help {
        help::draw(frame, frame.area());
    }
}

/// Tree on the left, documentation over source on the right — or one pane
/// filling the screen when zoomed.
fn draw_body(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.zoom {
        app.panes = Panes::default();
        match app.focus {
            Pane::Tree => {
                app.panes.tree = area;
                tree::draw(frame, app, area);
            }
            Pane::Doc => {
                app.panes.doc = area;
                doc::draw(frame, app, area);
            }
            Pane::Code => {
                app.panes.code = area;
                code::draw(frame, app, area);
            }
        }
        return;
    }

    let tree_width = (area.width as f32 * 0.32).round().clamp(26.0, 48.0) as u16;
    let [left, right] =
        Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(20)]).areas(area);

    let [doc_area, code_area] = if app.vertical_split {
        Layout::vertical([Constraint::Percentage(40), Constraint::Min(5)]).areas(right)
    } else {
        Layout::horizontal([Constraint::Percentage(45), Constraint::Min(20)]).areas(right)
    };

    app.panes = Panes {
        tree: left,
        doc: doc_area,
        code: code_area,
    };

    tree::draw(frame, app, left);
    doc::draw(frame, app, doc_area);
    code::draw(frame, app, code_area);
}

/// `just <path>` on the left, counts on the right.
fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let path = app
        .project()
        .path
        .as_ref()
        .map(|p| shorten_home(&p.display().to_string()))
        .unwrap_or_else(|| "<no justfile>".to_owned());

    let left = Line::from(vec![
        Span::styled(
            " just ",
            Style::default()
                .bg(theme::ACCENT)
                .fg(theme::BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(path, Style::default().fg(theme::FG)),
    ]);

    let modules = app.tree.count_modules();
    let globals = app.sources.iter().filter(|s| s.global).count();
    let mut counts = vec![
        count(app.tree.count_recipes(), "recipe", theme::RECIPE),
        count(modules, "module", theme::MODULE),
    ];
    if globals > 0 {
        counts.push(count(globals, "global", theme::ALIAS));
    }

    let mut right: Vec<Span> = counts.into_iter().flatten().collect();
    right.push(Span::styled("? help ", theme::label()));
    let right = Line::from(right);

    let [l, r] = Layout::horizontal([
        Constraint::Min(10),
        Constraint::Length(right.width() as u16),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(left), l);
    frame.render_widget(Paragraph::new(right), r);
}

/// `12 recipes `, pluralised.
fn count(number: usize, noun: &str, color: Color) -> [Span<'static>; 2] {
    let plural = if number == 1 { "" } else { "s" };
    [
        Span::styled(number.to_string(), Style::default().fg(color)),
        Span::styled(format!(" {noun}{plural}  "), theme::label()),
    ]
}

/// Key hints, the search or argument prompt, or the last status message.
fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = match app.mode {
        Mode::Search => prompt(
            " search ",
            theme::ACCENT,
            "",
            &app.search,
            "Enter accept · Esc clear",
        ),
        Mode::Args => prompt(
            " args ",
            theme::MODULE,
            &format!("just {} ", app.run_target().unwrap_or_default()),
            &app.args_input,
            "Enter run · Esc cancel",
        ),
        _ => match &app.status {
            Some((message, kind)) => {
                let color = match kind {
                    StatusKind::Info => theme::RECIPE,
                    StatusKind::Error => theme::INTERP,
                };
                Line::from(vec![
                    Span::styled(" ● ", Style::default().fg(color)),
                    Span::styled(message.clone(), Style::default().fg(color)),
                ])
            }
            None => hints(app),
        },
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn prompt(tag: &str, color: Color, prefix: &str, input: &str, help: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(tag.to_owned(), Style::default().bg(color).fg(theme::BG)),
        Span::raw(" "),
        Span::styled(prefix.to_owned(), Style::default().fg(theme::DIM)),
        Span::styled(input.to_owned(), Style::default().fg(theme::FG)),
        Span::styled("▏", Style::default().fg(theme::ACCENT)),
        Span::styled(format!("   {help}"), theme::label()),
    ])
}

fn hints(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for (key, label) in [
        ("↑↓", "move"),
        ("→←", "open/close"),
        ("⏎", "run"),
        ("s", "submit"),
        ("S", "jobs"),
        ("/", "search"),
        ("⇥", "pane"),
        ("?", "help"),
    ] {
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(label, theme::label()));
    }
    if app.zoom {
        spans.push(Span::styled("   [zoom]", Style::default().fg(theme::MATCH)));
    }
    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The bordered block every pane uses.
fn pane_block(title: impl Into<String>, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(Span::styled(title.into(), theme::title(focused)))
}

/// The bordered block every overlay uses.
fn overlay_block(title: &str, footer: &str, color: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            title.to_owned(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(footer.to_owned(), theme::label()))
}

/// An underlined heading inside the documentation pane.
fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_owned(),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))
}

/// `   key  value`, right-aligned key.
fn field(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:>8}  "), theme::label()),
        Span::styled(value.to_owned(), theme::value()),
    ])
}

/// `  name = value`, for settings and variables.
fn pair(key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key}"), Style::default().fg(theme::VARIABLE)),
        Span::styled(" = ", theme::label()),
        Span::styled(value.to_owned(), Style::default().fg(theme::STRING)),
    ])
}

fn badge(text: &str, color: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(color).bg(theme::SELECTION_BG),
    )
}

/// Cut `text` to `width` columns, marking the cut with an ellipsis.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Pad `spans` out to `width`, so a highlighted row fills its pane.
fn pad(spans: &mut Vec<Span<'static>>, width: usize) {
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
}

/// `/home/you/x` reads better as `~/x`.
pub fn shorten_home(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && path.starts_with(&home) => {
            format!("~{}", &path[home.len()..])
        }
        _ => path.to_owned(),
    }
}

/// A centred overlay of the given size, clipped to the screen.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    area.centered_horizontally(Constraint::Length(width.min(area.width)))
        .centered_vertically(Constraint::Length(height.min(area.height)))
}
