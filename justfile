# just-tui — an explorer for just recipes
#
# Run `just` with no arguments to see this list.

set dotenv-load := false

# Directory the release binary lands in
target := "target/release/just-tui"

# Show every recipe in this justfile
default:
    @just --list --unsorted

# Build the debug binary
build:
    cargo build

# Build the optimised binary
release:
    cargo build --release

# Launch the explorer against a justfile
#
# With no argument it browses this project's own justfile.
# Pass a directory to browse a different project.
run dir=".":
    cargo run --quiet -- {{dir}}

# Run the test suite
test *filter:
    cargo test {{filter}}

# Format, lint, and test — the full check before committing
check: fmt-check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Rewrite the source with rustfmt
fmt:
    cargo fmt

# Fail if anything is unformatted
[private]
fmt-check:
    cargo fmt --check

# Install the binary into ~/.cargo/bin
install:
    cargo install --path .

# Remove build artefacts
clean:
    cargo clean

mod demo
