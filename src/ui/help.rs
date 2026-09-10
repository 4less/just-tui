//! The key reference overlay.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Padding, Paragraph};

use super::{centered, overlay_block};
use crate::theme;

const KEYS: &[(&str, &str)] = &[
    ("j / k / ↑ ↓", "move in the focused pane"),
    ("J / K", "next / previous recipe"),
    ("h / l / ← →", "collapse / expand a module"),
    ("Enter, Space", "run recipe, toggle module, follow alias"),
    ("r", "run the selected recipe"),
    ("s", "submit to Slurm (sbatch)"),
    ("S", "browse Slurm jobs and their logs"),
    ("H", "past submissions, to reuse settings"),
    ("a", "run with extra arguments"),
    ("n", "dry run (just --dry-run)"),
    ("/", "fuzzy search recipes and docs"),
    ("Tab / Shift-Tab", "cycle pane focus"),
    ("g / G", "top / bottom"),
    ("Ctrl-d / Ctrl-u", "half page down / up"),
    ("PgDn / PgUp", "page down / up"),
    ("e / c", "expand all / collapse all modules"),
    ("p", "show or hide private recipes"),
    ("m", "fold the [group(...)] layer in or out"),
    ("v", "toggle doc/code split direction"),
    ("w", "wrap long source lines"),
    ("f", "zoom the focused pane"),
    ("y", "copy recipe source (OSC 52)"),
    ("o", "open the justfile in $EDITOR"),
    ("R", "reload the justfile"),
    ("?", "this help"),
    ("q / Esc", "clear search, then quit"),
    ("Ctrl-c", "quit immediately"),
];

pub fn draw(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = KEYS
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:>16}  "),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*label, theme::value()),
            ])
        })
        .collect();

    let popup = centered(area, 60, KEYS.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            overlay_block(" Keys ", " any key closes ", theme::ACCENT)
                .padding(Padding::horizontal(1)),
        ),
        popup,
    );
}
