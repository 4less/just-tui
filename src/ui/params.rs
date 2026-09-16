//! The run-with-arguments form: one row per recipe parameter. The row
//! renderer is shared with the Slurm submit form.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, truncate};
use crate::app::App;
use crate::params::{ParamForm, quote};
use crate::theme;

const WIDTH: u16 = 78;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(form) = app.params.as_ref() else {
        return;
    };
    let target = app.run_target().unwrap_or_default();

    let width = WIDTH.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = (0..form.len())
        .map(|index| row(form, index, index == form.cursor, inner, None))
        .collect();
    lines.push(Line::default());
    lines.push(preview(&target, form, inner));
    if let Err(message) = form.args() {
        lines.push(problem(&message));
    }

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            &format!(" Run {target} "),
            " ↑↓ field · del clear · ⏎ run · esc cancel ",
            theme::MODULE,
        )),
        popup,
    );
}

/// One parameter: sigil and name, the typed value, and what an empty
/// value means. `origin` is shown on the selected row when given.
pub fn row(
    form: &ParamForm,
    index: usize,
    selected: bool,
    width: usize,
    origin: Option<&str>,
) -> Line<'static> {
    let param = &form.params[index];
    let value = &form.values[index];
    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!("{:<10}", format!("{}{}", param.sigil(), param.name)),
            Style::default().fg(theme::VARIABLE),
        ),
        match value.is_empty() {
            true => Span::styled(format!("{:<22}", "—"), Style::default().fg(theme::DIM)),
            false => Span::styled(format!("{value:<22}"), Style::default().fg(theme::STRING)),
        },
        Span::styled(
            if selected { "▏" } else { " " },
            Style::default().fg(theme::ACCENT),
        ),
    ];

    if selected && let Some(origin) = origin {
        spans.push(Span::styled(
            format!("{origin}  "),
            Style::default().fg(theme::MATCH),
        ));
    }

    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    spans.push(Span::styled(
        truncate(&param.hint(), width.saturating_sub(used)),
        theme::label(),
    ));
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}

/// `$ just target arg…`, as it will be run.
fn preview(target: &str, form: &ParamForm, width: usize) -> Line<'static> {
    let mut command = format!("just {target}");
    for arg in form.args_lenient() {
        command.push(' ');
        command.push_str(&quote(&arg));
    }
    Line::from(vec![
        Span::styled("   $ ", theme::label()),
        Span::styled(
            truncate(&command, width.saturating_sub(5)),
            Style::default().fg(theme::FG),
        ),
    ])
}

/// A validation message, in the same style as the submit form's warnings.
pub fn problem(message: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("   ! {message}"),
        Style::default()
            .fg(theme::INTERP)
            .add_modifier(Modifier::BOLD),
    ))
}
