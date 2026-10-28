//! Waiting on queued jobs picked by hand: the picker over the form, and the
//! `sbatch` flags it comes to.

use std::path::Path;

use super::{fixture_app, test_cluster};
use crate::app::{AfterPick, App, Mode};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::slurm::{self, Settings, Waits};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
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

/// A queue with a running job, a pending one, and the remainder of an array.
fn queue() -> Vec<slurm::Job> {
    slurm::merge_jobs(
        Some(
            "4210|nightly|RUNNING|qib|s|s|00:12:33|node07|/work|64|128G\n\
             4215|sort|PENDING|qib|s|N/A|0:00|(Priority)|/work|4|8G\n\
             4220_[3-9]|sweep|PENDING|qib|s|N/A|0:00|(Resources)|/work|4|8G",
        ),
        None,
        1,
    )
    .jobs
}

#[test]
fn a_job_waiting_on_picked_jobs_keeps_its_own_dependencies() {
    let settings = Settings {
        partition: "qib-compute".into(),
        ..Default::default()
    };
    let after = ["4210".to_owned(), "4215".to_owned()];
    let args = slurm::build_command(
        Path::new("/work"),
        Path::new("/work"),
        &[],
        "align",
        &settings,
        None,
        Waits::after(&after),
    );

    assert!(
        args.contains(&"--dependency=afterok:4210:4215".to_owned()),
        "got {args:?}"
    );
    assert!(args.contains(&"--kill-on-invalid-dep=yes".to_owned()));
    // Unlike a link of a chain, the recipe's own dependencies are not jobs
    // of their own, so they still run inside this one.
    let wrap = args.iter().position(|a| a == "--wrap").unwrap();
    assert_eq!(args[wrap + 1], "just align");

    // Nothing picked is nothing passed.
    let alone = slurm::build_command(
        Path::new("/work"),
        Path::new("/work"),
        &[],
        "align",
        &settings,
        None,
        Waits::after(&[]),
    );
    let plain = slurm::build_command(
        Path::new("/work"),
        Path::new("/work"),
        &[],
        "align",
        &settings,
        None,
        Waits::ALONE,
    );
    assert_eq!(alone, plain);
}

#[test]
fn the_picker_lists_the_queue_and_ticks_go_back_to_the_form() {
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit_single();

    // F7 opens the picker over the form; the queue is asked for on a thread,
    // so here it is handed over directly.
    app.handle_key(key(KeyCode::F(7)));
    assert_eq!(app.mode, Mode::AfterPick);
    assert!(app.after_pick.is_some());
    app.after_pick = Some(AfterPick::with(queue(), &[]));

    let text = screen(&mut app, 110, 30);
    assert!(text.contains("Wait for which jobs?"));
    assert!(text.contains("nightly"), "the running job is listed");
    assert!(
        text.contains("4220 "),
        "an array's remainder is offered as the array as a whole: {text}"
    );
    assert!(
        text.contains("Submit build to Slurm"),
        "the form stays readable underneath"
    );

    // Tick the first and third rows: newest first, as the browser lists them.
    app.handle_key(key(KeyCode::Char(' ')));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Char(' ')));
    let pick = app.after_pick.as_ref().unwrap();
    assert_eq!(pick.chosen_ids(), ["4220", "4210"]);

    app.handle_key(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Submit);
    assert!(app.after_pick.is_none());
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.after, ["4220", "4210"]);
    assert!(
        !form.settings.extra.contains("dependency"),
        "the waits are not a setting"
    );

    // The form says what it waits on, and the command carries it.
    let text = screen(&mut app, 110, 30);
    assert!(text.contains("after     4220 · 4210"), "{text}");
    assert!(text.contains("--dependency=afterok:4220:4210"));
    assert!(text.contains("--kill-on-invalid-dep=yes"));
    assert!(!text.contains("--no-deps"));
}

#[test]
fn esc_leaves_the_form_as_it_was_and_the_picker_starts_from_it() {
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit_single();
    app.form.as_mut().unwrap().after = vec!["4215".to_owned()];

    app.handle_key(key(KeyCode::F(7)));
    app.after_pick = Some(AfterPick::with(queue(), &["4215".to_owned()]));
    let pick = app.after_pick.as_ref().unwrap();
    assert!(
        pick.is_chosen("4215"),
        "what the form waits on starts ticked"
    );

    // Untick everything, then think better of it.
    app.handle_key(key(KeyCode::Char('n')));
    assert!(app.after_pick.as_ref().unwrap().chosen_ids().is_empty());
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.mode, Mode::Submit);
    assert_eq!(app.form.as_ref().unwrap().after, ["4215"]);

    // `a` ticks the whole queue.
    app.handle_key(key(KeyCode::F(7)));
    app.after_pick = Some(AfterPick::with(queue(), &["4215".to_owned()]));
    app.handle_key(key(KeyCode::Char('a')));
    app.handle_key(key(KeyCode::Enter));
    assert_eq!(
        app.form.as_ref().unwrap().after,
        ["4220", "4215", "4210"],
        "in the order the queue lists them, newest first"
    );
}

#[test]
fn an_id_whose_job_has_left_the_queue_is_still_shown_so_it_can_be_unticked() {
    let mut app = fixture_app();
    app.cluster = Some(test_cluster());
    app.open_submit_single();
    app.form.as_mut().unwrap().after = vec!["3999".to_owned()];
    app.handle_key(key(KeyCode::F(7)));
    app.after_pick = Some(AfterPick::with(queue(), &["3999".to_owned()]));

    let text = screen(&mut app, 110, 30);
    assert!(text.contains("3999"));
    assert!(text.contains("not in your queue any more"));

    // It comes after the queue, so the last row is the one to untick; the
    // cursor starts at the top, not on it.
    let pick = app.after_pick.as_mut().unwrap();
    assert_eq!(pick.cursor, 0);
    pick.move_cursor(-1);
    pick.toggle();
    assert!(pick.chosen_ids().is_empty());
    app.handle_key(key(KeyCode::Enter));
    assert!(app.form.as_ref().unwrap().after.is_empty());
}

#[test]
fn a_link_of_a_chain_waits_on_picked_jobs_as_well_as_its_upstream() {
    use crate::chain::Link;
    use crate::config::Resolved;
    use crate::submit::{ChainForm, ChainRow, Purpose};

    let link = |namepath: &str, after: Vec<usize>| Link {
        id: 0,
        namepath: namepath.to_owned(),
        args: String::new(),
        argv: Vec::new(),
        after,
    };
    let row = |namepath: &str, after: Vec<usize>| {
        ChainRow::new(
            link(namepath, after),
            Resolved::default(),
            Path::new("/work").to_path_buf(),
            Path::new("/work").to_path_buf(),
        )
    };
    let mut chain = ChainForm::new(
        "align".to_owned(),
        Purpose::Submit,
        vec![row("fetch", vec![]), row("align", vec![0])],
        &[],
    );
    // The form for align was opened and two queued jobs ticked in it.
    chain.step = Some(1);
    chain.accept(
        Settings::default(),
        vec!["4210".to_owned(), "4215".to_owned()],
    );

    assert_eq!(chain.waits_label(1), "waits on fetch, job 4210, job 4215");
    assert_eq!(chain.placeholder_ids(1), ["<fetch>", "4210", "4215"]);
    assert_eq!(chain.rows[1].after, ["4210", "4215"]);
    assert_eq!(
        chain.waits_label(0),
        "",
        "the first link still waits on nothing"
    );
}
