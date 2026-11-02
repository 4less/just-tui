//! The source pane: the recipe exactly as written, with line numbers.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::pane_block;
use crate::app::{App, Mode, Pane};
use crate::editor::{Editor, expand_tabs};
use crate::highlight;
use crate::theme;
use crate::tree::Kind;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.mode == Mode::Edit
        && let Some(editor) = app.editor.as_mut()
    {
        draw_editor(frame, editor, area);
        return;
    }
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

/// The recipe being edited: highlighted as it is typed, with a caret.
fn draw_editor(frame: &mut Frame, editor: &mut Editor, area: Rect) {
    let name = editor
        .path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let block = pane_block(format!(" editing {name}:{} ", editor.start_line), true)
        .border_style(Style::default().fg(theme::MATCH))
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title_bottom(match editor.dirty() {
            true => Span::styled(" modified ", Style::default().fg(theme::MATCH)),
            false => Span::raw(""),
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let gutter = (editor.start_line + editor.lines.len())
        .to_string()
        .len()
        .max(2);
    let [numbers, text] =
        Layout::horizontal([Constraint::Length(gutter as u16 + 3), Constraint::Min(1)])
            .areas(inner);
    editor.view = text;
    editor.height = text.height as usize;
    editor.width = text.width as usize;
    editor.follow();

    // Highlighted whole, since a line's colours depend on the ones above it.
    let mut highlighted = highlight::recipe(&editor.text());
    highlighted.resize(editor.lines.len(), Line::default());
    let shown = editor.top..(editor.top + editor.height).min(editor.lines.len());

    let gutter_lines: Vec<Line> = shown
        .clone()
        .map(|row| {
            let color = match row == editor.row {
                true => theme::ACCENT,
                false => theme::BORDER,
            };
            Line::from(Span::styled(
                format!("{:>gutter$} │ ", editor.start_line + row),
                Style::default().fg(color),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(gutter_lines), numbers);

    let text_lines: Vec<Line> = highlighted
        .drain(shown)
        .map(|line| {
            let spans = line
                .spans
                .into_iter()
                .scan(String::new(), |before, span| {
                    // Tabs expand by where they fall on the whole line.
                    let start = expand_tabs(before).chars().count();
                    before.push_str(&span.content);
                    let wide = expand_tabs(before);
                    let content: String = wide.chars().skip(start).collect();
                    Some(Span::styled(content, span.style))
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(text_lines).scroll((0, editor.left as u16)),
        text,
    );

    let x = editor.caret_column() - editor.left;
    let y = editor.row - editor.top;
    frame.set_cursor_position((text.x + x as u16, text.y + y as u16));
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
