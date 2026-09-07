//! Talking to the `just` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::model::Justfile;

/// One justfile as `just` reported it, plus how to run its recipes.
pub struct Loaded {
    pub justfile: Justfile,
    /// Directory the recipes should be run from. For a project this is the
    /// justfile's own directory; for a global library it is wherever the user
    /// started just-tui, so `git::st` acts on the repo they are standing in.
    pub working_dir: PathBuf,
    /// Path of the root justfile, when just reported one.
    pub path: Option<PathBuf>,
    pub explicit_file: Option<PathBuf>,
    /// Name shown on the tree's root row.
    pub label: String,
    /// Came from the global library rather than the current project.
    pub global: bool,
}

/// Run `just --dump --dump-format json` and parse the result.
pub fn load(dir: &Path, explicit_file: Option<&Path>) -> Result<Loaded> {
    let mut cmd = Command::new("just");
    cmd.current_dir(dir);
    if let Some(file) = explicit_file {
        cmd.arg("--justfile").arg(file);
    }
    cmd.args(["--dump", "--dump-format", "json"]);

    let output = cmd
        .output()
        .context("could not run `just` — is it installed and on PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        return Err(anyhow!(if message.is_empty() {
            "`just --dump` failed".to_owned()
        } else {
            message.to_owned()
        }));
    }

    let justfile: Justfile =
        serde_json::from_slice(&output.stdout).context("could not parse `just --dump` output")?;

    let path = justfile.source.as_ref().map(PathBuf::from);
    let working_dir = path
        .as_ref()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| dir.to_path_buf());

    let label = path
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "justfile".to_owned());

    Ok(Loaded {
        justfile,
        working_dir,
        path,
        explicit_file: explicit_file.map(Path::to_path_buf),
        label,
        global: false,
    })
}

/// Where global recipe files live, `~/.justx` unless overridden.
pub fn global_dir(override_dir: Option<&Path>) -> Option<PathBuf> {
    match override_dir {
        Some(dir) => Some(dir.to_path_buf()),
        None => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".justx")),
    }
}

/// Load every `.just` file in the global directory, plus just's own global
/// justfile when one exists. Each becomes its own root in the explorer.
///
/// `run_dir` is where these recipes will execute — the directory just-tui was
/// started in, not the directory the files live in.
pub fn load_globals(dir: Option<&Path>, run_dir: &Path) -> Vec<Loaded> {
    let mut loaded = Vec::new();

    if let Some(dir) = dir {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && (path.extension().is_some_and(|e| e == "just")
                        || path.file_name().is_some_and(|n| n == "justfile"))
            })
            .collect();
        files.sort();

        for file in files {
            let Ok(mut source) = load(run_dir, Some(&file)) else {
                continue;
            };
            source.label = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "global".to_owned());
            source.working_dir = run_dir.to_path_buf();
            source.global = true;
            loaded.push(source);
        }
    }

    // `just --global-justfile`, for people who use that instead.
    if let Some(path) = just_global_justfile()
        && !loaded
            .iter()
            .any(|l| l.path.as_deref() == Some(path.as_path()))
        && let Ok(mut source) = load(run_dir, Some(&path))
    {
        source.label = "global".to_owned();
        source.working_dir = run_dir.to_path_buf();
        source.global = true;
        loaded.push(source);
    }

    loaded
}

/// The file `just -g` would use, if it exists.
fn just_global_justfile() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));

    [xdg.join("just/justfile"), home.join(".user.justfile")]
        .into_iter()
        .find(|p| p.is_file())
}

/// Argument prefix that pins later invocations to the same justfile.
pub fn file_args(loaded: &Loaded) -> Vec<String> {
    let Some(file) = &loaded.explicit_file else {
        return Vec::new();
    };
    let mut args = vec!["--justfile".into(), file.display().to_string()];
    // `--justfile` alone would move just into the file's own directory, which
    // is wrong for a global recipe meant to act on the current project.
    if loaded.global {
        args.push("--working-directory".into());
        args.push(loaded.working_dir.display().to_string());
    }
    args
}
