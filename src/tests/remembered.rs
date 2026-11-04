//! Settings remembered without a submission: what a form was left holding,
//! the recent allocations offered to every recipe, and the partitions'
//! own defaults.

use std::path::{Path, PathBuf};

use super::{fixture_app, test_cluster};
use crate::app::{App, Choice, HistoryPick, Mode};
use crate::config::{Configs, LAST_RUN, LAST_SET, STATE_NAME};
use crate::input::{KeyCode, KeyEvent, KeyModifiers};
use crate::recent::{RECENT_NAME, Recent};
use crate::slurm::{Cluster, Field, Partition, Settings, parse_partition};

fn temp(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("just-tui-remembered-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mem(value: &str) -> Settings {
    Settings {
        mem: value.to_owned(),
        ..Default::default()
    }
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

/// The fixture app, with its state kept in a directory that exists.
fn app_in(dir: &Path) -> App {
    let mut app = fixture_app();
    app.configs = Configs::new(dir);
    app.cluster = Some(test_cluster());
    app
}

#[test]
fn settings_set_but_not_sent_are_labelled_as_such() {
    let dir = temp("label");
    let mut configs = Configs::new(&dir);
    configs.remember_set("post", &mem("4G")).unwrap();
    let text = std::fs::read_to_string(dir.join(STATE_NAME)).unwrap();
    assert!(text.contains("unsent     = yes"), "{text}");

    // Read back from disk, as the next session would.
    let mut configs = Configs::new(&dir);
    let resolved = configs.resolve(&[], "post");
    assert_eq!(resolved.settings.mem, "4G");
    assert_eq!(resolved.origin(Field::Mem), Some(LAST_SET));

    configs.remember("post", &mem("6G")).unwrap();
    let resolved = Configs::new(&dir).resolve(&[], "post");
    assert_eq!(resolved.origin(Field::Mem), Some(LAST_RUN));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn esc_on_a_changed_form_remembers_it() {
    let dir = temp("esc");
    let mut app = app_in(&dir);
    assert!(app.select_namepath("post"));
    app.open_submit_single();
    app.form.as_mut().unwrap().settings.mem = "12G".into();
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.is_none(), "{:?}", app.status);

    app.open_submit_single();
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.mem, "12G", "the next form starts from it");
    assert_eq!(form.resolved.origin(Field::Mem), Some(LAST_SET));
    assert_eq!(app.recent.entries()[0].settings.mem, "12G");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn esc_on_an_untouched_form_remembers_nothing() {
    let dir = temp("untouched");
    let mut app = app_in(&dir);
    assert!(app.select_namepath("post"));
    app.open_submit_single();
    press(&mut app, KeyCode::Esc);
    assert!(!dir.join(STATE_NAME).exists());
    assert!(app.recent.entries().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_recent_list_keeps_allocations_once_newest_first() {
    let dir = temp("recent");
    let mut recent = Recent::load(Some(&dir));
    let mut with_args = mem("8G");
    with_args.args = "--fast".into();
    recent.add("a", &with_args, "").unwrap();
    recent.add("b", &mem("16G"), "41").unwrap();
    recent.add("c", &mem("8G"), "").unwrap();
    recent.add("d", &Settings::default(), "").unwrap();

    let entries = Recent::load(Some(&dir)).entries().to_vec();
    assert!(dir.join(RECENT_NAME).exists());
    let rows: Vec<(&str, &str)> = entries
        .iter()
        .map(|e| (e.namepath.as_str(), e.settings.mem.as_str()))
        .collect();
    assert_eq!(rows, [("c", "8G"), ("b", "16G")], "8G once, empty never");
    assert!(entries.iter().all(|e| e.settings.args.is_empty()));
    assert_eq!(entries[1].job_id, "41");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn f6_offers_what_other_recipes_were_set_to() {
    let mut app = fixture_app();
    app.recent.add("build", &mem("48G"), "").unwrap();
    app.recent.add("post", &mem("2G"), "").unwrap();
    let entries = app.history_entries(Some("post"));
    let others: Vec<(&str, &str)> = entries
        .iter()
        .filter_map(|choice| match choice {
            Choice::Other(other) => Some((other.namepath, other.job_id)),
            _ => None,
        })
        .collect();
    assert_eq!(
        others,
        [("build", "")],
        "its own recipe's is prefilled instead"
    );
}

#[test]
fn partitions_offer_their_own_defaults() {
    let line =
        "PartitionName=gpu State=UP DefaultTime=02:00:00 DefMemPerNode=16384 MaxTime=2-00:00:00";
    let mut gpu = parse_partition(line).unwrap();
    assert_eq!(gpu.default_mem_mb, Some(16384));
    gpu.gres = vec!["gpu:a100:4".into()];
    let cluster = Cluster {
        partitions: vec![
            gpu,
            Partition {
                name: "short".into(),
                is_default: true,
                state: "UP".into(),
                ..Default::default()
            },
            Partition {
                name: "old".into(),
                state: "DOWN".into(),
                ..Default::default()
            },
        ],
        default_account: Some("lab".into()),
        ..Default::default()
    };

    let profiles = cluster.profiles();
    let names: Vec<&str> = profiles.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["short", "gpu"],
        "the default first, a down one not at all"
    );
    let gpu = &profiles[1].1;
    assert_eq!(gpu.partition, "gpu");
    assert_eq!(gpu.time, "02:00:00");
    assert_eq!(gpu.mem, "16G");
    assert_eq!(gpu.gpus, "1");
    assert_eq!(gpu.account, "lab");
}

#[test]
fn a_partition_row_sets_only_what_it_names() {
    let mut app = fixture_app();
    app.cluster = Some(Cluster {
        partitions: vec![Partition {
            name: "long".into(),
            state: "UP".into(),
            default_time: Some(600),
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(app.select_namepath("post"));
    app.open_submit_single();
    app.form.as_mut().unwrap().settings.mem = "32G".into();
    app.picker = Some(HistoryPick {
        namepath: Some("post".into()),
        index: 0,
        from: Mode::Submit,
    });
    app.mode = Mode::History;
    press(&mut app, KeyCode::Enter);

    let form = app.form.as_ref().unwrap();
    assert_eq!(form.settings.partition, "long");
    assert_eq!(form.settings.time, "10:00:00");
    assert_eq!(form.settings.mem, "32G");
}
