#!/usr/bin/env python3
"""Render the documentation site.

The pages are static HTML: GitHub Pages serves them straight from `docs/`
with no build step. This script exists only so the chrome — the top bar,
the sidebar, the footer — is written once instead of five times. Run it
after editing PAGES, and commit what it writes.
"""

import html
import pathlib
import re

ROOT = pathlib.Path(__file__).parent
REPO = "https://github.com/4less/just-tui"

NAV = [
    ("Getting started", [
        ("index.html", "What just-tui is"),
        ("index.html#install", "Installing"),
        ("index.html#quickstart", "Quickstart"),
    ]),
    ("Browsing", [
        ("recipes.html", "The explorer"),
        ("recipes.html#groups", "Groups"),
        ("recipes.html#search", "Search"),
        ("recipes.html#global", "Global recipes"),
    ]),
    ("Slurm", [
        ("slurm.html", "Submitting a recipe"),
        ("slurm.html#config", "Config files"),
        ("slurm.html#logs", "Where logs go"),
        ("slurm.html#batch", "One recipe, many inputs"),
    ]),
    ("Jobs", [
        ("jobs.html", "The job browser"),
        ("jobs.html#usage", "Live memory and CPU"),
        ("jobs.html#actions", "Kill and rerun"),
        ("jobs.html#history", "Past submissions"),
    ]),
    ("Reference", [
        ("keys.html", "Every key"),
        ("keys.html#files", "Files it writes"),
    ]),
]

FOOT_LINKS = [
    (REPO, "Repository"),
    (REPO + "/issues", "Issues"),
    ("https://github.com/casey/just", "just"),
    ("https://slurm.schedmd.com/", "Slurm"),
]


def chrome(page: str, title: str, crumbs: str, body: str, prev, nxt) -> str:
    nav = []
    for section, links in NAV:
        items = []
        for href, label in links:
            current = ' aria-current="page"' if href == page else ""
            items.append(f'<li><a href="{href}"{current}>{label}</a></li>')
        nav.append(f"<h2>{section}</h2>\n<ul>\n" + "\n".join(items) + "\n</ul>")

    top = []
    for href, label in [("index.html", "Overview"), ("recipes.html", "Recipes"),
                        ("slurm.html", "Slurm"), ("jobs.html", "Jobs"),
                        ("keys.html", "Keys")]:
        current = ' aria-current="page"' if href == page else ""
        top.append(f'<a href="{href}"{current}>{label}</a>')

    pager = ""
    if prev or nxt:
        parts = []
        if prev:
            parts.append(f'<a href="{prev[0]}"><span>Previous</span>← {prev[1]}</a>')
        if nxt:
            parts.append(f'<a class="next" href="{nxt[0]}"><span>Next</span>{nxt[1]} →</a>')
        pager = '<nav class="pager">' + "".join(parts) + "</nav>"

    footer = " · ".join(f'<a href="{h}">{t}</a>' for h, t in FOOT_LINKS)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)} — just-tui</title>
<meta name="description" content="just-tui: a terminal explorer for just recipes, with first-class Slurm submission and job monitoring.">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;700&display=swap" rel="stylesheet">
<link rel="stylesheet" href="style.css">
</head>
<body>
<header class="topbar">
  <a class="brand" href="index.html"><span class="just">just</span><span class="tui">tui</span></a>
  <nav class="topnav">{"".join(top)}</nav>
  <span class="spacer"></span>
  <span class="pill">v0.2.0</span>
  <a class="pill" href="{REPO}">GitHub ★</a>
</header>

<div class="shell">
  <aside class="sidebar">{"".join(nav)}</aside>
  <main>
    <p class="crumbs">{crumbs}</p>
    {body}
    {pager}
  </main>
</div>

<footer>
  <span>just-tui — a terminal explorer for <a href="https://github.com/casey/just">just</a> recipes</span>
  <span>{footer}</span>
</footer>
</body>
</html>
"""


def term(title: str, content: str) -> str:
    """A screenshot of the tool, framed as a terminal window."""
    dots = ('<span class="dot" style="background:#f7768e"></span>'
            '<span class="dot" style="background:#e0af68"></span>'
            '<span class="dot" style="background:#9ece6a"></span>')
    return (f'<div class="term"><div class="chrome">{dots}<span>{html.escape(title)}</span></div>'
            f"<pre><code>{html.escape(content)}</code></pre></div>")


def code(text: str) -> str:
    return f"<pre><code>{html.escape(text.strip())}</code></pre>"


def build(pages):
    for page, (title, crumbs, body, prev, nxt) in pages.items():
        (ROOT / page).write_text(chrome(page, title, crumbs, body, prev, nxt))
        print(f"wrote {page}")
