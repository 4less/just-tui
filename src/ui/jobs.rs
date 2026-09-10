//! The Slurm job browser: the queue on top, the selected job's log below.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{overlay_block, pad, pane_block, truncate};
use crate::app::{App, LogKind};
use crate::slurm::{self, Job};
use crate::theme;

/// Rows the job list gets before the log pane takes the rest.
const LIST_ROWS: u16 = 12;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.jobs.as_ref() else {
        return;
    };

    let footer = format!(
        " ↑↓ job · ⇥ {} · ⏎ open log · u reuse settings · f {} · d {}d · r reload · esc back ",
        view.which.other().label(),
        view.filter.label(),
        view.days(),
    );
    let title = format!(
        " Slurm jobs — {} shown of {} in the last {} days ",
        view.visible.len(),
        view.list.jobs.len(),
        view.days(),
    );

    frame.render_widget(Clear, area);
    let block = overlay_block(&title, &footer, theme::ACCENT);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = LIST_ROWS.min(inner.height.saturating_sub(6)).max(3);
    let [list_area, detail_area, log_area] = Layout::vertical([
        Constraint::Length(rows),
        Constraint::Length(2),
        Constraint::Min(3),
    ])
    .areas(inner);

    draw_list(frame, app, list_area);
    draw_detail(frame, app, detail_area);
    draw_log(frame, app, log_area);
}

/// One row per job, scrolled to keep the cursor in view.
fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let Some(view) = app.jobs.as_ref() else {
        return;
    };
    let width = area.width as usize;

    if view.visible.is_empty() {
        let note = view
            .list
            .note
            .clone()
            .unwrap_or_else(|| format!("no {} jobs", view.filter.label()));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {note}"),
                Style::default().fg(theme::MATCH),
            ))),
            area,
        );
        return;
    }

    // Keep the cursor inside the window without a stateful list widget.
    let rows = area.height as usize;
    let first = view.cursor.saturating_sub(rows.saturating_sub(1));
    let lines: Vec<Line> = view
        .visible
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(index, &job)| row(&view.list.jobs[job], index == view.cursor, width))
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn row(job: &Job, selected: bool, width: usize) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!("{:<11}", job.id),
            Style::default().fg(theme::VARIABLE),
        ),
        Span::styled(
            format!("{:<11}", truncate(job.state_word(), 10)),
            Style::default().fg(state_color(job)),
        ),
        Span::styled(
            format!("{:<34}", truncate(&job.name, 33)),
            Style::default().fg(theme::FG),
        ),
        Span::styled(format!("{:<9}", truncate(&job.elapsed, 8)), theme::label()),
    ];

    let tail = match (job.exit_label().is_empty(), job.pending()) {
        (false, _) => job.exit_label(),
        (true, true) => job.nodes.clone(),
        (true, false) => job.partition.clone(),
    };
    spans.push(Span::styled(
        truncate(&tail, width.saturating_sub(68).max(6)),
        Style::default().fg(if job.failed() {
            theme::INTERP
        } else {
            theme::DIM
        }),
    ));
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}

fn state_color(job: &Job) -> Color {
    match job.state_word() {
        "RUNNING" => theme::RECIPE,
        "PENDING" | "CONFIGURING" | "REQUEUED" => theme::MATCH,
        "COMPLETED" => theme::DIM,
        _ if job.failed() => theme::INTERP,
        _ => theme::FG,
    }
}

/// What the job asked for, if just-tui is the one that submitted it.
fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;
    let lines = match app.job_record() {
        Some(record) => vec![
            Line::from(vec![
                Span::styled("   submitted ", theme::label()),
                Span::styled(record.when.clone(), Style::default().fg(theme::STRING)),
                Span::styled("  ·  ", theme::label()),
                Span::styled(record.label(), Style::default().fg(theme::RECIPE)),
                Span::styled("  ·  ", theme::label()),
                Span::styled(record.asked_for(), Style::default().fg(theme::VARIABLE)),
            ]),
            Line::from(vec![
                Span::styled("   $ ", theme::label()),
                Span::styled(
                    truncate(&record.command, width.saturating_sub(6)),
                    Style::default().fg(theme::DIM),
                ),
            ]),
        ],
        None => {
            let job = app.jobs.as_ref().and_then(|view| view.selected());
            vec![
                Line::from(Span::styled(
                    match job {
                        Some(job) if !job.work_dir.is_empty() => {
                            format!("   ran in {}", super::shorten_home(&job.work_dir))
                        }
                        _ => "   not submitted from just-tui — no settings recorded".to_owned(),
                    },
                    theme::label(),
                )),
                Line::default(),
            ]
        }
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// The tail of the selected log, with anything that looks like an error
/// picked out.
fn draw_log(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.jobs.as_mut() else {
        return;
    };
    view.height = area.height.saturating_sub(2);

    let path = view
        .shown_path()
        .map(|p| super::shorten_home(&p.display().to_string()))
        .unwrap_or_else(|| "no file".to_owned());
    let title = format!(
        " {} — {}{} ",
        view.which.label(),
        path,
        if view.truncated { "  (tail)" } else { "" }
    );

    let block = pane_block(title, true).border_style(Style::default().fg(match view.which {
        LogKind::Err => theme::INTERP,
        LogKind::Out => theme::BORDER,
    }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let first = view.scroll as usize;
    let lines: Vec<Line> = view
        .lines
        .iter()
        .skip(first)
        .take(inner.height as usize)
        .map(|text| {
            Line::from(Span::styled(
                truncate(text, inner.width as usize),
                match slurm::is_error_line(text) {
                    true => Style::default()
                        .fg(theme::INTERP)
                        .add_modifier(Modifier::BOLD),
                    false => Style::default().fg(theme::FG),
                },
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
