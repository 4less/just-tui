//! The Slurm submit form and the config-file chooser.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{centered, overlay_block, pad, shorten_home, truncate};
use crate::app::App;
use crate::config::CONFIG_NAME;
use crate::slurm::{self, Cluster, Field, Partition, Waits};
use crate::submit::{ChainForm, SubmitForm, leaf};
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

    // A row of a chain names its place in the walk, and what it waits on.
    let step = app
        .chain
        .as_ref()
        .and_then(|chain| chain.step.map(|step| (chain, step)));
    let (title, footer) = match step {
        Some((chain, step)) => {
            let waits = chain.waits_label(step);
            let title = match waits.is_empty() {
                true => format!(
                    " step {} of {} · {} ",
                    step + 1,
                    chain.rows.len(),
                    form.namepath
                ),
                false => format!(
                    " step {} of {} · {} · {waits} ",
                    step + 1,
                    chain.rows.len(),
                    form.namepath
                ),
            };
            (
                title,
                " ↑↓ ←→ move · ^u clear · ⏎ accept · F2-F4 save · F7 after · esc back to the chain ",
            )
        }
        None => (
            format!(" Submit {} to Slurm ", form.namepath),
            " ↑↓ ←→ move · ^u clear · ⏎ submit · F2-F4 save · F5 edit · F6 history · F7 after ",
        ),
    };

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
    // What it waits on: in a chain the rows before it, by name until they
    // have ids, then whatever queued jobs were picked in this form.
    let mut after = step.map_or(Vec::new(), |(chain, step)| chain.row_placeholders(step));
    after.extend(form.after.iter().cloned());
    let waits = match step.is_some() {
        true => Waits::link(&after),
        false => Waits::after(&after),
    };
    lines.extend(summary(form, cluster, inner, waits));
    if let Some((chain, step)) = step {
        lines.push(progress_line(chain, step, inner));
    }

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(&title, footer, theme::ACCENT)),
        popup,
    );
}

/// `done: tree · s00_fetch    next: s02_refilter`, so a reader in the middle
/// of a chain knows where they are.
fn progress_line(chain: &ChainForm, step: usize, width: usize) -> Line<'static> {
    let done: Vec<&str> = chain.rows[..step]
        .iter()
        .filter(|row| row.checked)
        .map(|row| leaf(&row.link.namepath))
        .collect();
    let next: Vec<&str> = chain.rows[step + 1..]
        .iter()
        .filter(|row| row.checked)
        .map(|row| leaf(&row.link.namepath))
        .collect();
    let mut spans = vec![Span::styled("   done: ", theme::label())];
    spans.push(Span::styled(
        match done.is_empty() {
            true => "—".to_owned(),
            false => done.join(" · "),
        },
        Style::default().fg(theme::DIM),
    ));
    spans.push(Span::styled("    next: ", theme::label()));
    spans.push(Span::styled(
        match next.is_empty() {
            true => "confirm".to_owned(),
            false => next.join(" · "),
        },
        Style::default().fg(theme::MATCH),
    ));
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    match text.chars().count() > width {
        true => Line::from(Span::styled(truncate(&text, width), theme::label())),
        false => Line::from(spans),
    }
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
    ];
    spans.extend(value_spans(value, selected.then_some(form.cursor)));
    spans.push(Span::raw(" "));

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

/// Columns a field's value gets before it has to scroll under the caret.
const VALUE: usize = 22;

/// The value, with the character under the caret picked out. A value longer
/// than its column scrolls, so the caret stays visible wherever it is.
fn value_spans(value: &str, cursor: Option<usize>) -> Vec<Span<'static>> {
    let Some(cursor) = cursor else {
        return vec![match value.is_empty() {
            true => Span::styled(format!("{:<VALUE$}", "—"), Style::default().fg(theme::DIM)),
            false => Span::styled(
                format!("{:<VALUE$}", truncate(value, VALUE)),
                Style::default().fg(theme::STRING),
            ),
        }];
    };

    // Keep the caret in view: scroll only once it would fall off the end.
    let chars: Vec<char> = value.chars().collect();
    let first = (cursor + 1).saturating_sub(VALUE);
    let window: String = chars.iter().skip(first).take(VALUE).collect();
    let at = cursor - first;

    let mut spans = Vec::new();
    let shown: Vec<char> = window.chars().collect();
    spans.push(Span::styled(
        shown[..at.min(shown.len())].iter().collect::<String>(),
        Style::default().fg(theme::STRING),
    ));
    // Past the end of the value the caret sits on a blank.
    let under = shown.get(at).copied().unwrap_or(' ');
    spans.push(Span::styled(
        under.to_string(),
        Style::default().bg(theme::ACCENT).fg(theme::BG),
    ));
    let rest: String = shown.iter().skip(at + 1).collect();
    let used = at + 1 + rest.chars().count();
    spans.push(Span::styled(
        format!("{rest}{}", " ".repeat(VALUE.saturating_sub(used))),
        Style::default().fg(theme::STRING),
    ));
    spans
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
        return format!("{} matches · ^o pick", plan.count());
    }
    if SubmitForm::takes_path(field) {
        return format!("{} · ^o pick a file", field.hint());
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
/// `waits` names the jobs this one waits on, stood in for by names until a
/// chain is submitted.
fn summary(form: &SubmitForm, cluster: &Cluster, width: usize, waits: Waits) -> Vec<Line<'static>> {
    let (out, _) = slurm::log_paths(&form.namepath, &form.settings);
    let command = slurm::preview_command(
        &form.base,
        &form.logs,
        &form.just_args,
        &form.namepath,
        &form.settings,
        form.batch(),
        waits,
    );

    let mut lines = Vec::new();

    // Queued jobs picked by hand are named before the command, so what the
    // `--dependency` in it stands for is plain.
    if !form.after.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("   after     ", theme::label()),
            Span::styled(
                truncate(&form.after.join(" · "), width.saturating_sub(48)),
                Style::default()
                    .fg(theme::MATCH)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  starts once they have finished ok · F7", theme::label()),
        ]));
    }

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

/// Which queued jobs the one in the form waits on. Ticks take effect on
/// Enter, so Esc is a real way out.
pub fn draw_after_pick(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pick) = app.after_pick.as_ref() else {
        return;
    };
    let width = 76.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(2) as usize;
    let name_width = pick
        .rows
        .iter()
        .filter_map(|row| row.job.as_ref())
        .map(|job| job.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, inner.saturating_sub(44));

    let mut lines: Vec<Line> = Vec::new();
    for (index, row) in pick.rows.iter().enumerate() {
        let selected = index == pick.cursor;
        let chosen = pick.is_chosen(&row.id);
        let mut spans = vec![
            Span::styled(
                if selected { " ▸ " } else { "   " },
                Style::default().fg(theme::ACCENT),
            ),
            Span::styled(
                if chosen { "[x] " } else { "[ ] " },
                Style::default().fg(if chosen { theme::RECIPE } else { theme::DIM }),
            ),
            Span::styled(
                format!("{:<10}", truncate(&row.id, 10)),
                Style::default().fg(theme::MATCH),
            ),
        ];
        match row.job.as_ref() {
            Some(job) => {
                let state = job.state_word().to_owned();
                // A pending job's reason says why it waits; a running one's
                // clock says how long it has been at it.
                let detail = match job.pending() {
                    true => job.nodes.clone(),
                    false => job.elapsed_now(0),
                };
                spans.push(Span::styled(
                    format!("{:<name_width$}  ", truncate(&job.name, name_width)),
                    Style::default().fg(theme::FG),
                ));
                spans.push(Span::styled(
                    format!("{state:<10}"),
                    Style::default().fg(match state.as_str() {
                        "RUNNING" => theme::RECIPE,
                        "PENDING" => theme::MATCH,
                        _ => theme::VARIABLE,
                    }),
                ));
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                spans.push(Span::styled(
                    truncate(
                        &format!("{detail}  {}", job.partition),
                        inner.saturating_sub(used),
                    ),
                    theme::label(),
                ));
            }
            None => {
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                spans.push(Span::styled(
                    truncate(
                        "not in your queue any more — untick it unless you are sure",
                        inner.saturating_sub(used),
                    ),
                    Style::default().fg(theme::DIM),
                ));
            }
        }
        pad(&mut spans, inner);
        let line = Line::from(spans);
        lines.push(match selected {
            true => line.style(Style::default().bg(theme::SELECTION_BG)),
            false => line,
        });
    }
    if pick.rows.is_empty() {
        lines.push(Line::from(Span::styled(
            match pick.fetching() {
                true => "   asking squeue…".to_owned(),
                false => format!(
                    "   {}",
                    pick.note
                        .as_deref()
                        .unwrap_or("no jobs of yours are queued")
                ),
            },
            theme::label(),
        )));
    } else if let Some(note) = pick.note.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("   ! {note}"),
            Style::default().fg(theme::MATCH),
        )));
    }

    lines.push(Line::default());
    let count = pick.chosen.len();
    let summary = match count {
        0 => "   waits on nothing".to_owned(),
        1 => "   waits on 1 job".to_owned(),
        n => format!("   waits on {n} jobs"),
    };
    lines.push(Line::from(vec![
        Span::styled(summary.clone(), Style::default().fg(theme::FG)),
        Span::styled(
            truncate(
                match count {
                    0 => "",
                    _ => "   starts once each has finished ok; cancelled if one fails",
                },
                inner.saturating_sub(summary.chars().count()),
            ),
            theme::label(),
        ),
    ]));

    let title = match pick.fetching() {
        true => " Wait for which jobs?  (refreshing) ",
        false => " Wait for which jobs? ",
    };
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            title,
            " space toggle · a all · n none · r refresh · ⏎ done · esc back ",
            theme::MATCH,
        )),
        popup,
    );
}

// ---------------------------------------------------------------------------
// Chains
// ---------------------------------------------------------------------------

/// The chooser: one row per link, in the order they would be submitted.
pub fn draw_chain(frame: &mut Frame, app: &App, area: Rect) {
    let Some(chain) = app.chain.as_ref() else {
        return;
    };
    let width = area
        .width
        .saturating_sub(6)
        .clamp(WIDTH.min(area.width.saturating_sub(2)), MAX_WIDTH)
        .min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;
    let name_width = chain
        .rows
        .iter()
        .map(|row| row.link.label().chars().count())
        .max()
        .unwrap_or(0)
        .clamp(12, inner.saturating_sub(30));

    let run = chain.is_run();
    let mut lines: Vec<Line> = Vec::new();
    for (index, row) in chain.rows.iter().enumerate() {
        let selected = index == chain.cursor;
        let status = row.status();
        let mut spans = vec![
            Span::styled(
                if selected { " ▸ " } else { "   " },
                Style::default().fg(theme::ACCENT),
            ),
            Span::styled(
                if row.checked { "[x] " } else { "[ ] " },
                Style::default().fg(if row.checked {
                    theme::RECIPE
                } else {
                    theme::DIM
                }),
            ),
            Span::styled(format!("{:>2}  ", index + 1), theme::label()),
            Span::styled(
                format!("{:<name_width$}  ", truncate(&row.link.label(), name_width)),
                Style::default().fg(if row.checked { theme::FG } else { theme::DIM }),
            ),
        ];
        if !run {
            spans.push(Span::styled(
                format!("{status:<12}"),
                Style::default().fg(match status {
                    "no settings" => theme::INTERP,
                    "edited" => theme::MATCH,
                    _ => theme::VARIABLE,
                }),
            ));
        }
        let detail = match (run, row.needs_form()) {
            (true, _) => format!("$ just --no-deps {}", row.link.label()),
            (false, true) => "will ask".to_owned(),
            (false, false) => row.settings.asked_for(),
        };
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        spans.push(Span::styled(
            truncate(&detail, inner.saturating_sub(used)),
            theme::label(),
        ));
        pad(&mut spans, inner);
        let line = Line::from(spans);
        lines.push(match selected {
            true => line.style(Style::default().bg(theme::SELECTION_BG)),
            false => line,
        });

        let mut waits: Vec<String> = chain
            .waits_on(index)
            .iter()
            .map(|i| (i + 1).to_string())
            .collect();
        waits.extend(row.after.iter().map(|id| format!("job {id}")));
        if !waits.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("{:>10}↳ after {}", "", waits.join(", ")),
                theme::label(),
            )));
        }
    }

    lines.push(Line::default());
    let asks = match run {
        true => 0,
        false => chain.needing_form(),
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "   {} of {} {}",
                chain.checked(),
                chain.rows.len(),
                if run { "recipes" } else { "jobs" }
            ),
            Style::default().fg(theme::FG),
        ),
        Span::styled(
            match asks {
                0 => String::new(),
                1 => " · 1 needs settings".to_owned(),
                n => format!(" · {n} need settings"),
            },
            Style::default().fg(theme::INTERP),
        ),
        Span::styled(
            match run {
                true => "   unchecked = already done, not run",
                false => "   unchecked = already done, not waited for",
            },
            theme::label(),
        ),
    ]));

    let (title, footer) = match &chain.purpose {
        crate::submit::Purpose::Run { dry: true, .. } => (
            format!(" Dry-run {} and what it runs with ", chain.head),
            " space toggle · a all · n none · ⏎ run · esc ",
        ),
        crate::submit::Purpose::Run { .. } => (
            format!(" Run {} and what it runs with ", chain.head),
            " space toggle · a all · n none · ⏎ run · esc ",
        ),
        crate::submit::Purpose::Submit => (
            format!(" Submit {} and what it runs with ", chain.head),
            " space toggle · a all · n none · f open form · ⏎ continue · esc ",
        ),
    };
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(&title, footer, theme::ACCENT)),
        popup,
    );
}

/// Every `sbatch` line the chain would run, waiting for a `y`.
pub fn draw_chain_confirm(frame: &mut Frame, app: &App, area: Rect) {
    let Some(chain) = app.chain.as_ref() else {
        return;
    };
    let width = area.width.saturating_sub(4).min(MAX_WIDTH);
    let inner = width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (index, row) in chain.rows.iter().enumerate() {
        if !row.checked {
            continue;
        }
        let placeholders = chain.placeholder_ids(index);
        let plan = crate::batch::plan(&row.base, &row.logs, &row.link.namepath, &row.settings)
            .ok()
            .flatten();
        let batch = plan
            .as_ref()
            .map(|plan| (plan.manifest.as_path(), plan.count()));
        let command = slurm::preview_command(
            &row.base,
            &row.logs,
            &row.just_args,
            &row.link.namepath,
            &row.settings,
            batch,
            Waits::link(&placeholders),
        );
        lines.push(Line::from(vec![
            Span::styled(format!("  {:>2}  ", index + 1), theme::label()),
            Span::styled(
                row.link.label(),
                Style::default()
                    .fg(theme::RECIPE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                match chain.waits_label(index).is_empty() {
                    true => String::new(),
                    false => format!("   {}", chain.waits_label(index)),
                },
                theme::label(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("      $ ", theme::label()),
            Span::styled(
                truncate(&command, inner.saturating_sub(8)),
                Style::default().fg(theme::FG),
            ),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "   <name> stands for the job id that link gets when it is submitted",
        theme::label(),
    )));

    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(
            &format!(" Submit {} jobs in this order? ", chain.checked()),
            " y submit them · any other key goes back to the chooser ",
            theme::MATCH,
        )),
        popup,
    );
}
