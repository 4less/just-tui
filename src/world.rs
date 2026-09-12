//! Everything outside the process: other programs, the filesystem, threads.
//!
//! Natively each of these is the real thing. In the browser there is no
//! `just`, no Slurm, no disk and no threads, so the same calls are answered
//! from [`demo`] — canned output of the exact shape the parsers already read,
//! which is why nothing above this module needs to know which build it is in.

#[cfg(target_arch = "wasm32")]
pub mod demo;

// ---------------------------------------------------------------- processes --

/// Run a program and take its standard output, or `None` if it could not be
/// run. Every read of the cluster goes through here.
#[cfg(not(target_arch = "wasm32"))]
pub fn capture(program: &str, args: &[&str], timeout: std::time::Duration) -> Option<String> {
    use std::process::Command;
    use std::sync::mpsc;

    let (owned_program, owned_args) = (
        program.to_owned(),
        args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
    );
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Command::new(&owned_program).args(&owned_args).output());
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn capture(program: &str, args: &[&str], _timeout: std::time::Duration) -> Option<String> {
    demo::capture(program, args)
}

/// Run a program for its effect, reporting what it complained about.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(program: &str, args: &[&str], timeout: std::time::Duration) -> Result<(), String> {
    use std::process::Command;
    use std::sync::mpsc;

    let (owned_program, owned_args) = (
        program.to_owned(),
        args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
    );
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Command::new(&owned_program).args(&owned_args).output());
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .lines()
            .next()
            .unwrap_or("it failed without saying why")
            .to_owned()),
        Ok(Err(err)) => Err(format!("could not run {program}: {err}")),
        Err(_) => Err(format!("{program} did not answer")),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn run(program: &str, args: &[&str], _timeout: std::time::Duration) -> Result<(), String> {
    demo::run(program, args)
}

/// Like [`capture`], but run in a given directory and reporting why it failed.
#[cfg(not(target_arch = "wasm32"))]
pub fn capture_in(
    program: &str,
    args: &[&str],
    dir: &std::path::Path,
    timeout: std::time::Duration,
) -> Result<String, String> {
    use std::process::Command;
    use std::sync::mpsc;

    let (owned_program, owned_dir) = (program.to_owned(), dir.to_path_buf());
    let owned_args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(
            Command::new(&owned_program)
                .args(&owned_args)
                .current_dir(&owned_dir)
                .output(),
        );
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(Ok(output)) => Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .lines()
            .next()
            .unwrap_or("it failed without saying why")
            .to_owned()),
        Ok(Err(err)) => Err(format!("could not run {program}: {err}")),
        Err(_) => Err(format!("{program} did not answer")),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn capture_in(
    program: &str,
    args: &[&str],
    _dir: &std::path::Path,
    _timeout: std::time::Duration,
) -> Result<String, String> {
    demo::capture_in(program, args)
}

// --------------------------------------------------------------- filesystem --

#[cfg(not(target_arch = "wasm32"))]
pub fn read(path: &std::path::Path) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

#[cfg(target_arch = "wasm32")]
pub fn read(path: &std::path::Path) -> std::io::Result<String> {
    demo::read(path)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)
}

/// The demo has nowhere to write to, and nothing to gain from pretending.
#[cfg(target_arch = "wasm32")]
pub fn write(_path: &std::path::Path, _text: &str) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn create_dir_all(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[cfg(target_arch = "wasm32")]
pub fn create_dir_all(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

/// File names directly inside a directory, and whether each is itself a
/// directory. Enough for the log search and for globbing.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_dir(path: &std::path::Path) -> Vec<(std::path::PathBuf, bool)> {
    let Ok(entries) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| {
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            (entry.path(), is_dir)
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub fn read_dir(path: &std::path::Path) -> Vec<(std::path::PathBuf, bool)> {
    demo::read_dir(path)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn exists(path: &std::path::Path) -> bool {
    path.exists()
}

#[cfg(target_arch = "wasm32")]
pub fn exists(path: &std::path::Path) -> bool {
    demo::exists(path)
}

/// The last `limit` bytes of a file, and whether anything was cut off the
/// front. A log can be gigabytes; only its end ever matters.
#[cfg(not(target_arch = "wasm32"))]
pub fn tail_bytes(path: &std::path::Path, limit: u64) -> Option<(String, bool)> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let from = size.saturating_sub(limit);
    let _ = file.seek(SeekFrom::Start(from));

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    Some((String::from_utf8_lossy(&buffer).into_owned(), from > 0))
}

#[cfg(target_arch = "wasm32")]
pub fn tail_bytes(path: &std::path::Path, _limit: u64) -> Option<(String, bool)> {
    demo::read(path).ok().map(|text| (text, false))
}

// ------------------------------------------------------------------ threads --

/// Do something off the drawing thread. The browser has one thread — shared
/// memory there needs headers GitHub Pages cannot send — but the demo answers
/// from memory, so running it inline costs nothing: the result is waiting in
/// the channel by the time the next frame drains it.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<F: FnOnce() + Send + 'static>(work: F) {
    std::thread::spawn(work);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<F: FnOnce() + Send + 'static>(work: F) {
    work();
}

/// Whose jobs to ask about. There is no environment in a browser, so the
/// demo answers with the name its canned listings were recorded under.
#[cfg(not(target_arch = "wasm32"))]
pub fn user() -> String {
    std::env::var("USER").unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub fn user() -> String {
    "you".to_owned()
}

// --------------------------------------------------------------------- time --

/// A monotonic reading, for measuring how long ago something happened.
///
/// `std::time::Instant::now()` panics on a bare wasm target — there is no
/// clock behind it — so in the browser this reads `performance.now()`, which
/// is monotonic and in milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub type Instant = std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Instant(f64);

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub fn now() -> Self {
        let millis = ratzilla::web_sys::window()
            .and_then(|window| window.performance())
            .map(|performance| performance.now())
            .unwrap_or(0.0);
        Self(millis)
    }

    pub fn elapsed(&self) -> std::time::Duration {
        let millis = (Self::now().0 - self.0).max(0.0);
        std::time::Duration::from_millis(millis as u64)
    }
}

/// Seconds since the epoch. `SystemTime::now()` panics on wasm for the same
/// reason, so the browser reads the wall clock the page already has.
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
pub fn unix_seconds() -> u64 {
    // `Date.now()` is milliseconds since the epoch, which is the one clock a
    // page always has.
    let millis = ratzilla::web_sys::js_sys::Date::now();
    (millis / 1000.0) as u64
}

/// The local time, spelled the way `sacct` spells its timestamps.
#[cfg(not(target_arch = "wasm32"))]
pub fn timestamp() -> Option<String> {
    capture(
        "date",
        &["+%Y-%m-%dT%H:%M:%S"],
        std::time::Duration::from_secs(2),
    )
    .map(|text| text.trim().to_owned())
    .filter(|stamp| !stamp.is_empty())
}

#[cfg(target_arch = "wasm32")]
pub fn timestamp() -> Option<String> {
    demo::timestamp()
}
