//! Submitting one recipe over many inputs.
//!
//! A recipe with an `each` setting is expanded into one line of arguments per
//! input, written to a manifest beside the logs, and submitted as a single
//! Slurm array. Task *n* reads line *n+1* and runs the recipe with it.
//!
//! ```text
//! each = data/level3/*.fna     # a glob…
//! each = @runs.txt             # …or one line of arguments per row
//! args = input={} mode=peel    # {} is where each value lands
//! ```

use std::path::{Path, PathBuf};

use crate::slurm::{Settings, log_dir};

/// Where each value lands in the argument string.
const PLACEHOLDER: &str = "{}";

/// An expansion, ready to be written and submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// One line of arguments per task, in task order.
    pub lines: Vec<String>,
    /// Manifest path, relative to the job's working directory.
    pub manifest: PathBuf,
}

impl Plan {
    pub fn count(&self) -> usize {
        self.lines.len()
    }
}

/// Work out what a recipe expands to, without writing anything. `Ok(None)`
/// means the recipe is an ordinary single job.
pub fn plan(base: &Path, namepath: &str, settings: &Settings) -> Result<Option<Plan>, String> {
    let each = settings.each.trim();
    if each.is_empty() {
        return Ok(None);
    }

    let values = match each.strip_prefix('@') {
        Some(file) => from_file(base, file.trim())?,
        None => from_glob(base, each)?,
    };
    if values.is_empty() {
        return Err(format!("`{each}` matched nothing"));
    }

    let lines: Vec<String> = values
        .iter()
        .map(|value| line_for(&settings.args, value))
        .collect();

    // Named by what it contains, so submitting the same expansion twice
    // writes the same file, and a different one never overwrites a manifest
    // an array is still reading.
    let stem = crate::slurm::log_stem(namepath, settings);
    let manifest =
        log_dir(Path::new(""), namepath, settings).join(format!("{stem}-{}.args", digest(&lines)));

    Ok(Some(Plan { lines, manifest }))
}

/// One line of the manifest: the arguments with this value substituted in.
fn line_for(args: &str, value: &str) -> String {
    let args = args.trim();
    if args.contains(PLACEHOLDER) {
        return args.replace(PLACEHOLDER, value);
    }
    match args.is_empty() {
        true => value.to_owned(),
        false => format!("{args} {value}"),
    }
}

/// Put the manifest on disk. The directory is the log directory, which the
/// submit path creates anyway.
pub fn write(base: &Path, plan: &Plan) -> std::io::Result<()> {
    let path = base.join(&plan.manifest);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for line in &plan.lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(path, text)
}

/// The arguments a given task of an array ran with, read back from a manifest.
pub fn line(base: &Path, manifest: &str, task: usize) -> Option<String> {
    let text = std::fs::read_to_string(base.join(manifest)).ok()?;
    text.lines().nth(task).map(str::to_owned)
}

/// Lines of a file, ignoring blanks and comments, so a manifest can be
/// commented and kept under version control.
fn from_file(base: &Path, file: &str) -> Result<Vec<String>, String> {
    let path = base.join(file);
    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("could not read {}: {err}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

// ---------------------------------------------------------------------------
// Globbing
// ---------------------------------------------------------------------------

/// Expand a path pattern against the working directory. `*` and `?` work in
/// any component; results come back sorted, and relative when the pattern was,
/// since that is what the job's `--chdir` makes them relative to.
fn from_glob(base: &Path, pattern: &str) -> Result<Vec<String>, String> {
    let absolute = pattern.starts_with('/');
    let mut current: Vec<PathBuf> = vec![match absolute {
        true => PathBuf::from("/"),
        false => PathBuf::new(),
    }];

    for part in pattern.trim_start_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        let mut next = Vec::new();
        for prefix in &current {
            if !is_wild(part) {
                next.push(prefix.join(part));
                continue;
            }
            let dir = match prefix.as_os_str().is_empty() {
                true => base.to_path_buf(),
                false => match absolute {
                    true => prefix.clone(),
                    false => base.join(prefix),
                },
            };
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // A pattern never matches a dotfile unless it asks to.
                if name.starts_with('.') && !part.starts_with('.') {
                    continue;
                }
                if matches(part, &name) {
                    next.push(prefix.join(name));
                }
            }
        }
        current = next;
    }

    // Literal patterns are only worth reporting if they exist.
    let mut out: Vec<String> = current
        .into_iter()
        .filter(|path| {
            let full = match absolute {
                true => path.clone(),
                false => base.join(path),
            };
            full.exists()
        })
        .map(|path| path.display().to_string())
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

fn is_wild(part: &str) -> bool {
    part.contains('*') || part.contains('?')
}

/// `*` matches any run of characters, `?` exactly one. Backtracks on the last
/// star, which is all a path component ever needs.
fn matches(pattern: &str, name: &str) -> bool {
    let (pattern, name): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), name.chars().collect());
    let (mut p, mut n) = (0, 0);
    let (mut star, mut resume) = (None, 0);

    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some(p);
                resume = n;
                p += 1;
            }
            Some('?') => {
                p += 1;
                n += 1;
            }
            Some(c) if *c == name[n] => {
                p += 1;
                n += 1;
            }
            // Give the last star one more character and try again.
            _ => match star {
                Some(at) => {
                    p = at + 1;
                    resume += 1;
                    n = resume;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// FNV-1a, enough to name a file after its contents.
fn digest(lines: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for line in lines {
        for byte in line.as_bytes().iter().chain(b"\n") {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    format!("{hash:08x}")[..8].to_owned()
}
