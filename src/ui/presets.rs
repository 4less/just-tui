//! The presets window over the submit form: saved presets, and the ones
//! read off the partitions that are not saved yet.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, truncate};
use crate::app::{App, PresetRow};
use crate::config::PRESETS_NAME;
use crate::theme;

const WIDTH: u16 = 96;
const ROWS: usize = 16;
/// The cursor, the kind, and the name.
const NAME: usize = 18;
const FIXED: usize = 3 + 7 + NAME + 2;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pick) = app.preset_pick.as_ref() else {
        return;
    };
    let rows = app.preset_rows();
    let width = WIDTH.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(2) as usize;

    let first = pick.index.saturating_sub(ROWS.saturating_sub(1));
    let mut lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(ROWS)
        .map(|(index, row)| line(row, index == pick.index, inner))
        .collect();
    if rows.is_empty() {
        let why = match app.cluster.as_ref() {
            Some(cluster) if cluster.detected => "no presets yet — e starts the file",
            Some(_) => {
                "no presets yet, and Slurm has no partitions to infer from — e starts the file"
            }
            None => "no presets yet, and Slurm has not answered — e starts the file",
        };
        lines.push(Line::from(Span::styled(
            format!("   {why}"),
            theme::label(),
        )));
    }

    let saved = rows.iter().filter(|row| !row.inferred).count();
    let title = format!(
        " Presets · {saved} in ~/.just-tui/{PRESETS_NAME} · {} from Slurm ",
        rows.len() - saved
    );
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            &title,
            " ⏎ apply · e edit the file · i save this one · I save all from Slurm · esc back ",
            theme::MATCH,
        )),
        popup,
    );
}

fn line(row: &PresetRow, selected: bool, width: usize) -> Line<'static> {
    let (kind, color) = match row.inferred {
        true => ("slurm", theme::ATTRIBUTE),
        false => ("saved", theme::MATCH),
    };
    let mut summary = row.settings.asked_for();
    if !row.settings.account.is_empty() {
        summary = match summary.is_empty() {
            true => format!("account {}", row.settings.account),
            false => format!("{summary} · account {}", row.settings.account),
        };
    }

    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(format!("{kind:<7}"), Style::default().fg(color)),
        Span::styled(
            format!("{:<NAME$}", truncate(&row.name, NAME - 1)),
            Style::default().fg(theme::RECIPE),
        ),
        Span::raw("  "),
        Span::styled(
            truncate(&summary, width.saturating_sub(FIXED)),
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
