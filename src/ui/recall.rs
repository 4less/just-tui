//! The history overlay: past submissions, newest first, ready to be loaded
//! back into the submit form.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, truncate};
use crate::app::{App, Choice};
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
        .map(|choice| columns(choice, scoped).what.chars().count())
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
        .map(|(index, choice)| row(choice, index == pick.index, scoped, label, inner))
        .collect();

    let title = match &pick.namepath {
        Some(name) => format!(" Settings for {name} ({}) ", records.len()),
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

/// A row's text, column by column.
struct Columns {
    when: String,
    id: String,
    what: String,
    /// `what` is a placeholder, drawn dim.
    empty: bool,
    settings: String,
}

/// What tells one row apart from the next. Inside a single recipe's history
/// that is the arguments alone; across recipes it is the recipe as well; a
/// preset is its name.
fn columns(choice: &Choice, scoped: bool) -> Columns {
    match choice {
        Choice::Past(record) => {
            let what = match scoped {
                true => record.settings.args.trim().to_owned(),
                false => record.label(),
            };
            Columns {
                when: record.when.clone(),
                id: record.job_id.clone(),
                empty: what.is_empty(),
                what: match what.is_empty() {
                    true => "(no arguments)".to_owned(),
                    false => what,
                },
                settings: record.asked_for(),
            }
        }
        Choice::Preset(name, settings) => Columns {
            when: "preset".to_owned(),
            id: String::new(),
            what: (*name).to_owned(),
            empty: false,
            settings: settings.asked_for(),
        },
        Choice::Cluster(name, settings) => Columns {
            when: "cluster".to_owned(),
            id: String::new(),
            what: name.clone(),
            empty: false,
            settings: settings.asked_for(),
        },
        Choice::Other(other) => Columns {
            when: other.when.to_owned(),
            id: match other.job_id.is_empty() {
                true => "not sent".to_owned(),
                false => other.job_id.to_owned(),
            },
            what: format!("from {}", other.namepath),
            empty: false,
            settings: other.settings.asked_for(),
        },
    }
}

fn row(choice: &Choice, selected: bool, scoped: bool, label: usize, width: usize) -> Line<'static> {
    let row = columns(choice, scoped);
    let settings = width.saturating_sub(FIXED + 2 + label);
    let what_style = match choice {
        _ if row.empty => theme::label(),
        Choice::Past(_) => Style::default().fg(theme::RECIPE),
        Choice::Preset(..) => Style::default().fg(theme::MATCH),
        Choice::Cluster(..) => Style::default().fg(theme::ATTRIBUTE),
        Choice::Other(_) => Style::default().fg(theme::MODULE),
    };
    let when_style = match choice {
        Choice::Preset(..) => Style::default().fg(theme::MATCH),
        Choice::Cluster(..) => Style::default().fg(theme::ATTRIBUTE),
        _ => theme::label(),
    };

    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(format!("{:<20}", truncate(&row.when, 19)), when_style),
        Span::styled(
            format!("{:<10}", truncate(&row.id, 9)),
            Style::default().fg(theme::VARIABLE),
        ),
        Span::styled(
            format!("{:<label$}", truncate(&row.what, label)),
            what_style,
        ),
        Span::raw("  "),
        Span::styled(
            truncate(&row.settings, settings),
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
