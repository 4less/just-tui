//! The source pane: the recipe exactly as written, with line numbers.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::pane_block;
use crate::app::{App, Pane};
use crate::highlight;
use crate::theme;
use crate::tree::Kind;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Code;
    let (title, lines) = contents(app);
    app.code_lines = lines.len();
    app.code_height = area.height.saturating_sub(2);

    let block = pane_block(title, focused)
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title_bottom(match app.wrap_code {
            true => Span::styled(" wrap ", theme::label()),
            false => Span::raw(""),
        });

    let mut paragraph = Paragraph::new(lines)
        .scroll((app.code_scroll, 0))
        .block(block);
    if app.wrap_code {
        paragraph = paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(paragraph, area);

    draw_scrollbar(frame, app, area);
}

fn draw_scrollbar(frame: &mut Frame, app: &App, area: Rect) {
    let hidden = app.code_lines.saturating_sub(app.code_height as usize);
    if hidden == 0 {
        return;
    }
    let mut state = ScrollbarState::new(hidden).position(app.code_scroll as usize);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(theme::BORDER_FOCUS))
            .track_style(Style::default().fg(theme::BORDER)),
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

/// The pane title and its numbered, highlighted lines.
fn contents(app: &App) -> (String, Vec<Line<'static>>) {
    let Some(node) = app.selected() else {
        return (" Source ".to_owned(), Vec::new());
    };

    let Some(info) = node.info.as_ref() else {
        let text = match node.kind {
            Kind::Module => "select a recipe inside this module",
            Kind::Group => "select a recipe inside this group",
            _ => "aliases have no body — press Enter to follow",
        };
        return (
            " Source ".to_owned(),
            vec![Line::from(Span::styled(text, theme::label()))],
        );
    };

    let title = match info.source.as_ref().and_then(|p| p.file_name()) {
        Some(name) => format!(" {}:{} ", name.to_string_lossy(), info.start_line),
        None => " Source ".to_owned(),
    };

    let highlighted = highlight::recipe(&info.code);
    let gutter = (info.start_line + highlighted.len())
        .to_string()
        .len()
        .max(2);
    let lines = highlighted
        .into_iter()
        .enumerate()
        .map(|(offset, line)| number_line(info.start_line + offset, gutter, line))
        .collect();

    (title, lines)
}

fn number_line(number: usize, gutter: usize, line: Line<'static>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{number:>gutter$} │ "),
        Style::default().fg(theme::BORDER),
    )];
    spans.extend(line.spans);
    Line::from(spans)
}
