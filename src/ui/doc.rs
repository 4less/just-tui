//! The documentation pane: what a recipe is, takes, needs and runs as.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Padding, Paragraph, Wrap};

use super::{badge, field, pair, pane_block, section, shorten_home};
use crate::app::{App, Pane};
use crate::highlight;
use crate::model::{Recipe, render_expression};
use crate::theme;
use crate::tree::{Kind, Node};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Pane::Doc;
    let lines = contents(app);
    app.doc_lines = lines.len();
    app.doc_height = area.height.saturating_sub(2);

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.doc_scroll, 0))
        .block(pane_block(" Documentation ", focused).padding(Padding::horizontal(1)));
    frame.render_widget(paragraph, area);
}

fn contents(app: &App) -> Vec<Line<'static>> {
    match app.selected() {
        None => vec![Line::from(Span::styled("no recipes match", theme::label()))],
        Some(node) => match node.kind {
            Kind::Module => module_lines(node),
            Kind::Alias => alias_lines(node),
            Kind::Recipe => recipe_lines(node),
        },
    }
}

fn heading(name: &str, kind: &str, color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            name.to_owned(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {kind}"), theme::label()),
    ])
}

fn module_lines(node: &Node) -> Vec<Line<'static>> {
    let kind = if node.is_root && node.global {
        "global recipes"
    } else {
        "module"
    };
    let mut lines = vec![heading(&node.name, kind, theme::MODULE), Line::default()];

    if !node.namepath.is_empty() {
        lines.push(field("path", &node.namepath));
    }
    if let Some(src) = &node.source {
        lines.push(field("source", &shorten_home(&src.display().to_string())));
    }
    lines.push(field("entries", &node.children.len().to_string()));

    if let Some(doc) = &node.summary {
        lines.push(Line::default());
        lines.extend(highlight::doc(doc));
    }
    for (title, entries) in [("Settings", &node.settings), ("Variables", &node.variables)] {
        if entries.is_empty() {
            continue;
        }
        lines.push(Line::default());
        lines.push(section(title));
        lines.extend(entries.iter().map(|(key, value)| pair(key, value)));
    }
    lines
}

fn alias_lines(node: &Node) -> Vec<Line<'static>> {
    vec![
        heading(&node.name, "alias", theme::ALIAS),
        Line::default(),
        field("target", node.alias_target.as_deref().unwrap_or("?")),
        Line::default(),
        Line::from(Span::styled(
            "press Enter to jump to the recipe",
            theme::label(),
        )),
    ]
}

fn recipe_lines(node: &Node) -> Vec<Line<'static>> {
    let Some(info) = node.info.as_ref() else {
        return Vec::new();
    };
    let recipe = &info.recipe;
    let mut lines = vec![signature(recipe)];

    let badges = badges(node, recipe, info.exact);
    if !badges.is_empty() {
        lines.push(Line::from(badges));
    }
    lines.push(Line::default());

    if let Some(doc) = &info.doc {
        lines.extend(highlight::doc(doc));
        lines.push(Line::default());
    }
    lines.extend(parameters(recipe));
    lines.extend(dependencies(recipe));
    lines.extend(attributes(recipe));

    lines.push(section("Invoke"));
    lines.push(Line::from(vec![
        Span::styled("  $ ", theme::label()),
        Span::styled(
            format!("just {}", recipe.namepath),
            Style::default().fg(theme::STRING),
        ),
    ]));

    if let Some(src) = &info.source {
        lines.push(Line::default());
        lines.push(field(
            "source",
            &format!(
                "{}:{}",
                shorten_home(&src.display().to_string()),
                info.start_line
            ),
        ));
    }
    lines
}

/// `name arg="default" *rest`, coloured by role.
fn signature(recipe: &Recipe) -> Line<'static> {
    let mut spans = vec![Span::styled(
        recipe.name.clone(),
        Style::default()
            .fg(theme::RECIPE)
            .add_modifier(Modifier::BOLD),
    )];
    for parameter in &recipe.parameters {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{}{}", parameter.sigil(), parameter.name),
            Style::default().fg(theme::VARIABLE),
        ));
        if let Some(default) = parameter.default_text() {
            spans.push(Span::styled("=", theme::label()));
            spans.push(Span::styled(default, Style::default().fg(theme::STRING)));
        }
    }
    Line::from(spans)
}

fn badges(node: &Node, recipe: &Recipe, exact: bool) -> Vec<Span<'static>> {
    let mut badges = Vec::new();
    if node.private {
        badges.push(badge("private", theme::DIM));
    }
    if recipe.shebang {
        badges.push(badge("shebang", theme::KEYWORD));
    }
    if recipe.quiet {
        badges.push(badge("quiet", theme::DIM));
    }
    if let Some(group) = &node.group {
        badges.push(badge(group, theme::MODULE));
    }
    if !exact {
        badges.push(badge("reconstructed", theme::INTERP));
    }
    badges
}

fn parameters(recipe: &Recipe) -> Vec<Line<'static>> {
    if recipe.parameters.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![section("Parameters")];
    for parameter in &recipe.parameters {
        let mut spans = vec![
            Span::raw("  "),
            Span::styled(
                format!("{}{}", parameter.sigil(), parameter.name),
                Style::default().fg(theme::VARIABLE),
            ),
        ];
        match parameter.default_text() {
            Some(default) => {
                spans.push(Span::styled("  default ", theme::label()));
                spans.push(Span::styled(default, Style::default().fg(theme::STRING)));
            }
            None => spans.push(Span::styled("  required", theme::label())),
        }
        if let Some(note) = match parameter.kind.as_str() {
            "star" => Some("  variadic (0+)"),
            "plus" => Some("  variadic (1+)"),
            _ => None,
        } {
            spans.push(Span::styled(note, theme::label()));
        }
        if parameter.export {
            spans.push(Span::styled("  exported", theme::label()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::default());
    lines
}

fn dependencies(recipe: &Recipe) -> Vec<Line<'static>> {
    if recipe.dependencies.is_empty() {
        return Vec::new();
    }
    let (before, after) = recipe.split_dependencies();
    let mut lines = vec![section("Dependencies")];
    for (deps, arrow, when) in [(before, "  ↑ ", "before"), (after, "  ↓ ", "after")] {
        for dependency in deps {
            let args: Vec<String> = dependency.arguments.iter().map(render_expression).collect();
            let call = match args.is_empty() {
                true => dependency.recipe.clone(),
                false => format!("{}({})", dependency.recipe, args.join(", ")),
            };
            lines.push(Line::from(vec![
                Span::styled(arrow, theme::label()),
                Span::styled(call, Style::default().fg(theme::ACCENT)),
                Span::styled(format!("  runs {when}"), theme::label()),
            ]));
        }
    }
    lines.push(Line::default());
    lines
}

fn attributes(recipe: &Recipe) -> Vec<Line<'static>> {
    let labels = recipe.attribute_labels();
    if labels.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![section("Attributes")];
    for label in labels {
        lines.push(Line::from(vec![
            Span::styled("  [", theme::label()),
            Span::styled(label, Style::default().fg(theme::ATTRIBUTE)),
            Span::styled("]", theme::label()),
        ]));
    }
    lines.push(Line::default());
    lines
}
