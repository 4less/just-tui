//! Application state and a full-frame render.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::{fixture_app, test_cluster};
use crate::app::{JobsView, Mode};
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
    assert_eq!(form.current(), Field::Name);
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

/// The whole frame as one string, for the overlay checks below.
fn rendered(app: &mut crate::app::App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_job_browser_shows_the_queue_and_a_log() {
    let listing = "\
4190|tree-force-1|FAILED|qib-compute|s|s|e|00:00:35|1:0|node03|/work/proj|8|64G
4180|build|COMPLETED|qib-compute|s|s|e|00:03:50|0:0|node02|/work/proj|4|16G";

    let mut app = fixture_app();
    let mut view = JobsView::with(crate::slurm::merge_jobs(None, Some(listing), 7));
    view.lines = vec![
        "loading input".to_owned(),
        "slurmstepd: error: exceeded memory limit".to_owned(),
    ];
    app.jobs = Some(view);
    app.mode = Mode::Jobs;

    let text = rendered(&mut app, 110, 30);
    assert!(text.contains("Slurm jobs"));
    assert!(text.contains("4190") && text.contains("FAILED"));
    assert!(text.contains("tree-force-1"), "the job name is listed");
    assert!(text.contains("exit 1"), "and how it ended");
    assert!(text.contains("4180"), "finished jobs are listed too");
    assert!(
        text.contains("exceeded memory limit"),
        "the log pane shows the tail"
    );

    // `f` steps all → running → failed, which is the point of the view.
    app.cycle_job_filter();
    app.cycle_job_filter();
    let text = rendered(&mut app, 110, 30);
    assert!(text.contains("4190"));
    assert!(!text.contains("4180"), "the completed job is filtered out");
}

#[test]
fn a_job_name_is_built_from_the_recipe_and_its_arguments() {
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit();

    let form = app.form.as_mut().expect("form is open");
    form.settings.args = "name=ada".to_owned();
    assert_eq!(
        crate::slurm::job_name(&form.namepath, &form.settings),
        "build-name-ada"
    );

    // A typed name wins, and is what the log file is called too.
    form.settings.name = "ada trial".to_owned();
    assert_eq!(
        crate::slurm::job_name(&form.namepath, &form.settings),
        "ada-trial"
    );
    let (out, _) = crate::slurm::log_paths(&form.namepath, &form.settings);
    assert_eq!(out.to_str().unwrap(), "logs/ada-trial-%j.out");
}

#[test]
fn overlays_survive_a_tiny_terminal() {
    use crate::app::HistoryPick;
    use crate::history::Record;

    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.history.push_for_test(Record {
        job_id: "4190".to_owned(),
        namepath: "build".to_owned(),
        when: "2026-09-10T13:03:21".to_owned(),
        settings: crate::slurm::Settings {
            args: "arm=cong_filt_peel force=1".to_owned(),
            mem: "128G".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    });
    app.open_submit();
    app.picker = Some(HistoryPick {
        namepath: Some("build".to_owned()),
        index: 0,
        from: Mode::Submit,
    });
    app.mode = Mode::History;

    // Narrow enough that every flexible column is squeezed to nothing.
    for width in [20, 30, 40, 60, 120] {
        let text = rendered(&mut app, width, 14);
        assert!(!text.is_empty(), "renders at {width} columns");
    }

    // With room, the arguments are shown in full: they are what tells two
    // runs of one recipe apart.
    let text = rendered(&mut app, 150, 14);
    assert!(
        text.contains("arm=cong_filt_peel force=1"),
        "arguments are not truncated when there is room"
    );
    assert!(
        !text.contains("build arm="),
        "the title already names the recipe"
    );
}

#[test]
fn killing_a_job_has_to_be_confirmed() {
    use crate::history::Record;

    let listing = "\
4210|nightly|RUNNING|qib|s|s|00:12:33|node07|/work|64|128G
4180|build|COMPLETED|qib|s|s|00:03:50|node02|/work|4|16G";

    let mut app = fixture_app();
    app.jobs = Some(JobsView::with(crate::slurm::merge_jobs(
        Some(listing),
        None,
        7,
    )));
    app.mode = Mode::Jobs;

    // Nothing to kill on a job that has already stopped.
    app.move_job(1);
    app.ask_job_action(true, false);
    assert_eq!(app.mode, Mode::Jobs, "no confirmation for a finished job");
    assert!(app.pending.is_none());

    // A running one asks first, and leaves rather than acts on any other key.
    app.move_job(-1);
    app.ask_job_action(true, false);
    assert_eq!(app.mode, Mode::ConfirmJob);
    assert_eq!(app.pending.as_ref().unwrap().id, "4210");
    let text = rendered(&mut app, 100, 20);
    assert!(text.contains("Kill this job?") && text.contains("scancel 4210"));

    app.abandon_job_action();
    assert_eq!(app.mode, Mode::Jobs);
    assert!(app.pending.is_none(), "the job is left alone");

    // Resubmitting needs the settings it went out with.
    app.ask_job_action(true, true);
    assert_eq!(app.mode, Mode::Jobs, "nothing recorded for this job");

    app.history.push_for_test(Record {
        job_id: "4210".to_owned(),
        namepath: "build".to_owned(),
        command: "sbatch --mem=128G --wrap \"just build\"".to_owned(),
        ..Default::default()
    });
    app.ask_job_action(true, true);
    assert_eq!(app.mode, Mode::ConfirmJob);
    assert!(app.pending.as_ref().unwrap().resubmit);
    let text = rendered(&mut app, 100, 20);
    assert!(text.contains("Kill this job and run it again?"));
    assert!(
        text.contains("just build"),
        "it shows what would be resubmitted"
    );
}

#[test]
fn a_job_is_rerun_only_once_it_has_stopped() {
    use crate::history::Record;

    let mut app = fixture_app();
    app.jobs = Some(JobsView::with(crate::slurm::merge_jobs(
        Some("4210|nightly|RUNNING|qib|s|s|00:12:33|node07|/work|64|128G"),
        Some("4190|nightly|FAILED|qib|s|s|e|00:00:35|1:0|node03|/work|64|128G"),
        7,
    )));
    app.mode = Mode::Jobs;

    for id in ["4210", "4190"] {
        app.history.push_for_test(Record {
            job_id: id.to_owned(),
            namepath: "build".to_owned(),
            command: "sbatch --wrap \"just build\"".to_owned(),
            ..Default::default()
        });
    }

    // A job still on the queue is replaced, not duplicated.
    app.ask_job_action(false, true);
    assert_eq!(app.mode, Mode::Jobs, "no confirmation while it runs");
    assert!(app.pending.is_none());

    // One that has stopped — failed here — is exactly what rerunning is for.
    app.move_job(1);
    app.ask_job_action(false, true);
    assert_eq!(app.mode, Mode::ConfirmJob);
    let pending = app.pending.as_ref().expect("asked to confirm");
    assert!(pending.resubmit && !pending.kill);

    let text = rendered(&mut app, 100, 20);
    assert!(text.contains("Submit this job again?"));
    assert!(!text.contains("scancel"), "nothing is being killed");
}
