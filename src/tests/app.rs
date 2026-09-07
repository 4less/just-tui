//! Application state and a full-frame render.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::{fixture_app, test_cluster};
use crate::app::Mode;
use crate::slurm::Field;
use crate::tree::Kind;
use crate::ui;

#[test]
fn navigation_starts_on_the_first_recipe() {
    let app = fixture_app();
    let selected = app.selected().expect("something selected");
    assert_eq!(selected.kind, Kind::Recipe);
}

#[test]
fn renders_a_full_frame() {
    let mut app = fixture_app();
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &mut app)).unwrap();

    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..24)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Explorer"));
    assert!(text.contains("Documentation"));
    assert!(text.contains("build"), "the tree lists recipes");
    assert!(text.contains("Build it"), "the doc pane shows the doc");
    assert!(text.contains("cargo build"), "the code pane shows the body");
    assert!(!text.contains("_hidden"), "private stays hidden");
}

#[test]
fn the_submit_form_opens_with_resolved_settings() {
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit();

    assert_eq!(app.mode, Mode::Submit);
    let form = app.form.as_ref().expect("form is open");
    assert_eq!(form.namepath, "build");
    assert_eq!(form.current(), Field::Partition);
}

#[test]
fn clicks_are_routed_to_the_pane_under_them() {
    use crate::app::Pane;
    use crate::ui::Panes;
    use ratatui::layout::Rect;

    let panes = Panes {
        tree: Rect::new(0, 1, 30, 20),
        doc: Rect::new(30, 1, 70, 8),
        code: Rect::new(30, 9, 70, 12),
    };

    assert_eq!(panes.at(5, 5), Some(Pane::Tree));
    assert_eq!(panes.at(50, 5), Some(Pane::Doc));
    assert_eq!(panes.at(50, 15), Some(Pane::Code));
    assert_eq!(panes.at(5, 0), None, "the header belongs to no pane");

    // Row 0 of the list sits one line below the pane's top border.
    assert_eq!(panes.tree_row(5, 2), Some(0));
    assert_eq!(panes.tree_row(5, 7), Some(5));
    assert_eq!(panes.tree_row(50, 7), None, "outside the tree");
}
