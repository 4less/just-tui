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

# `just bump` raises the patch; `just bump minor` the minor. Nothing is pushed:
# check the tag, then `git push --follow-tags`.
#
# Refuses on a dirty tree or off main, since the tag is meant to name a state
# you could go back to.
[doc("Cut a version: bump the number, commit it, and tag it")]
bump level="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "working tree is dirty"; exit 1; }
    test "$(git branch --show-current)" = "main" || { echo "not on main"; exit 1; }

    current=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
    IFS=. read -r major minor patch <<< "$current"
    case "{{level}}" in
      major) major=$((major + 1)); minor=0; patch=0 ;;
      minor) minor=$((minor + 1)); patch=0 ;;
      patch) patch=$((patch + 1)) ;;
      *) echo "level must be major, minor or patch"; exit 1 ;;
    esac
    next="$major.$minor.$patch"

    sed -i "0,/^version = \".*\"/s//version = \"$next\"/" Cargo.toml
    cargo check --quiet          # so Cargo.lock follows the bump
    git add Cargo.toml Cargo.lock
    git commit -m "Release $next"
    git tag -a "v$next" -m "Release $next"
    echo "tagged v$next — push with: git push --follow-tags"

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
