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

const WIDTH: u16 = 96;
const ROWS: usize = 16;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pick) = app.picker.as_ref() else {
        return;
    };
    let records = app.picked_entries();
    let width = WIDTH.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(2) as usize;

    // Scroll the window so the cursor stays in it.
    let first = pick.index.saturating_sub(ROWS.saturating_sub(1));
    let lines: Vec<Line> = records
        .iter()
        .enumerate()
        .skip(first)
        .take(ROWS)
        .map(|(index, record)| row(record, index == pick.index, inner))
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

fn row(record: &Record, selected: bool, width: usize) -> Line<'static> {
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
            format!("{:<9}", truncate(&record.job_id, 8)),
            Style::default().fg(theme::VARIABLE),
        ),
        Span::styled(
            format!("{:<30}", truncate(&record.label(), 29)),
            Style::default().fg(theme::RECIPE),
        ),
    ];
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    spans.push(Span::styled(
        truncate(&record.asked_for(), width.saturating_sub(used)),
        Style::default().fg(theme::STRING),
    ));
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}
