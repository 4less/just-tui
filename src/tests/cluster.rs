//! Slurm units, log paths, sbatch commands, and the config chain.

use std::path::Path;

use super::test_cluster;
use crate::config::{CONFIG_NAME, ConfigFile, Configs};
use crate::slurm::{self, Field, Settings};

#[test]
fn parses_slurm_units() {
    assert_eq!(slurm::parse_mem("92000"), Some(92_000));
    assert_eq!(slurm::parse_mem("64G"), Some(65_536));
    assert_eq!(slurm::parse_mem("1T"), Some(1_048_576));
    assert_eq!(slurm::parse_mem("UNLIMITED"), None);

    assert_eq!(slurm::parse_time("30"), Some(30));
    assert_eq!(slurm::parse_time("48:00:00"), Some(48 * 60));
    assert_eq!(slurm::parse_time("2-00:00:00"), Some(2 * 24 * 60));
    assert_eq!(slurm::parse_time("infinite"), None);

    assert_eq!(slurm::format_time(96 * 60), "4-00:00:00");
    assert_eq!(slurm::format_mem(65_536), "64G");
    assert_eq!(slurm::format_mem(92_000), "89.8G");
}

#[test]
fn logs_mirror_the_module_structure() {
    let plain = Settings::default();
    let (out, err) = slurm::log_paths("level3::db::tree", &plain);
    assert_eq!(out.to_str().unwrap(), "logs/level3/db/tree-%j.out");
    assert_eq!(err.to_str().unwrap(), "logs/level3/db/tree-%j.err");

    let (out, _) = slurm::log_paths("build", &plain);
    assert_eq!(out.to_str().unwrap(), "logs/build-%j.out");

    // Array tasks need the task id, or they all write to one file.
    let array = Settings {
        array: "0-3".into(),
        ..Default::default()
    };
    let (out, _) = slurm::log_paths("demo::greet", &array);
    assert_eq!(out.to_str().unwrap(), "logs/demo/greet-%A_%a.out");
}

#[test]
fn job_names_carry_the_arguments() {
    let args = Settings {
        args: "force=1 n=10".into(),
        ..Default::default()
    };
    assert_eq!(
        slurm::job_name("level3::db::tree", &args),
        "level3-db-tree-force-1-n-10"
    );
    // The module path is already in the directory, so the file stem is not.
    let (out, _) = slurm::log_paths("level3::db::tree", &args);
    assert_eq!(
        out.to_str().unwrap(),
        "logs/level3/db/tree-force-1-n-10-%j.out"
    );

    // A typed name replaces the generated one everywhere.
    let named = Settings {
        name: "nightly run".into(),
        args: "force=1".into(),
        ..Default::default()
    };
    assert_eq!(slurm::job_name("level3::db::tree", &named), "nightly-run");
    let (out, _) = slurm::log_paths("level3::db::tree", &named);
    assert_eq!(out.to_str().unwrap(), "logs/level3/db/nightly-run-%j.out");

    // With no arguments nothing changes from the plain recipe name.
    assert_eq!(slurm::job_name("build", &Settings::default()), "build");
}

#[test]
fn resolves_log_paths_once_a_job_has_an_id() {
    let plain = Settings::default();
    let (out, err) = slurm::resolved_log_paths("demo::greet", &plain, "12345");
    assert_eq!(out, "logs/demo/greet-12345.out");
    assert_eq!(err, "logs/demo/greet-12345.err");

    // Only the running task knows its own array index.
    let array = Settings {
        array: "0-3".into(),
        ..Default::default()
    };
    let (out, _) = slurm::resolved_log_paths("demo::greet", &array, "12345");
    assert_eq!(out, "logs/demo/greet-12345_*.out");
}

#[test]
fn builds_an_sbatch_command() {
    let settings = Settings {
        partition: "qib-compute".into(),
        cpus: "32".into(),
        mem: "128G".into(),
        time: "96:00:00".into(),
        extra: "--exclusive".into(),
        args: "force=1".into(),
        ..Default::default()
    };
    let args = slurm::build_command(Path::new("/work/proj"), "level3::db::tree", &settings);

    assert!(args.contains(&"--chdir=/work/proj".to_owned()));
    // The arguments are baked into the name, so two runs stay apart.
    assert!(args.contains(&"--job-name=level3-db-tree-force-1".to_owned()));
    assert!(args.contains(&"--output=logs/level3/db/tree-force-1-%j.out".to_owned()));
    assert!(args.contains(&"--partition=qib-compute".to_owned()));
    assert!(args.contains(&"--cpus-per-task=32".to_owned()));
    assert!(
        args.contains(&"--exclusive".to_owned()),
        "extra passes through"
    );
    assert!(
        !args.iter().any(|a| a.starts_with("--account")),
        "empty fields are omitted"
    );

    let wrap = args.iter().position(|a| a == "--wrap").expect("--wrap");
    assert_eq!(args[wrap + 1], "just level3::db::tree force=1");
}

#[test]
fn warns_when_a_request_exceeds_the_partition() {
    let cluster = test_cluster();
    let settings = Settings {
        partition: "short".into(),
        mem: "128G".into(),
        cpus: "64".into(),
        time: "48:00:00".into(),
        ..Default::default()
    };
    let warnings = slurm::warnings(&cluster, &settings);
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    assert!(warnings[0].contains("mem 128G exceeds 64G"));
    assert!(warnings[1].contains("64 cpus exceeds 16"));
    assert!(warnings[2].contains("exceeds 02:00:00"));

    let fits = Settings {
        partition: "short".into(),
        mem: "32G".into(),
        cpus: "8".into(),
        time: "01:00:00".into(),
        ..Default::default()
    };
    assert!(slurm::warnings(&cluster, &fits).is_empty());
}

#[test]
fn a_partition_summarises_its_limits() {
    let cluster = test_cluster();
    let summary = cluster.partition("short").unwrap().limits();
    assert!(summary.contains("16 cpu/node"));
    assert!(summary.contains("64G /node"));
    assert!(summary.contains("max 02:00:00"));
    assert_eq!(cluster.default_partition().unwrap().name, "short");
}

#[test]
fn config_files_parse_defaults_and_sections() {
    let text = "\
# a comment
partition = qib-compute
queue     = ignored-by-later-key
time      = 48:00:00
cpus      = 32

[level3::db::tree]
mem  = 128G
time = 96:00:00
";
    let file = ConfigFile::parse(Path::new("/x/.just-tui-cluster-config"), text);
    assert_eq!(file.defaults.time, "48:00:00");
    assert_eq!(file.defaults.cpus, "32");
    assert_eq!(
        file.defaults.partition, "ignored-by-later-key",
        "`queue` is an alias"
    );

    let scoped = file.settings_for("level3::db::tree");
    assert_eq!(scoped.mem, "128G");
    assert_eq!(
        scoped.time, "96:00:00",
        "the section overrides the defaults"
    );
    assert_eq!(scoped.cpus, "32", "defaults still apply");

    let other = file.settings_for("build");
    assert_eq!(other.mem, "", "sections apply to their recipe only");
}

#[test]
fn nearer_configs_override_wider_ones() {
    let dir = std::env::temp_dir().join("just-tui-config-test");
    std::fs::remove_dir_all(&dir).ok();
    let module = dir.join("level3");
    std::fs::create_dir_all(&module).unwrap();
    std::fs::write(
        dir.join(CONFIG_NAME),
        "partition = general\ntime = 24:00:00\ncpus = 8\n",
    )
    .unwrap();
    std::fs::write(module.join(CONFIG_NAME), "partition = bigmem\nmem = 256G\n").unwrap();

    let mut configs = Configs::new(&dir);
    let resolved = configs.resolve(&[dir.clone(), module.clone()], "level3::db::tree");

    assert_eq!(resolved.settings.partition, "bigmem", "the module wins");
    assert_eq!(resolved.settings.mem, "256G", "only the module sets this");
    assert_eq!(
        resolved.settings.time, "24:00:00",
        "the project still applies"
    );
    assert_eq!(resolved.sources.len(), 2);

    assert_eq!(
        resolved.origin(Field::Partition).unwrap(),
        "level3/.just-tui-cluster-config"
    );
    assert_eq!(
        resolved.origin(Field::Time).unwrap(),
        ".just-tui-cluster-config"
    );

    // A hand-written config outranks whatever the recipe last ran with: the
    // state file only fills in fields no config sets.
    configs
        .remember(
            "level3::db::tree",
            &Settings {
                partition: "gpu".into(),
                nodes: "4".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let mut reloaded = Configs::new(&dir);
    let resolved = reloaded.resolve(&[dir.clone(), module.clone()], "level3::db::tree");
    assert_eq!(
        resolved.settings.partition, "bigmem",
        "config beats last run"
    );
    assert_eq!(
        resolved.settings.nodes, "4",
        "last run fills what config leaves unset"
    );
    assert_eq!(resolved.origin(Field::Nodes).unwrap(), "last run");
    assert_eq!(resolved.settings.mem, "256G");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_config_is_created_from_a_template() {
    let dir = std::env::temp_dir().join("just-tui-template-test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let mut configs = Configs::new(&dir);

    let path = configs.ensure_file(&dir).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# just-tui Slurm defaults"));
    // Everything in the template is commented out, so it changes nothing.
    assert_eq!(
        ConfigFile::parse(&path, &text).defaults,
        Settings::default()
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn saving_defaults_writes_a_readable_file() {
    let dir = std::env::temp_dir().join("just-tui-save-test");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let mut configs = Configs::new(&dir);
    let settings = Settings {
        partition: "bigmem".into(),
        mem: "256G".into(),
        ..Default::default()
    };

    let path = configs.save_defaults(&dir, None, &settings).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("partition  = bigmem"));
    assert!(text.contains("mem        = 256G"));

    let reparsed = ConfigFile::parse(&path, &text);
    assert_eq!(
        reparsed.defaults, settings,
        "a saved file reads back the same"
    );

    std::fs::remove_dir_all(&dir).ok();
}
