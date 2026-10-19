//! Stamps the build with the commit it came from, so a binary on a cluster
//! can say what it is. Everything here is best-effort: a build from a tarball
//! with no git around still compiles, it just says `unknown`. A build of the
//! published crate is the release itself, and says so.

use std::process::Command;

fn main() {
    // The reflog is appended to by every commit, checkout and reset, which
    // `.git/HEAD` is not: that only moves when the branch does. Watching the
    // branch ref itself would miss a switch to another branch.
    for path in [".git/logs/HEAD", ".git/HEAD", ".git/index"] {
        println!("cargo:rerun-if-changed={path}");
    }

    // `v0.2.0` on the tag itself, `v0.2.0-4-g8a9c685` four commits past it,
    // and a bare hash before the first tag ever exists.
    //
    // `-dirty` only appears if something else already caused a rerun, since
    // an unstaged edit moves none of the files watched above. The stamp is
    // therefore exact for a committed tree — which is what gets deployed —
    // and may lag behind uncommitted local edits.
    //
    // Outside a checkout — `cargo install just-tui` builds from crates.io —
    // git would describe whatever repository happens to enclose the build
    // directory, so it is not asked at all.
    let checkout = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default())
        .join(".git")
        .exists();
    let (commit, date) = match checkout {
        true => (
            git(&["describe", "--tags", "--always", "--dirty"]),
            git(&["log", "-1", "--format=%cs"]),
        ),
        false => (
            format!(
                "v{}",
                std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
            ),
            "crates.io".to_owned(),
        ),
    };

    println!("cargo:rustc-env=JUST_TUI_COMMIT={commit}");
    println!("cargo:rustc-env=JUST_TUI_DATE={date}");
}

fn git(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}
