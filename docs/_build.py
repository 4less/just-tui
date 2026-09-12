#!/usr/bin/env python3
"""Render the site.

Two layouts: a landing page with a header, and the documentation shell —
sidebar, content, and a rail of the headings on the page. Both are static
HTML; this exists only so the chrome is written once rather than six times.
"""

import html
import pathlib
import re

ROOT = pathlib.Path(__file__).parent
REPO = "https://github.com/4less/just-tui"
VERSION = "v0.2.0"

TOP = [
    ("index.html", "Overview"),
    ("docs.html", "Docs"),
    ("slurm.html", "Slurm"),
    ("jobs.html", "Jobs"),
    ("keys.html", "Keys"),
]

NAV = [
    ("Getting started", [
        ("docs.html", "Introduction"),
        ("docs.html#install", "Installing"),
        ("docs.html#quickstart", "Quickstart"),
        ("docs.html#panes", "What the panes show"),
    ]),
    ("Browsing recipes", [
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

FOOT = [(REPO, "Repository"), (REPO + "/issues", "Issues"),
        ("https://github.com/casey/just", "just"),
        ("https://slurm.schedmd.com/", "Slurm")]


def head(title, desc, page):
    top = "".join(
        f'<a href="{h}"{" aria-current=\"page\"" if h == page else ""}>{t}</a>'
        for h, t in TOP)
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<meta name="description" content="{html.escape(desc)}">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;700&display=swap" rel="stylesheet">
<link rel="stylesheet" href="style.css">
</head>
<body>
<header class="topbar">
  <a class="brand" href="index.html"><span class="just">just</span><span class="tui">tui</span></a>
  <nav class="topnav">{top}</nav>
  <span class="spacer"></span>
  <span class="searchbox">search<span class="k">Ctrl K</span></span>
  <a class="btn" href="docs.html#install">cargo install --path .</a>
  <a class="btn" href="{REPO}">★ Star on GitHub</a>
</header>
"""


def foot():
    links = "".join(f'<a href="{h}">{t}</a>' for h, t in FOOT)
    return f"""
<footer><div class="inner">
  <span>just-tui · {VERSION} · a terminal explorer for <a href="https://github.com/casey/just">just</a> recipes</span>
  <span class="links">{links}</span>
</div></footer>
</body>
</html>
"""


def doc_page(page, title, crumbs, badge, body, prev, nxt):
    """Sidebar, content, and a rail listing this page's own headings."""
    nav = []
    for section, links in NAV:
        items = "".join(
            f'<li><a href="{h}"{" aria-current=\"page\"" if h == page else ""}>{t}</a></li>'
            for h, t in links)
        nav.append(f"<h2>{section}</h2><ul>{items}</ul>")

    # The rail is built from the headings actually in the body, so it cannot
    # drift away from them.
    heads = re.findall(r'<h2 id="([^"]+)">(?:<[^>]+>)*([^<]+)', body)
    rail = "".join(f'<li><a href="#{i}">{html.escape(t.strip())}</a></li>' for i, t in heads)
    rail_html = f'<div class="box"><h2>On this page</h2><ul>{rail}</ul></div>' if rail else ""

    pager = ""
    if prev or nxt:
        parts = []
        if prev:
            parts.append(f'<a href="{prev[0]}"><span>Previous</span>← {prev[1]}</a>')
        if nxt:
            parts.append(f'<a class="next" href="{nxt[0]}"><span>Next</span>{nxt[1]} →</a>')
        pager = f'<nav class="pager">{"".join(parts)}</nav>'

    return f"""{head(title + " — just-tui", title, page)}
<div class="crumbbar">
  <a href="index.html">just-tui</a> / <a href="docs.html">Docs</a> / {crumbs}
  <span class="spacer"></span>
  <span>{VERSION}</span>
</div>

<div class="shell">
  <aside class="sidebar">{"".join(nav)}</aside>
  <main>
    <p class="badge">{badge}</p>
    {body}
    {pager}
  </main>
  <aside class="rail">
    {rail_html}
    <div class="box"><h2>Repository</h2><ul>
      <li><a href="{REPO}">Browse the source</a></li>
      <li><a href="{REPO}/issues">Report an issue</a></li>
      <li><a href="{REPO}/blob/main/README.md">README</a></li>
    </ul></div>
  </aside>
</div>
{foot()}"""


def landing(title, desc, body):
    return f"{head(title, desc, 'index.html')}{body}{foot()}"


def term(title, content, right=""):
    dots = ('<span class="dot" style="background:#f7768e"></span>'
            '<span class="dot" style="background:#e0af68"></span>'
            '<span class="dot" style="background:#9ece6a"></span>')
    tail = f'<span class="right">{html.escape(right)}</span>' if right else ""
    return (f'<div class="term"><div class="chrome">{dots}<span>{html.escape(title)}</span>{tail}</div>'
            f"<pre><code>{html.escape(content.strip())}</code></pre></div>")


def panel(title, tag, body):
    tag_html = f'<span class="tag">{html.escape(tag)}</span>' if tag else ""
    return (f'<div class="panel"><header><b>{html.escape(title)}</b>{tag_html}</header>'
            f'<div class="body">{body}</div></div>')


def code(text):
    return f"<pre><code>{html.escape(text.strip())}</code></pre>"


def write(name, text):
    (ROOT / name).write_text(text)
    print(f"wrote {name}")
