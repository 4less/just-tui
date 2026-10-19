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

# `main` is protected: nothing reaches it except through a pull request. So a
# release is two steps. `just bump` (or `just bump minor`) puts the version
# change on a `release/vX.Y.Z` branch, pushes it, and opens the pull request
# (with `gh`, or prints the link). Merge it on GitHub, then `just tag` tags
# the merged commit on main and pushes the tag.
#
# Refuses on a dirty tree, off main, or behind origin, so the release is cut
# from exactly what is on GitHub.
[doc("Cut a version: bump the number on a release branch and open its pull request")]
bump level="patch":
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "working tree is dirty"; exit 1; }
    test "$(git branch --show-current)" = "main" || { echo "not on main"; exit 1; }
    git fetch --quiet origin main
    test "$(git rev-parse main)" = "$(git rev-parse origin/main)" \
      || { echo "main differs from origin/main — pull or push first"; exit 1; }

    current=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
    IFS=. read -r major minor patch <<< "$current"
    case "{{level}}" in
      major) major=$((major + 1)); minor=0; patch=0 ;;
      minor) minor=$((minor + 1)); patch=0 ;;
      patch) patch=$((patch + 1)) ;;
      *) echo "level must be major, minor or patch"; exit 1 ;;
    esac
    next="$major.$minor.$patch"
    branch="release/v$next"

    git switch --quiet -c "$branch"
    sed -i "0,/^version = \".*\"/s//version = \"$next\"/" Cargo.toml
    cargo check --quiet          # so Cargo.lock follows the bump
    git add Cargo.toml Cargo.lock
    git commit --quiet -m "Release $next"
    git push --quiet -u origin "$branch"
    git switch --quiet main

    if command -v gh >/dev/null; then
      gh pr create --base main --head "$branch" --title "Release $next" --body "Version $next."
    else
      repo=$(git remote get-url origin | sed -E 's#^(git@|https://)github.com[:/]##; s#\.git$##')
      echo "open the pull request: https://github.com/$repo/compare/main...$branch?expand=1"
    fi
    echo "once it is merged: just tag"

# Tag what is on main now with the version in Cargo.toml, and push the tag.
# Run after the release pull request is merged; the merge, not the branch
# commit, is what the tag should name.
[doc("Tag the merged release on main and push the tag")]
tag:
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "working tree is dirty"; exit 1; }
    test "$(git branch --show-current)" = "main" || { echo "not on main"; exit 1; }
    git pull --quiet --ff-only origin main

    version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
    if git rev-parse --quiet --verify "refs/tags/v$version" >/dev/null; then
      echo "v$version is already tagged — was the release pull request merged?"
      exit 1
    fi
    git tag -a "v$version" -m "Release $version"
    git push --quiet origin "v$version"
    git branch --quiet -D "release/v$version" 2>/dev/null || true
    echo "tagged and pushed v$version"

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
