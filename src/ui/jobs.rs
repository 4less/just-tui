//! The Slurm job browser: the queue on top, the selected job's log below.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, pane_block, truncate};
use crate::app::{App, LogKind};
use crate::slurm::{self, Job, Usage, format_mem};
use crate::theme;

/// Columns the row spends on everything but the job's name.
const FIXED: usize = 67;

/// Shown while a log is being found and read on another thread.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Rows the job list gets before the log pane takes the rest.
const LIST_ROWS: u16 = 12;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.jobs.as_ref() else {
        return;
    };

    let footer = format!(
        " ↑↓ · ⇥ {} · ⏎ log · u reuse · s rerun · x kill · X kill+rerun · f {} · d {}d · p {} · esc ",
        view.which.other().label(),
        view.filter.label(),
        view.days(),
        if view.auto { "pause" } else { "resume" },
    );
    let title = format!(
        " Slurm jobs — {} shown of {} in the last {} days — {} ",
        view.visible.len(),
        view.list.jobs.len(),
        view.days(),
        if view.auto { "live" } else { "paused" },
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
        .map(|(index, &job)| {
            let job = &view.list.jobs[job];
            row(
                job,
                view.usage.get(&job.id).copied(),
                index == view.cursor,
                view.since_fetch(),
                width,
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn row(job: &Job, usage: Option<Usage>, selected: bool, since: u64, width: usize) -> Line<'static> {
    let name = width.saturating_sub(FIXED).max(8);
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
            format!("{:<11}", truncate(&state_label(job), 10)),
            Style::default().fg(state_color(job)),
        ),
        Span::styled(
            format!("{:<width$}", truncate(&job.name, name), width = name + 1),
            Style::default().fg(theme::FG),
        ),
        Span::styled(format!("{:<11}", job.elapsed_now(since)), theme::label()),
    ];
    spans.extend(stats(job, usage));
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}

/// The right-hand columns: what a running job is using, and for anything else
/// how it ended or why it has not started.
fn stats(job: &Job, usage: Option<Usage>) -> Vec<Span<'static>> {
    if job.state_word() != "RUNNING" {
        let tail = match (job.exit_label().is_empty(), job.pending()) {
            (false, _) => job.exit_label(),
            (true, true) => job.nodes.clone(),
            (true, false) => job.partition.clone(),
        };
        return vec![Span::styled(
            tail,
            Style::default().fg(if job.failed() {
                theme::INTERP
            } else {
                theme::DIM
            }),
        )];
    }

    let usage = usage.unwrap_or_default();
    let alloc = job
        .alloc_mem_mb
        .map(format_mem)
        .unwrap_or_else(|| "?".to_owned());
    let used = usage
        .max_rss_mb
        .map(format_mem)
        .unwrap_or_else(|| "—".to_owned());

    vec![
        Span::styled(
            format!("{used:>6}/{alloc:<6}"),
            Style::default().fg(theme::FG),
        ),
        percent(usage.mem_percent(job), true),
        Span::styled(format!("  {:>3}c ", job.cpus), theme::label()),
        percent(usage.cpu_percent(job), false),
    ]
}

/// A usage figure, coloured by how comfortable it is. Memory reads the way it
/// does in `seff` — high is close to the limit — while CPU is the other way
/// round: a low figure means allocated cores are sitting idle.
fn percent(value: Option<f64>, memory: bool) -> Span<'static> {
    let Some(value) = value else {
        return Span::styled("   —", theme::label());
    };
    let strain = if memory { value } else { 100.0 - value };
    let color = match strain {
        s if s < 20.0 => theme::RECIPE,
        s if s <= 80.0 => theme::MATCH,
        _ => theme::INTERP,
    };
    Span::styled(format!("{value:>4.0}%"), Style::default().fg(color))
}

/// Slurm's longest state name does not fit, and everyone calls it OOM.
fn state_label(job: &Job) -> String {
    match job.state_word() {
        "OUT_OF_MEMORY" => "OOM".to_owned(),
        other => other.to_owned(),
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

/// What the job asked for, if just-tui is the one that submitted it, and what
/// it is using if it is still running.
fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;
    if let Some((job, usage)) = app.job_usage() {
        let since = app.jobs.as_ref().map_or(0, |view| view.since_fetch());
        let lines = vec![
            Line::from(vec![
                Span::styled("   using    ", theme::label()),
                Span::styled(
                    format!(
                        "{} of {}",
                        usage.max_rss_mb.map(format_mem).unwrap_or("—".to_owned()),
                        job.alloc_mem_mb.map(format_mem).unwrap_or("?".to_owned()),
                    ),
                    Style::default().fg(theme::FG),
                ),
                Span::raw(" "),
                percent(usage.mem_percent(job), true),
                Span::styled("  ·  cpu ", theme::label()),
                percent(usage.cpu_percent(job), false),
                Span::styled(format!(" of {} cores", job.cpus), theme::label()),
                Span::styled("  ·  ", theme::label()),
                Span::styled(job.elapsed_now(since), Style::default().fg(theme::RECIPE)),
                Span::styled(" on ", theme::label()),
                Span::styled(job.nodes.clone(), Style::default().fg(theme::VARIABLE)),
            ]),
            match app.job_record() {
                Some(record) => Line::from(vec![
                    Span::styled("   $ ", theme::label()),
                    Span::styled(
                        truncate(&record.command, width.saturating_sub(6)),
                        Style::default().fg(theme::DIM),
                    ),
                ]),
                None => Line::default(),
            },
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let lines = match app.job_record() {
        Some(record) => vec![
            Line::from(vec![
                Span::styled("   submitted ", theme::label()),
                Span::styled(record.when.clone(), Style::default().fg(theme::STRING)),
                Span::styled("  ·  ", theme::label()),
                Span::styled(task_label(app, record), Style::default().fg(theme::RECIPE)),
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
/// picked out. While the loader is still working the pane says so and spins,
/// rather than freezing the list behind it.
fn draw_log(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(view) = app.jobs.as_mut() else {
        return;
    };
    view.height = area.height.saturating_sub(2);
    let loading = view.loading();

    let path = view
        .shown_path()
        .map(|p| super::shorten_home(&p.display().to_string()))
        .unwrap_or_else(|| "no file".to_owned());
    let title = match loading {
        true => format!(
            " {} {} reading… ",
            SPINNER[view.frame % SPINNER.len()],
            view.which.label(),
        ),
        false => format!(
            " {} — {}{} ",
            view.which.label(),
            path,
            if view.truncated { "  (tail)" } else { "" }
        ),
    };

    let block =
        pane_block(title, true).border_style(Style::default().fg(match (loading, view.which) {
            (true, _) => theme::DIM,
            (false, LogKind::Err) => theme::INTERP,
            (false, LogKind::Out) => theme::BORDER,
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if loading {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(
                    " {} looking for the log — the list stays live",
                    SPINNER[view.frame % SPINNER.len()]
                ),
                Style::default().fg(theme::DIM),
            ))),
            inner,
        );
        return;
    }

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

/// Anything that reaches the scheduler asks first, and takes nothing but `y`
/// for an answer.
pub fn draw_confirm(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pending) = app.pending.as_ref() else {
        return;
    };
    let width = 72.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                if pending.kill { "  scancel " } else { "  job " },
                theme::label(),
            ),
            Span::styled(
                pending.id.clone(),
                Style::default()
                    .fg(theme::INTERP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ", theme::label()),
            Span::styled(
                truncate(&pending.name, inner.saturating_sub(20)),
                Style::default().fg(theme::FG),
            ),
        ]),
        Line::default(),
    ];

    if pending.resubmit {
        let same = app
            .job_record()
            .map(|record| record.command.clone())
            .unwrap_or_default();
        lines.push(Line::from(Span::styled(
            match pending.kill {
                true => "  then submit the same job again:",
                false => "  goes back on the queue exactly as it was:",
            },
            theme::label(),
        )));
        lines.push(Line::from(Span::styled(
            format!("  $ {}", truncate(&same, inner.saturating_sub(4))),
            Style::default().fg(theme::STRING),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  The job stops. Whatever it has written so far is kept.",
            theme::label(),
        )));
    }

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            pending.title(),
            " y do it · any other key leaves the job alone ",
            theme::INTERP,
        )),
        popup,
    );
}

/// What the selected job was doing. For one task of an expansion that is its
/// own line of the manifest, not the whole recipe's arguments, which are the
/// same for every task and so tell you nothing.
fn task_label(app: &App, record: &crate::history::Record) -> String {
    let Some(job) = app.jobs.as_ref().and_then(|view| view.selected()) else {
        return record.label();
    };
    let task = job
        .id
        .split_once('_')
        .and_then(|(_, index)| index.parse::<usize>().ok());

    match (task, record.manifest.is_empty()) {
        (Some(task), false) => {
            match crate::batch::line(std::path::Path::new(&record.base), &record.manifest, task) {
                Some(args) => format!("{} {args}", record.namepath),
                None => format!("{} task {task}", record.namepath),
            }
        }
        _ => record.label(),
    }
}
