//! The Slurm submit form and the config-file chooser.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, shorten_home, truncate};
use crate::app::App;
use crate::config::CONFIG_NAME;
use crate::slurm::{self, Cluster, Field, Partition};
use crate::submit::SubmitForm;
use crate::theme;

/// The form is happiest narrow, but the `sbatch` line under it grows with
/// every flag, so it takes more when the terminal has it to give.
const WIDTH: u16 = 78;
const MAX_WIDTH: u16 = 130;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let Some(form) = app.form.as_ref() else {
        return;
    };
    let fallback = Cluster::default();
    let cluster = app.cluster.as_ref().unwrap_or(&fallback);

    let width = area
        .width
        .saturating_sub(6)
        .clamp(WIDTH.min(area.width.saturating_sub(2)), MAX_WIDTH)
        .min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = slurm::FIELDS
        .iter()
        .enumerate()
        .map(|(index, field)| {
            field_row(
                form,
                cluster,
                *field,
                index == form.field,
                inner,
                app.detecting.is_some(),
            )
        })
        .collect();

    lines.push(Line::default());
    lines.extend(summary(form, cluster, inner));

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            &format!(" Submit {} to Slurm ", form.namepath),
            " ↑↓ field · ←→ pick · ⏎ submit · F2/F3/F4 save · F5 edit · F6 history ",
            theme::ACCENT,
        )),
        popup,
    );
}

/// One editable row: label, value, and either the pick-list limits or a hint.
fn field_row(
    form: &SubmitForm,
    cluster: &Cluster,
    field: Field,
    selected: bool,
    width: usize,
    detecting: bool,
) -> Line<'static> {
    let value = form.settings.get(field);
    let mut spans = vec![
        Span::styled(
            if selected { " ▸ " } else { "   " },
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            format!("{:<10}", field.label()),
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

    // Only the selected row has space to say where its value came from.
    if selected && let Some(origin) = form.origin() {
        spans.push(Span::styled(
            format!("{origin}  "),
            Style::default().fg(theme::MATCH),
        ));
    }

    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    spans.push(Span::styled(
        truncate(
            &note(form, cluster, field, value, detecting),
            width.saturating_sub(used),
        ),
        theme::label(),
    ));
    pad(&mut spans, width);

    let line = Line::from(spans);
    match selected {
        true => line.style(Style::default().bg(theme::SELECTION_BG)),
        false => line,
    }
}

/// What to show to the right of a field: detected choices, or its hint.
fn note(
    form: &SubmitForm,
    cluster: &Cluster,
    field: Field,
    value: &str,
    detecting: bool,
) -> String {
    // `each` takes over the range, leaving this field good only for a
    // throttle. Saying so beats a hint that is no longer true.
    if field == Field::Array && form.plan.is_some() {
        return "throttle only, e.g. %4 — each sets the range".to_owned();
    }
    if field == Field::Each
        && let Some(plan) = form.plan.as_ref()
    {
        return format!("{} matches", plan.count());
    }
    let choices = cluster.choices(field);
    if choices.is_empty() {
        return match field {
            Field::Partition if detecting => "asking the cluster…".to_owned(),
            Field::Partition => "no partitions detected".to_owned(),
            Field::Name if value.is_empty() => "auto: recipe + args".to_owned(),
            other => other.hint().to_owned(),
        };
    }
    let detail = cluster
        .partition(value)
        .map(Partition::limits)
        .unwrap_or_else(|| format!("{} available", choices.len()));
    format!("◂ ▸  {detail}")
}

/// Log path, the command as it will be run, warnings, and provenance.
fn summary(form: &SubmitForm, cluster: &Cluster, width: usize) -> Vec<Line<'static>> {
    let (out, _) = slurm::log_paths(&form.namepath, &form.settings);
    let command = slurm::preview_command(&form.base, &form.namepath, &form.settings, form.batch());

    let mut lines = Vec::new();

    // An expansion changes what "submitting" means, so it is said plainly and
    // before anything else.
    if let Some(plan) = form.plan.as_ref() {
        lines.push(Line::from(vec![
            Span::styled("   expands   ", theme::label()),
            Span::styled(
                format!("{} jobs", plan.count()),
                Style::default()
                    .fg(theme::MATCH)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  first: ", theme::label()),
            Span::styled(
                truncate(
                    plan.lines.first().map(String::as_str).unwrap_or(""),
                    width.saturating_sub(34),
                ),
                Style::default().fg(theme::STRING),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   manifest  ", theme::label()),
            Span::styled(
                plan.manifest.display().to_string(),
                Style::default().fg(theme::RECIPE),
            ),
        ]));
    }
    if let Some(problem) = form.plan_error.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("   ! each: {problem}"),
            Style::default()
                .fg(theme::INTERP)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.extend(vec![
        Line::from(vec![
            Span::styled("   job name  ", theme::label()),
            Span::styled(
                slurm::job_name(&form.namepath, &form.settings),
                Style::default().fg(theme::VARIABLE),
            ),
        ]),
        Line::from(vec![
            Span::styled("   logs      ", theme::label()),
            Span::styled(
                out.display().to_string(),
                Style::default().fg(theme::RECIPE),
            ),
        ]),
        Line::from(vec![
            Span::styled("   $ ", theme::label()),
            Span::styled(
                truncate(&command, width.saturating_sub(5)),
                Style::default().fg(theme::FG),
            ),
        ]),
    ]);

    // Anything but a bare throttle in the array field is dropped when `each`
    // is doing the expanding.
    let array = form.settings.array.trim();
    if form.plan.is_some() && !array.is_empty() && !array.starts_with('%') {
        lines.push(Line::from(Span::styled(
            format!("   ! array `{array}` is ignored — each sets the range; write %n to throttle"),
            Style::default().fg(theme::MATCH),
        )));
    }

    for warning in slurm::warnings(cluster, &form.settings) {
        lines.push(Line::from(Span::styled(
            format!("   ! {warning}"),
            Style::default()
                .fg(theme::INTERP)
                .add_modifier(Modifier::BOLD),
        )));
    }
    if let Some(note) = &cluster.note {
        lines.push(Line::from(Span::styled(
            format!("   ! {note}"),
            Style::default().fg(theme::MATCH),
        )));
    }

    lines.push(Line::from(Span::styled(
        format!("   {}", form.provenance()),
        theme::label(),
    )));
    lines
}

/// Which `.just-tui-cluster-config` to open in `$EDITOR`.
pub fn draw_config_pick(frame: &mut Frame, app: &App, area: Rect) {
    let Some(form) = app.form.as_ref() else {
        return;
    };

    let lines: Vec<Line> = form
        .scopes
        .iter()
        .enumerate()
        .map(|(index, (label, dir))| {
            let path = dir.join(CONFIG_NAME);
            let exists = path.exists();
            Line::from(vec![
                Span::styled(
                    if index == form.scope_index {
                        " ▸ "
                    } else {
                        "   "
                    },
                    Style::default().fg(theme::ACCENT),
                ),
                Span::styled(format!("{label:<9}"), Style::default().fg(theme::VARIABLE)),
                Span::styled(
                    shorten_home(&path.display().to_string()),
                    Style::default().fg(if exists { theme::STRING } else { theme::DIM }),
                ),
                Span::styled(
                    if exists { "" } else { "   (will be created)" },
                    theme::label(),
                ),
            ])
        })
        .collect();

    let popup = centered(area, 74, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            " Edit which config? ",
            " ↑↓ scope · ⏎ open in $EDITOR · esc back ",
            theme::MATCH,
        )),
        popup,
    );
}
