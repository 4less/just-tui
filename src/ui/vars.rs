//! The variables list `V` opens, and the full-screen directory explorer that
//! picks a path for one of them.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::{centered, overlay_block, pad, shorten_home, truncate};
use crate::app::App;
use crate::theme;
use crate::vars::ExplorerFocus;

const WIDTH: u16 = 92;

pub fn draw_vars(frame: &mut Frame, app: &App, area: Rect) {
    let Some(form) = app.vars.as_ref() else {
        return;
    };
    let width = WIDTH.min(area.width.saturating_sub(2));
    let inner = width.saturating_sub(4) as usize;
    let name_width = form
        .rows
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(6, 20);

    let mut lines: Vec<Line> = Vec::new();
    for (index, row) in form.rows.iter().enumerate() {
        let selected = index == form.cursor;
        let mut spans = vec![
            Span::styled(
                if selected { " ▸ " } else { "   " },
                Style::default().fg(theme::ACCENT),
            ),
            Span::styled(
                format!("{:<name_width$}  ", row.name),
                Style::default().fg(theme::VARIABLE),
            ),
        ];
        match (
            selected.then_some(form.typing.as_ref()).flatten(),
            &row.overridden,
        ) {
            (Some(typed), _) => {
                spans.push(Span::styled(
                    typed.clone(),
                    Style::default().fg(theme::STRING),
                ));
                spans.push(Span::styled("▏", Style::default().fg(theme::ACCENT)));
            }
            (None, Some(value)) => {
                spans.push(Span::styled(
                    shorten_home(value),
                    Style::default().fg(theme::STRING),
                ));
                spans.push(Span::styled(
                    format!("   overrides {}", row.declared),
                    theme::label(),
                ));
            }
            (None, None) => spans.push(Span::styled(row.declared.clone(), theme::value())),
        }
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        if used > inner {
            let last = spans.len() - 1;
            let keep = inner.saturating_sub(used - spans[last].content.chars().count());
            spans[last] = Span::styled(truncate(&spans[last].content, keep), spans[last].style);
        }
        pad(&mut spans, inner);
        let line = Line::from(spans);
        lines.push(match selected {
            true => line.style(Style::default().bg(theme::SELECTION_BG)),
            false => line,
        });
    }

    lines.push(Line::default());
    let command = form
        .selected()
        .and_then(|row| row.overridden.as_ref().map(|value| (row, value)))
        .map(|(row, value)| {
            format!(
                "$ just --set {} {} …",
                row.key,
                crate::params::quote(&shorten_home(value))
            )
        })
        .unwrap_or_else(|| "no override: the justfile's value is used".to_owned());
    lines.push(Line::from(vec![
        Span::raw("   "),
        Span::styled(truncate(&command, inner.saturating_sub(3)), theme::label()),
    ]));

    let title = match form.module.is_empty() {
        true => " Variables ".to_owned(),
        false => format!(" Variables of {} ", form.module),
    };
    let footer = match form.typing {
        Some(_) => " ⏎ save · esc cancel · ^u clear ",
        None => " ⏎ pick a path · e type · d reset · esc close ",
    };
    let popup = centered(area, width, lines.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(overlay_block(&title, footer, theme::VARIABLE)),
        popup,
    );
}

pub fn draw_explorer(frame: &mut Frame, app: &App, area: Rect) {
    let Some(explorer) = app.explorer.as_ref() else {
        return;
    };
    frame.render_widget(Clear, area);

    let [main, footer] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let (list_area, pins_area) = match explorer.pins.is_empty() {
        true => (main, None),
        false => {
            let pins_width = explorer
                .pins
                .iter()
                .map(|(tag, _)| tag.chars().count() as u16)
                .max()
                .unwrap_or(0)
                .clamp(12, 28)
                + 6;
            let [pins, list] =
                Layout::horizontal([Constraint::Length(pins_width), Constraint::Min(10)])
                    .areas(main);
            (list, Some(pins))
        }
    };

    // The listing: one row per entry, directories first.
    let listing_focused = explorer.focus == ExplorerFocus::Listing;
    let height = list_area.height.saturating_sub(2) as usize;
    let width = list_area.width.saturating_sub(2) as usize;
    let first = explorer.cursor.saturating_sub(height.saturating_sub(1));
    let mut lines: Vec<Line> = explorer
        .entries
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, entry)| {
            let selected = index == explorer.cursor && listing_focused;
            let (mark, color) = match entry.is_dir {
                true => ("▸ ", theme::MODULE),
                false => ("  ", theme::FG),
            };
            let name = match entry.is_dir {
                true => format!("{}/", entry.name),
                false => entry.name.clone(),
            };
            let mut spans = vec![
                Span::styled(format!(" {mark}"), Style::default().fg(color)),
                Span::styled(
                    truncate(&name, width.saturating_sub(3)),
                    Style::default().fg(color),
                ),
            ];
            pad(&mut spans, width);
            let line = Line::from(spans);
            match selected {
                true => line.style(Style::default().bg(theme::SELECTION_BG)),
                false => line,
            }
        })
        .collect();
    if explorer.entries.is_empty() {
        lines.push(Line::from(Span::styled("  (empty)", theme::label())));
    }
    let title = format!(
        " {} = {} ",
        explorer.key,
        shorten_home(&explorer.dir.display().to_string())
    );
    frame.render_widget(
        Paragraph::new(lines).block(panel(&title, listing_focused)),
        list_area,
    );

    // The bookmarks, when qwe has any.
    if let Some(pins_area) = pins_area {
        let focused = !listing_focused;
        let width = pins_area.width.saturating_sub(2) as usize;
        let lines: Vec<Line> = explorer
            .pins
            .iter()
            .enumerate()
            .take(pins_area.height.saturating_sub(2) as usize)
            .map(|(index, (tag, dir))| {
                let selected = index == explorer.pin_cursor && focused;
                let here = *dir == explorer.dir;
                let mut spans = vec![Span::styled(
                    truncate(&format!(" {}{tag}", if here { "● " } else { "  " }), width),
                    Style::default().fg(theme::ACCENT),
                )];
                pad(&mut spans, width);
                let line = Line::from(spans);
                match selected {
                    true => line.style(Style::default().bg(theme::SELECTION_BG)),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(panel(" qwe ", focused)),
            pins_area,
        );
    }

    // What choosing would set comes first; the keys get what is left.
    let choice = shorten_home(&explorer.choice().display().to_string());
    let keys = match explorer.pins.is_empty() {
        true => "⏎ open · ← up · space choose · c this dir · . hidden · ~ home · esc back",
        false => "⏎ open · ← up · space choose · c this dir · ⇥ qwe · . hidden · ~ home · esc back",
    };
    let total = footer.width as usize;
    let lead = " space sets ";
    let choice = truncate(&choice, (total * 2 / 3).saturating_sub(lead.len()));
    let used = lead.len() + choice.chars().count() + 3;
    let line = Line::from(vec![
        Span::styled(lead, theme::label()),
        Span::styled(
            choice,
            Style::default()
                .fg(theme::STRING)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled(truncate(keys, total.saturating_sub(used)), theme::label()),
    ]);
    frame.render_widget(Paragraph::new(line), footer);
}

fn panel(title: &str, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border(focused))
        .title(Span::styled(title.to_owned(), theme::title(focused)))
}
