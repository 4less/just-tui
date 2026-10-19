# The documentation site

Plain HTML, published by GitHub Pages from this directory. The pages are
committed; the browser demo is not — `.github/workflows/pages.yml` compiles it
from the same commit and publishes both on every push to `main`.

## Publishing it

Once, in **Settings → Pages** on GitHub:

| | |
| --- | --- |
| Source | **GitHub Actions** |

Every push to `main` then runs the Pages workflow, and the site appears at
`https://4less.github.io/just-tui/` a few minutes later. **Actions → Pages →
Run workflow** republishes it without a push.

## Editing it

`style.css` and the page text in `_content.py` are what you edit. The chrome —
top bar, hero, sidebar, right rail, footer, pager — lives once in `_build.py`
and is stamped into each page. There are two layouts: `landing()` for the front
page, `doc_page()` for everything under Docs, which also builds the "on this
page" rail from the `<h2 id="…">` headings actually present, so it cannot drift
away from them.

After editing:

```sh
just docs        # regenerates the six pages
```

Commit what it writes. CI regenerates the pages too and fails when they differ
from what is committed, so an edit to `_content.py` cannot go out without them.

`.nojekyll` turns off Jekyll, which would otherwise try to process the site and
ignore files beginning with an underscore.

## The browser demo

`docs/demo/` is just-tui itself, compiled to WebAssembly. It is built from the same source as
the binary: the Slurm calls, the filesystem and the threads are answered from
`src/world/demo/` instead of an operating system, and ratzilla draws the interface into the
page. `#jobs` in the URL opens the job browser directly.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

just demo-build     # writes docs/demo/
just demo-serve     # http://localhost:8000/just-tui/demo/
```

The built files are not committed (`docs/demo/` is gitignored): the Pages workflow builds
them from the commit it publishes, so the demo always matches `main` and reports its
version. `just demo-serve` copies the site under a `just-tui/` prefix because the
bundle is built with that public path, which is where Pages will serve it from.

## Checking it before pushing

```sh
python3 -m http.server -d docs 8000    # then open http://localhost:8000
```
