//! The explorer pane: modules as folders, recipes as files.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use super::{pane_block, truncate};
use crate::app::{App, Pane};
use crate::theme;
use crate::tree::{Filter, Kind, Node};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Tree;
    let filter = app.filter();
    let filtering = !app.search.is_empty();
    let width = area.width.saturating_sub(2) as usize;

    let items: Vec<ListItem> = app
        .visible
        .iter()
        .map(|&id| row(&app.tree.nodes[id], &filter, width, filtering))
        .collect();

    let title = match filtering {
        true => format!(" Explorer ({} matches) ", app.visible.len()),
        false => format!(" Explorer ({}) ", app.visible.len()),
    };

    let list = List::new(items)
        .block(pane_block(title, focused))
        .highlight_style(
            Style::default()
                .bg(theme::SELECTION_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

/// One row: indent guides, an icon, the name with search matches picked out,
/// and as much of the doc summary as fits.
fn row<'a>(node: &Node, filter: &Filter, width: usize, filtering: bool) -> ListItem<'a> {
    let mut spans = Vec::new();

    if node.depth > 0 {
        spans.push(Span::styled(
            "│ ".repeat(node.depth),
            Style::default().fg(theme::BORDER),
        ));
    }
    if node.is_root && node.global {
        spans.push(Span::styled("⌂ ", Style::default().fg(theme::ALIAS)));
    }

    let (icon, color) = icon_for(node, filtering);
    spans.push(Span::styled(icon, Style::default().fg(color)));
    spans.extend(name_spans(node, filter, color));

    if node.kind == Kind::Module {
        spans.push(Span::styled("/", Style::default().fg(theme::DIM)));
    }
    if node.is_default {
        spans.push(Span::styled(" ★", Style::default().fg(theme::MATCH)));
    }

    if let Some(summary) = summary_span(node, &spans, width) {
        spans.push(summary);
    }
    ListItem::new(Line::from(spans))
}

fn icon_for(node: &Node, filtering: bool) -> (&'static str, Color) {
    match node.kind {
        // A filter forces every branch open, so the arrow follows suit.
        Kind::Module => {
            let arrow = if node.expanded || filtering {
                "▾ "
            } else {
                "▸ "
            };
            let color = if node.global {
                theme::ALIAS
            } else {
                theme::MODULE
            };
            (arrow, color)
        }
        // A group folds like a module, but is coloured apart from one: it is
        // a label just put on the recipes, not a place they live.
        Kind::Group => {
            let arrow = if node.expanded || filtering {
                "▾ "
            } else {
                "▸ "
            };
            (arrow, theme::VARIABLE)
        }
        Kind::Recipe => ("• ", theme::RECIPE),
        Kind::Alias => ("↪ ", theme::ALIAS),
    }
}

/// The name, with the characters the fuzzy filter matched underlined.
fn name_spans(node: &Node, filter: &Filter, color: Color) -> Vec<Span<'static>> {
    let style = if node.private {
        Style::default()
            .fg(theme::DIM)
            .add_modifier(Modifier::ITALIC)
    } else if node.is_container() {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color)
    };

    // Matches are found against the full path, so shift them onto the name.
    let offset = node.search_path.len().saturating_sub(node.name.len());
    let hits: Vec<usize> = filter
        .match_indices(&node.search_path)
        .into_iter()
        .filter_map(|i| i.checked_sub(offset))
        .collect();

    if hits.is_empty() {
        return vec![Span::styled(node.name.clone(), style)];
    }
    node.name
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            let style = if hits.contains(&i) {
                Style::default()
                    .fg(theme::MATCH)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                style
            };
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

/// The first line of the doc comment, if there is room left for it.
fn summary_span(node: &Node, spans: &[Span], width: usize) -> Option<Span<'static>> {
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    if width <= used + 8 {
        return None;
    }
    let summary = node.summary.as_ref()?;
    let text = summary.lines().next().unwrap_or_default().trim();
    Some(Span::styled(
        format!("  {}", truncate(text, width - used - 3)),
        Style::default().fg(theme::DIM),
    ))
}
