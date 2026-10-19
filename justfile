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

# Show what this build is: version, commit, and date
version:
    @cargo run --quiet -- --version

# Build the browser demo into docs/demo
#
# Needs the wasm target and trunk:
#   rustup target add wasm32-unknown-unknown
#   cargo install trunk
[doc("Compile the interface to WebAssembly for the docs site")]
demo-build:
    trunk build --release

# Depends on the build, because serving a stale bundle looks exactly like a
# change that did not work.
[doc("Serve the documentation site with the demo at its real path")]
demo-serve port="8000": demo-build
    @echo "http://localhost:{{port}}/just-tui/demo/"
    rm -rf .demo-serve && mkdir -p .demo-serve/just-tui
    cp -r docs/* .demo-serve/just-tui/
    python3 -m http.server -d .demo-serve {{port}}

# Regenerate the documentation site into docs/
docs:
    cd docs && python3 _content.py

# Serve the documentation site locally
docs-serve port="8000":
    @echo "http://localhost:{{port}}"
    python3 -m http.server -d docs {{port}}

# Install the binary into ~/.cargo/bin
install:
    cargo install --path .

# Remove build artefacts
clean:
    cargo clean

mod demo
