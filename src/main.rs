//! The native binary.
//!
//! Everything it does lives in the library, so that under WebAssembly — where
//! there is no terminal and no argument list — this compiles away to nothing
//! and only the browser front end is built.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    just_tui::cli::main()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
