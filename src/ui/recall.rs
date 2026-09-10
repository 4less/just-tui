//! The history overlay: past submissions, newest first, ready to be loaded
//! back into the submit form.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, truncate};
use crate::app::App;
use crate::history::Record;
use crate::theme;

/// The overlay takes what the terminal offers, up to this. Past runs of one
/// recipe differ only in their arguments, so the columns that hold them are
/// worth every column available.
const MAX_WIDTH: u16 = 170;
const ROWS: usize = 20;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pick) = app.picker.as_ref() else {
        return;
    };
    let records = app.picked_entries();
    let width = area
        .width
        .saturating_sub(6)
        .min(MAX_WIDTH)
        .min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(2) as usize;
    // The title already names the recipe when the list is limited to one, so
    // its rows spend that space on the arguments instead.
    let scoped = pick.namepath.is_some();

    // One column width for every row, wide enough for the longest entry that
    // will fit, so the settings beside them line up.
    let rest = inner.saturating_sub(FIXED + 2);
    let longest = records
        .iter()
        .map(|record| shown(record, scoped).chars().count())
        .max()
        .unwrap_or(0);
    // Never let the floor climb above the ceiling: on a narrow terminal
    // three-fifths of what is left can be less than the floor would like.
    let ceiling = rest * 3 / 5;
    let label = longest.clamp(ceiling.min(12), ceiling);

    // Scroll the window so the cursor stays in it.
    let first = pick.index.saturating_sub(ROWS.saturating_sub(1));
    let lines: Vec<Line> = records
        .iter()
        .enumerate()
        .skip(first)
        .take(ROWS)
        .map(|(index, record)| row(record, index == pick.index, scoped, label, inner))
        .collect();

    let title = match &pick.namepath {
        Some(name) => format!(" Past submissions of {name} ({}) ", records.len()),
        None => format!(" Past submissions ({}) ", records.len()),
    };

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            &title,
            " ↑↓ pick · ⏎ load these settings into the form · esc back ",
            theme::MATCH,
        )),
        popup,
    );
}

/// Columns before the two that flex: the cursor, the timestamp and the id.
const FIXED: usize = 3 + 20 + 10;

/// What tells one run of a recipe apart from the next. Inside a single
/// recipe's history that is the arguments alone; across recipes it is the
/// recipe as well.
fn shown(record: &Record, scoped: bool) -> String {
    let what = match scoped {
        true => record.settings.args.trim().to_owned(),
        false => record.label(),
    };
    match what.is_empty() {
        true => "(no arguments)".to_owned(),
        false => what,
    }
}

fn row(record: &Record, selected: bool, scoped: bool, label: usize, width: usize) -> Line<'static> {
    let what = shown(record, scoped);
    let empty = record.settings.args.trim().is_empty() && scoped;
    let settings = width.saturating_sub(FIXED + 2 + label);

    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!("{:<20}", truncate(&record.when, 19)),
            theme::label(),
        ),
        Span::styled(
            format!("{:<10}", truncate(&record.job_id, 9)),
            Style::default().fg(theme::VARIABLE),
        ),
        Span::styled(
            format!("{:<label$}", truncate(&what, label)),
            match empty {
                true => theme::label(),
                false => Style::default().fg(theme::RECIPE),
            },
        ),
        Span::raw("  "),
        Span::styled(
            truncate(&record.asked_for(), settings),
            Style::default().fg(theme::STRING),
        ),
    ];
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}
