//! Talking to the `just` binary.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::Justfile;

/// One justfile as `just` reported it, plus how to run its recipes.
pub struct Loaded {
    pub justfile: Justfile,
    /// Directory the recipes should be run from. For a project this is the
    /// justfile's own directory; for the global justfile it is wherever the
    /// user started just-tui, exactly where `just -g` would be typed.
    pub working_dir: PathBuf,
    /// Path of the root justfile, when just reported one.
    pub path: Option<PathBuf>,
    pub explicit_file: Option<PathBuf>,
    /// Name shown on the tree's root row.
    pub label: String,
    /// just's global justfile (`just --global-justfile`), not the project's.
    pub global: bool,
}

/// Whether a load failed only because there is no justfile to find, rather
/// than because one exists and is broken.
pub fn is_missing_justfile(err: &anyhow::Error) -> bool {
    let message = err.to_string();
    message.contains("No justfile found") || message.contains("Global justfile not found")
}

/// Ask `just` to describe a justfile as JSON. `args` choose which one, the
/// same way they will choose it when a recipe is run.
#[cfg(not(target_arch = "wasm32"))]
fn dump(dir: &Path, args: &[String]) -> Result<String> {
    use std::process::Command;

    let mut cmd = Command::new("just");
    cmd.current_dir(dir);
    cmd.args(args);
    cmd.args(["--dump", "--dump-format", "json"]);

    let output = cmd
        .output()
        .context("could not run `just` — is it installed and on PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.trim();
        return Err(anyhow::anyhow!(if message.is_empty() {
            "`just --dump` failed".to_owned()
        } else {
            message.to_owned()
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The demo carries one justfile, already dumped.
#[cfg(target_arch = "wasm32")]
fn dump(_dir: &Path, _args: &[String]) -> Result<String> {
    Ok(crate::world::demo::DUMP.to_owned())
}

/// Run `just --dump --dump-format json` and parse the result.
pub fn load(dir: &Path, explicit_file: Option<&Path>) -> Result<Loaded> {
    let args: Vec<String> = match explicit_file {
        Some(file) => vec!["--justfile".into(), file.display().to_string()],
        None => Vec::new(),
    };
    let dump = dump(dir, &args)?;
    let justfile: Justfile =
        serde_json::from_str(&dump).context("could not parse `just --dump` output")?;

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

/// Where just-tui keeps what it remembers about global recipes: their last
/// Slurm settings, run arguments, history and logs. Not recipes — those come
/// from just's own global justfile.
pub fn memory_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".just-tui"))
}

/// just's global justfile, the one `just --global-justfile` uses, asked of
/// `just` itself so the search is exactly its own. `Ok(None)` when there is
/// none. Its recipes run from `run_dir`, the directory just-tui was started
/// in, as they would if `just -g` were typed there.
pub fn load_global(run_dir: &Path) -> Result<Option<Loaded>> {
    let dump = match dump(run_dir, &["--global-justfile".to_owned()]) {
        Ok(dump) => dump,
        Err(err) if is_missing_justfile(&err) => return Ok(None),
        Err(err) => return Err(err),
    };
    let justfile: Justfile =
        serde_json::from_str(&dump).context("could not parse `just --dump` output")?;
    Ok(Some(Loaded {
        path: justfile.source.as_ref().map(PathBuf::from),
        justfile,
        working_dir: run_dir.to_path_buf(),
        explicit_file: None,
        label: "global".to_owned(),
        global: true,
    }))
}

/// Argument prefix that pins later invocations to the same justfile. A
/// global recipe runs as `just --global-justfile <recipe>` from the directory
/// just-tui was started in: exactly what typing `just -g` there would do,
/// `[no-cd]` and all.
pub fn file_args(loaded: &Loaded) -> Vec<String> {
    if loaded.global {
        return vec!["--global-justfile".into()];
    }
    match &loaded.explicit_file {
        Some(file) => vec!["--justfile".into(), file.display().to_string()],
        None => Vec::new(),
    }
}
