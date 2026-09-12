# The documentation site

Plain HTML, served by GitHub Pages straight out of this directory. There is no
build step in CI: what is committed here is what is published.

## Publishing it

Once, in **Settings → Pages** on GitHub:

| | |
| --- | --- |
| Source | **Deploy from a branch** |
| Branch | `main` |
| Folder | `/docs` |

Save, and the site appears at `https://4less.github.io/just-tui/` within a
minute or two. Every push to `main` that touches `docs/` republishes it.

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

Commit what it writes. The generated HTML is committed deliberately: Pages
serves it without running anything, which is why the site cannot break in CI.

`.nojekyll` turns off Jekyll, which would otherwise try to process the site and
ignore files beginning with an underscore.

## Checking it before pushing

```sh
python3 -m http.server -d docs 8000    # then open http://localhost:8000
```
