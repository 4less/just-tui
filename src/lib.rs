//! just-tui — a file-explorer style browser for `just` recipes.
//!
//! A library so the same application can be driven by two front ends: the
//! native binary in `main.rs`, and the browser demo in [`web`], which runs the
//! whole interface against canned data through WebAssembly.

pub mod app;
pub mod batch;
pub mod config;
pub mod highlight;
pub mod history;
pub mod input;
pub mod just;
pub mod model;
pub mod slurm;
pub mod snapshot;
pub mod source;
pub mod submit;
pub mod theme;
pub mod tree;
pub mod ui;
pub mod world;

/// The command line, the event loop, and handing the screen to another
/// program: all of it native only.
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod terminal;

/// The browser front end.
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(test)]
mod tests;
