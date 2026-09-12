#!/usr/bin/env python3
"""The words. Run `just docs` to regenerate the site from this file."""

from _build import REPO, code, doc_page, landing, panel, term, write

EXPLORER = '''
 just  ~/git/4less/just-tui/justfile                16 recipes  1 module  2 global  ? help
╭ Explorer (15) ────────────────╮╭ Documentation ───────────────────────────────────────╮
│▾ just-tui/                    ││ log n="15"                                           │
││ ▸ demo/                      ││                                                      │
││ • build  Build the debug bi… ││ Show a compact log of the last commits               │
││ • check  Format, lint, and … ││                                                      │
││ • default ★  Show every rec… ││ Parameters                                           │
││ • test  Run the test suite   ││   n  default "15"                                    │
│⌂ ▾ git/                       │╰──────────────────────────────────────────────────────╯
││ • log ★  Show a compact log… │╭ git.just:3 ──────────────────────────────────────────╮
││ • st  Show what changed, st… ││  3 │ # Show a compact log of the last commits        │
│                               ││  4 │ log n="15":                                     │
│                               ││  5 │     git log --oneline --graph -n {{n}}          │
╰───────────────────────────────╯╰──────────────────────────────────────────────────────╯
 ↑↓ move →← open/close ⏎ run s slurm S jobs / search ⇥ pane ? help
'''

QUEUE = '''
╭ Slurm jobs — 5 of 5 in the last 7 days — live — all ──────────────────────────────────╮
│ ▸ 23474919   RUNNING    level3-run-s01_msa-nonfocal_tgt…  00:01:32   233G/1280G  18%  │
│   23474878   RUNNING    level3-run-s01_msa-nonfocal_cong… 06:39:02  1240G/1280G  97%  │
│   23474876   FAILED     level3-run-s01_msa-nonfocal_tgt…  00:00:00   exit 1           │
│   23473080   COMPLETED  level3-run-align-index            03:52:17   ok               │
│   23473075   OOM        level3-run-align-index            00:06:21   signal 125       │
│                                                                                       │
│   submitted 2026-09-08T22:00 · level3::run::s01_msa tgt_filt_peel · qib-compute        │
│╭ stderr — logs/level3/run/s01_msa-tgt_filt_peel-23474876.err ────────────────────────╮│
││ loading 4.2M rows                                                                   ││
││ slurmstepd: error: Exceeded job memory limit                                        ││
│╰─────────────────────────────────────────────────────────────────────────────────────╯│
╰ ↑↓ · v hide log · f filter · ⏎ log · u reuse · s rerun · x kill · X kill+rerun · esc ─╯
'''

# ================================================================= landing ===

LANDING = f"""
<section class="hero">
  <p class="badge">Terminal task runner · Slurm HPC</p>
  <h1>Your justfile, as a <em>file explorer</em>. Your recipes, as <em>cluster jobs</em>.</h1>
  <p>just-tui reads <code>just --dump</code> and lays a justfile out the way a file manager lays
  out a disk — modules are directories, recipes are files, and the right-hand split shows each
  recipe's documentation above its real source. Then it submits them to Slurm and watches what
  happens.</p>
  <div class="actions">
    <a class="btn solid" href="docs.html#install">Get started</a>
    <a class="btn" href="{REPO}">★ Star on GitHub</a>
  </div>
</section>

<div class="wrap">

<section class="section">
  {term("just-tui — the explorer", EXPLORER, "15 recipes")}
</section>

<section class="section">
  <h2>What it does</h2>
  <p class="sub">Three things a task runner in a terminal ought to do, and one it usually
  does not.</p>
  <div class="cards">
    <div class="card"><span class="num">01</span><h3>Reads a justfile properly</h3>
    <p>The whole comment block above a recipe, its parameters and defaults, dependencies split
    into those that run before and after the body, attributes and settings — not the one line
    <code>just --list</code> gives you.</p></div>
    <div class="card"><span class="num">02</span><h3>Submits the recipe itself</h3>
    <p>Not a generated script. Partitions, accounts and QoS are read off the cluster, each
    partition's limits shown beside it, and a request that exceeds them is flagged before you
    commit to it.</p></div>
    <div class="card"><span class="num">03</span><h3>Watches the queue</h3>
    <p>Running and finished jobs in one list, live memory and CPU against what they reserved, and
    the tail of the log that says why one failed — with the errors picked out.</p></div>
    <div class="card"><span class="num">04</span><h3>Remembers what it ran</h3>
    <p>Every submission is recorded with the exact settings it used, so a job that failed three
    weeks ago can be found, read, and sent again with one field changed.</p></div>
  </div>
</section>

<section class="section">
  <h2>The job browser</h2>
  <p class="sub">One key from the explorer. <code>squeue</code> for what is queued,
  <code>sacct</code> for what has finished, merged newest first.</p>
  {term("just-tui — Slurm jobs", QUEUE, "live")}
</section>

<section class="section">
  <h2>Get it</h2>
  <p class="sub">Not on crates.io yet, so install it from a clone. It needs
  <code>just</code> on your <code>PATH</code>.</p>
  {code('''
git clone https://github.com/4less/just-tui
cd just-tui
cargo install --path .
''')}
  <p style="text-align:center"><a class="btn solid" href="docs.html">Read the documentation →</a></p>
</section>

</div>
"""

# ============================================================ introduction ===

DOCS = f"""
<h1>Introduction</h1>

<p class="lead">just-tui is a terminal explorer for <a href="https://github.com/casey/just">just</a>
recipes. It runs them, submits them to Slurm, and watches the jobs that result — without leaving
the terminal or learning a second set of names for things.</p>

<h2 id="install">Installing</h2>

<p>just-tui is not published on crates.io yet, so install it from a clone:</p>

{code('''
git clone https://github.com/4less/just-tui
cd just-tui
cargo install --path .
''')}

<p>It needs <code>just</code> on <code>PATH</code> — it reads
<code>just --dump --dump-format json</code> and the justfiles that dump points at. Slurm's client
tools (<code>squeue</code>, <code>sbatch</code>, <code>sacct</code>, <code>sstat</code>,
<code>scontrol</code>) are needed only for the cluster features; without them everything else
works and the submit form falls back to free text.</p>

{panel("Which build am I running?", "version", f'''
<p>The binary stamps itself with the commit it came from, which is what tells two installs of one
release apart — useful after a <code>git pull</code> on a cluster.</p>
{code("""
$ just-tui --version
just-tui 0.2.0 (v0.2.0-4-g8a9c685, 2026-09-11)
""")}
<p>That reads: release 0.2.0, four commits past its tag, at <code>8a9c685</code>.</p>
''')}

<h2 id="quickstart">Quickstart</h2>

{code('''
just-tui                 # browse the justfile found from here
just-tui ../other        # browse another project
just-tui -f build.just   # a particular file
just-tui --list          # plain listing, no TUI
just-tui --no-global     # project recipes only
''')}

<p>Then <kbd>↑</kbd><kbd>↓</kbd> to move, <kbd>→</kbd> to open a module, <kbd>Enter</kbd> to run a
recipe, <kbd>s</kbd> to submit it to Slurm, <kbd>S</kbd> to watch the queue, <kbd>?</kbd> for every
key.</p>

<div class="note">
<strong>Running a recipe leaves the TUI</strong>, streams the output to the normal terminal, and
comes back on any key — so interactive recipes keep working.
</div>

<h2 id="panes">What the panes show</h2>

{term("just-tui — the explorer", EXPLORER, "15 recipes")}

<table>
<tr><th>Pane</th><th>Contents</th></tr>
<tr><td>Explorer</td><td>The root, its <code>mod</code>s, recipes and aliases, plus a
<code>⌂</code> root per global recipe file. The default recipe is starred; private ones are hidden
until <kbd>p</kbd>.</td></tr>
<tr><td>Documentation</td><td>The whole comment block above the recipe, parameters with defaults
and whether they are variadic, dependencies split into before and after, attributes, and the exact
<code>just …</code> line to invoke it.</td></tr>
<tr><td>Source</td><td>The recipe as written, line numbers matching the file, with highlighting for
interpolations, strings, shell variables and comments. A recipe that cannot be located in its file
is rebuilt from just's own dump and marked <code>reconstructed</code>.</td></tr>
</table>

<p><kbd>Tab</kbd> cycles focus, <kbd>f</kbd> zooms whichever pane has it, <kbd>v</kbd> puts the
documentation beside the source rather than above it, and <kbd>w</kbd> wraps long source lines.</p>
"""

# ================================================================ explorer ===

RECIPES = f"""
<h1>The explorer</h1>

<p class="lead">Modules are directories, recipes are files, aliases point at what they alias. The
tree is the justfile, including everything <code>mod</code> pulls in.</p>

<h2 id="groups">Groups</h2>

<p>A recipe carrying <code>[group('x')]</code> is filed under a collapsible folder inside its
module — beside the modules, never instead of them, which is what <code>just --list</code> does
too.</p>

{term("just-tui — groups", '''
│▾ project/                     ← the module
││ ▸ tools/                     ← a real `mod`, untouched
││ • default ★  Show every rec… ← ungrouped recipes stay put, above the folders
││ ▾ build  2 recipes           ← [group('build')]
││ │ • compile  Build the binary
││ │ • release  Optimised build
││ ▾ data  2 recipes            ← [group('data')]
││ │ • fetch  Download the inputs
││ │ • clean  Drop the cache
''')}

<p>The layer appears only where it divides something: a module whose recipes fall into two or more
buckets, counting ungrouped as one. A module with a single group, or none, is left alone.
<kbd>m</kbd> folds the layer away and back.</p>

<div class="note">
Groups are display only. A namepath never contains one, so running, submitting and config scopes
are unaffected.
</div>

<h2 id="search">Search</h2>

<p><kbd>/</kbd> matches recipe names as a fuzzy subsequence and documentation literally, and
highlights the characters it matched. It matches the file name too, so <code>/git</code> narrows to
a global <code>git.just</code> library and <code>/gitlog</code> finds one recipe inside it.
<kbd>q</kbd> clears the search first and quits second.</p>

<h2 id="global">Global recipes</h2>

<p>Every <code>.just</code> file in <code>~/.justx/</code> becomes its own root, marked
<code>⌂</code>, below the project. <code>just --global-justfile</code>'s file is picked up too if
you have one. They are browsed, documented, searched, run and submitted exactly like project
recipes; global libraries start collapsed, and just-tui still opens if there is no justfile here at
all.</p>

<div class="note warn">
<strong>Global recipes run in your current directory</strong>, not in <code>~/.justx</code>:
just-tui passes <code>--working-directory</code>, so <code>git::log</code> acts on the repository
you are standing in.
</div>
"""

# =================================================================== slurm ===

SLURM = f"""
<h1>Submitting a recipe</h1>

<p class="lead"><kbd>s</kbd> opens a form for the selected recipe. It submits the <em>recipe</em>,
not a generated script — nothing is inferred from the justfile, and fields start empty unless a
config file or a previous run filled them in.</p>

{code('sbatch --chdir=<justfile dir> --job-name=… --output=… <your flags> --wrap "just demo::greet"')}

{panel("Partitions are read off the cluster", "auto-detected", '''
<p>On opening the form just-tui asks <code>scontrol show partition</code>, <code>sinfo</code> and
<code>sacctmgr</code>, so <kbd>←</kbd><kbd>→</kbd> on the partition, account and QoS fields step
through what actually exists — each partition's limits beside it
(<code>24 cpu/node · 89.8G /node · max 02:00:00</code>). A request that exceeds the selected
partition is flagged before you submit. Detection runs on a thread, so the form opens at once and
the pick lists fill in when the controller answers.</p>
''')}

<h3>Editing a field</h3>

<table>
<tr><th>Key</th><th>Does</th></tr>
<tr><td><kbd>←</kbd> <kbd>→</kbd></td><td>move the caret — or step a pick list, on the fields the cluster fills in</td></tr>
<tr><td><kbd>Ctrl-←</kbd> <kbd>Ctrl-→</kbd></td><td>move the caret on those fields too</td></tr>
<tr><td><kbd>Home</kbd> <kbd>End</kbd>, <kbd>Ctrl-a</kbd> <kbd>Ctrl-e</kbd></td><td>either end of the field</td></tr>
<tr><td><kbd>Backspace</kbd> <kbd>Del</kbd></td><td>delete before the caret, delete under it</td></tr>
<tr><td><kbd>Ctrl-u</kbd></td><td>clear the field</td></tr>
</table>

<h3>Job names</h3>

<p>The name is built from the recipe <em>and its arguments</em>, so two runs of one recipe stay
apart in <code>squeue</code> and in <code>logs/</code>:</p>

<table>
<tr><th>Recipe</th><th>Args</th><th>Job name</th><th>Log file</th></tr>
<tr><td><code>build</code></td><td></td><td><code>build</code></td><td><code>logs/build-%j.out</code></td></tr>
<tr><td><code>demo::greet</code></td><td><code>ada</code></td><td><code>demo-greet-ada</code></td><td><code>logs/demo/greet-ada-%j.out</code></td></tr>
</table>

<p>The <code>name</code> field at the top overrides it: leave it empty and the name is generated,
type one and it is used verbatim, tidied into a safe file name.</p>

<h2 id="config">Config files</h2>

<p><code>.just-tui-cluster-config</code> holds Slurm defaults, and is read from several places:</p>

{code('''
# defaults for every recipe in this scope
partition = qib-compute
time      = 48:00:00
cpus      = 32
mem       = 64G

[level3::db::tree]   # one recipe, by its full path
mem  = 128G
time = 96:00:00
''')}

<p>Keys: <code>partition</code> (or <code>queue</code>), <code>account</code>, <code>qos</code>,
<code>cpus</code>, <code>mem</code>, <code>time</code>, <code>nodes</code>, <code>gpus</code> (or
<code>gres</code>), <code>array</code>, <code>extra</code>, <code>args</code>, <code>each</code>
(or <code>glob</code>, <code>inputs</code>). An empty or absent key means the flag is not passed.</p>

<h3>Precedence, weakest first</h3>

<ol class="prose">
<li>what that recipe was last submitted with (<code>.just-tui-cluster-state</code>)</li>
<li><code>~/.just-tui-cluster-config</code></li>
<li><code>&lt;justfile dir&gt;/.just-tui-cluster-config</code></li>
<li>each module directory's config, outermost first</li>
<li>a <code>[recipe::path]</code> section in any of the above</li>
<li>whatever you type in the form</li>
</ol>

<p><strong>The nearer config always wins</strong>, and every config beats the automatically
recorded state. The form shows where the selected field's value came from, beside it.</p>

<h3>Writing one from the form</h3>

<table>
<tr><th>Key</th><th>Writes</th></tr>
<tr><td><kbd>F2</kbd></td><td>the current values as <strong>module</strong> defaults</td></tr>
<tr><td><kbd>F3</kbd></td><td>as <strong>project</strong> defaults</td></tr>
<tr><td><kbd>F4</kbd></td><td>as a <code>[recipe]</code> section, this recipe only</td></tr>
<tr><td><kbd>F5</kbd></td><td>pick a scope and open its config in <code>$EDITOR</code></td></tr>
<tr><td><kbd>F6</kbd></td><td>past submissions of this recipe, to load one back</td></tr>
</table>

<h2 id="logs">Where logs go</h2>

<p>Always under <code>logs/</code> in the justfile's base directory, mirroring the module structure.
Array jobs use <code>-%A_%a</code> instead of <code>-%j</code>. The directory is created before
submitting.</p>

<table>
<tr><th>Recipe</th><th>Log file</th></tr>
<tr><td><code>build</code></td><td><code>logs/build-%j.out</code></td></tr>
<tr><td><code>demo::greet</code></td><td><code>logs/demo/greet-%j.out</code></td></tr>
<tr><td><code>level3::db::tree</code></td><td><code>logs/level3/db/tree-%j.out</code></td></tr>
</table>

<h2 id="batch">One recipe, many inputs</h2>

<p><code>each</code> expands a recipe into one Slurm array task per input:</p>

{code('''
[level3::run::s01_msa]
each = data/level3/*.fna     # a glob…
args = {}                    # …and where each match lands

[level3::run::s02_refilter]
each = @arms.txt             # …or one line of arguments per row
''')}

<p><code>{{}}</code> works like <code>xargs -I{{}}</code>, and is replaced everywhere it appears.
Without it the value is appended, which is how <code>just</code> takes a positional parameter. With
<code>@file</code>, blank lines and <code>#</code> comments are skipped, so a run list can be kept
under version control and commented.</p>

<div class="note snag">
<strong><code>just</code> takes recipe parameters positionally.</strong> Write
<code>args = {{}}</code>, not <code>args = input={{}}</code> — the latter passes the literal string
<code>input=data/x.fna</code> as the first parameter.
</div>

<h3>The manifest</h3>

<p>On submit the expansion is written beside the logs — one line of arguments per task — and a
single array is submitted that reads it:</p>

{code('''
logs/level3/run/s01_msa-245df667.args
  data/level3/focal_target.fna      ← task 0
  data/level3/nonfocal_cong.fna     ← task 1
  …

sbatch --array=0-36 --wrap 'just level3::run::s01_msa $(sed -n "$((SLURM_ARRAY_TASK_ID+1))p" logs/…/s01_msa-245df667.args)'
''')}

<p>The manifest is named after <em>its own contents</em>, so resubmitting an unchanged expansion
writes the same file, and a changed one takes a new name rather than overwriting a manifest an array
is still reading its way through. Task order is sorted, so a task number means the same thing every
time. The form says what it will do before you commit — <code>expands 37 jobs · first:
data/level3/focal_target.fna</code> — and names the manifest it would write.</p>

<h3><code>each</code> and <code>array</code></h3>

<p>Both make a job array, from opposite ends:</p>

<table>
<tr><th></th><th><code>array</code></th><th><code>each</code></th></tr>
<tr><td>you write</td><td>the range: <code>0-31%4</code></td><td>the inputs: <code>data/*.fna</code></td></tr>
<tr><td>range comes from</td><td>you</td><td>the number of matches</td></tr>
<tr><td>the recipe</td><td>reads <code>$SLURM_ARRAY_TASK_ID</code> itself</td><td>takes ordinary arguments</td></tr>
<tr><td>a manifest is written</td><td>no</td><td>yes</td></tr>
</table>

<p>Set both and <code>each</code> wins the range, keeping only a <code>%n</code> throttle from
<code>array</code>. The form says so on the field, and warns if a range typed there is being
dropped.</p>

<div class="note warn">
Every task of an array gets the <strong>same</strong> <code>--mem</code>, <code>--cpus</code> and
<code>--time</code>, so this suits work that is uniform across inputs. Rerunning a few failed tasks
means resubmitting with <code>--array=3,7,19</code> rather than pressing <kbd>s</kbd>.
</div>
"""

# ==================================================================== jobs ===

JOBS = f"""
<h1>The job browser</h1>

<p class="lead"><kbd>S</kbd> merges <code>squeue</code> for what is queued or running with
<code>sacct</code> for what has finished, newest first, and puts the tail of the selected job's log
underneath.</p>

{term("just-tui — Slurm jobs", QUEUE, "live")}

<p>Anything that looks like an error is picked out in red. The log file is found by asking
<code>scontrol</code> while the controller still knows the job, and otherwise by looking for a file
ending in <code>-&lt;job id&gt;</code> under the job's working directory — so jobs submitted outside
just-tui are readable too. Finding it happens on a thread, so the list stays navigable while it
works.</p>

<table>
<tr><th>Key</th><th>Does</th></tr>
<tr><td><kbd>↑</kbd> <kbd>↓</kbd></td><td>move between jobs; the log follows</td></tr>
<tr><td><kbd>v</kbd></td><td>hide the log pane — the queue takes the whole window</td></tr>
<tr><td><kbd>f</kbd></td><td>filter window: pick which states to show</td></tr>
<tr><td><kbd>Tab</kbd></td><td>switch between <code>stderr</code> and <code>stdout</code></td></tr>
<tr><td><kbd>Enter</kbd> <kbd>o</kbd></td><td>open the whole log in <code>$PAGER</code></td></tr>
<tr><td><kbd>d</kbd></td><td>how far back to look: 1 / 7 / 30 / 90 days</td></tr>
<tr><td><kbd>p</kbd></td><td>pause or resume the five-second refresh</td></tr>
<tr><td><kbd>r</kbd></td><td>reload · <kbd>y</kbd> copy the log path · <kbd>PgUp</kbd> <kbd>PgDn</kbd> <kbd>g</kbd> <kbd>G</kbd> scroll</td></tr>
</table>

<p><kbd>f</kbd> lists every state this cluster has reported alongside the usual ones —
<span class="state run">RUNNING</span> <span class="state pend">PENDING</span>
<span class="state fail">FAILED</span> and the rest — each with how many jobs hold it.
<kbd>Space</kbd> turns one on or off and the list behind follows at once, so there is nothing to
apply; <kbd>a</kbd> goes back to everything.</p>

<h2 id="usage">Live memory and CPU</h2>

<p>A running job shows what it is <strong>actually using</strong> against what it reserved:</p>

{code('''
23474919  RUNNING   level3-run-s01_msa-nonfocal_tgt…   00:01:32   233G/1280G  18%   64c  93%
23474878  RUNNING   level3-run-s01_msa-nonfocal_cong…  06:39:02  1240G/1280G  97%   64c  98%
''')}

<p><code>MaxRSS</code> and <code>AveCPU</code> come from one <code>sstat</code> call covering every
running job at once, not one per job. CPU % is CPU time consumed against CPU time reserved —
<code>AveCPU / (elapsed × cpus)</code> — so 100% means every allocated core was busy throughout.</p>

<p>Colours read as strain: memory is red near its limit, and <strong>CPU is the other way
round</strong> — red means allocated cores are idling, green means they are busy.</p>

{panel("Nothing waits on the scheduler", "threads", '''
<p>The clock on a running job ticks every second, carried forward locally from the elapsed time
<code>squeue</code> reported; the queue itself is re-asked every five seconds. Every call to Slurm —
<code>squeue</code>, <code>sacct</code>, <code>sstat</code>, <code>scontrol</code>, and the
detection behind the submit form — runs on a thread and is delivered through a channel the event
loop drains before each frame, so none of it lands in the middle of a keystroke.</p>
''')}

<h2 id="actions">Kill and rerun</h2>

<table>
<tr><th>Key</th><th>Does</th></tr>
<tr><td><kbd>s</kbd></td><td>submit the same job again — for one that has <strong>stopped</strong></td></tr>
<tr><td><kbd>x</kbd></td><td>cancel the job (<code>scancel</code>)</td></tr>
<tr><td><kbd>X</kbd></td><td>cancel it <strong>and</strong> submit the same job again</td></tr>
</table>

<p>All three read the settings back from the history record, so a job that needs another attempt
does not have to be retyped, and all three ask first — taking nothing but <kbd>y</kbd>, since
<kbd>Enter</kbd> is the key most likely to be leaned on:</p>

{code('''
╭ Kill this job and run it again? ─────────────────────────────╮
│  scancel 23474919   level3-run-s02_refilter-tgt_filt_rank    │
│                                                              │
│  then submit the same job again:                             │
│  $ sbatch --partition=qib-compute --cpus-per-task=64 --mem…  │
╰ y do it · any other key leaves the job alone ────────────────╯
''')}

<p><kbd>s</kbd> is refused while a job is still queued or running — two copies is almost never what
was meant, and <kbd>X</kbd> is the key for replacing one. <kbd>s</kbd> and <kbd>X</kbd> need a
history record, so they only offer themselves for jobs just-tui submitted; <kbd>x</kbd> works on
anything Slurm lets you cancel.</p>

<h2 id="history">Past submissions</h2>

<p>Every submission is appended to <code>.just-tui-cluster-history</code> beside the justfile — the
job id, the time, the exact <code>sbatch</code> line, and every field the form held. Unlike
<code>.just-tui-cluster-state</code>, which keeps only the last run of each recipe and is overridden
by any config file, this one forgets nothing.</p>

<ul class="prose">
<li><kbd>H</kbd> lists every past submission; <kbd>Enter</kbd> loads one back into the form.</li>
<li><kbd>F6</kbd> in the form lists only that recipe's past runs — and since the title already names
the recipe, those rows show the <strong>arguments</strong>, which is what tells two runs apart.</li>
<li><kbd>u</kbd> in the job browser jumps from a job to the settings it ran with.</li>
</ul>

<p>The file is machine-written and gitignored; it is trimmed to the last 500 entries.</p>
"""

# =============================================================== reference ===

KEYS = f"""
<h1>Every key</h1>

<h2 id="explorer">The explorer</h2>
<table>
<tr><th>Key</th><th>Action</th></tr>
<tr><td><kbd>j</kbd> <kbd>k</kbd> <kbd>↑</kbd> <kbd>↓</kbd></td><td>move in the focused pane</td></tr>
<tr><td><kbd>J</kbd> <kbd>K</kbd></td><td>next / previous recipe, skipping modules</td></tr>
<tr><td><kbd>h</kbd> <kbd>l</kbd> <kbd>←</kbd> <kbd>→</kbd></td><td>collapse / expand a module</td></tr>
<tr><td><kbd>Enter</kbd> <kbd>Space</kbd></td><td>run a recipe, toggle a module, follow an alias</td></tr>
<tr><td><kbd>r</kbd></td><td>run the selected recipe</td></tr>
<tr><td><kbd>a</kbd></td><td>run it with extra arguments</td></tr>
<tr><td><kbd>n</kbd></td><td>dry run (<code>just --dry-run</code>)</td></tr>
<tr><td><kbd>/</kbd></td><td>fuzzy search names, literal search in docs</td></tr>
<tr><td><kbd>Tab</kbd> <kbd>Shift-Tab</kbd></td><td>cycle pane focus</td></tr>
<tr><td><kbd>g</kbd> <kbd>G</kbd> <kbd>PgUp</kbd> <kbd>PgDn</kbd> <kbd>Ctrl-u</kbd> <kbd>Ctrl-d</kbd></td><td>scroll</td></tr>
<tr><td><kbd>e</kbd> <kbd>c</kbd></td><td>expand / collapse every module</td></tr>
<tr><td><kbd>p</kbd></td><td>show or hide private recipes</td></tr>
<tr><td><kbd>m</kbd></td><td>fold the <code>[group(…)]</code> layer in or out</td></tr>
<tr><td><kbd>v</kbd></td><td>documentation above the source, or beside it</td></tr>
<tr><td><kbd>w</kbd></td><td>wrap long source lines</td></tr>
<tr><td><kbd>f</kbd></td><td>zoom the focused pane</td></tr>
<tr><td><kbd>y</kbd></td><td>copy the recipe source (OSC 52, works over ssh)</td></tr>
<tr><td><kbd>o</kbd></td><td>open the justfile at that line in <code>$EDITOR</code></td></tr>
<tr><td><kbd>R</kbd></td><td>reload</td></tr>
<tr><td><kbd>s</kbd></td><td>submit the recipe to Slurm</td></tr>
<tr><td><kbd>S</kbd></td><td>browse Slurm jobs, their usage and logs</td></tr>
<tr><td><kbd>H</kbd></td><td>past submissions, to reuse settings</td></tr>
<tr><td><kbd>?</kbd></td><td>help</td></tr>
<tr><td><kbd>q</kbd></td><td>clear the search, then quit · <kbd>Ctrl-c</kbd> quits at once</td></tr>
</table>

<h2 id="form">The submit form</h2>
<table>
<tr><th>Key</th><th>Action</th></tr>
<tr><td><kbd>↑</kbd> <kbd>↓</kbd> <kbd>Tab</kbd></td><td>move between fields</td></tr>
<tr><td><kbd>←</kbd> <kbd>→</kbd></td><td>move the caret — or step a pick list, on the fields the cluster fills in</td></tr>
<tr><td><kbd>Ctrl-←</kbd> <kbd>Ctrl-→</kbd></td><td>move the caret on those fields too</td></tr>
<tr><td><kbd>Home</kbd> <kbd>End</kbd>, <kbd>Ctrl-a</kbd> <kbd>Ctrl-e</kbd></td><td>either end of the field</td></tr>
<tr><td><kbd>Backspace</kbd> <kbd>Del</kbd></td><td>delete before the caret, delete under it</td></tr>
<tr><td><kbd>Ctrl-u</kbd></td><td>clear the field</td></tr>
<tr><td><kbd>Enter</kbd></td><td>submit</td></tr>
<tr><td><kbd>F2</kbd> <kbd>F3</kbd> <kbd>F4</kbd></td><td>save as module / project / recipe defaults</td></tr>
<tr><td><kbd>F5</kbd></td><td>open a config in <code>$EDITOR</code></td></tr>
<tr><td><kbd>F6</kbd></td><td>past submissions of this recipe</td></tr>
<tr><td><kbd>Esc</kbd></td><td>close without submitting</td></tr>
</table>

<h2 id="browser">The job browser</h2>
<table>
<tr><th>Key</th><th>Action</th></tr>
<tr><td><kbd>↑</kbd> <kbd>↓</kbd></td><td>move between jobs</td></tr>
<tr><td><kbd>v</kbd></td><td>hide or show the log pane</td></tr>
<tr><td><kbd>f</kbd></td><td>filter window — <kbd>Space</kbd> toggles a state, <kbd>a</kbd> shows all</td></tr>
<tr><td><kbd>Tab</kbd></td><td><code>stderr</code> ↔ <code>stdout</code></td></tr>
<tr><td><kbd>Enter</kbd> <kbd>o</kbd></td><td>open the log in <code>$PAGER</code></td></tr>
<tr><td><kbd>u</kbd></td><td>load this job's settings into the submit form</td></tr>
<tr><td><kbd>s</kbd></td><td>submit the same job again (once it has stopped)</td></tr>
<tr><td><kbd>x</kbd> <kbd>X</kbd></td><td>kill it · kill it and submit it again</td></tr>
<tr><td><kbd>d</kbd></td><td>1 / 7 / 30 / 90 days</td></tr>
<tr><td><kbd>p</kbd></td><td>pause or resume the refresh</td></tr>
<tr><td><kbd>r</kbd></td><td>reload · <kbd>y</kbd> copy the log path</td></tr>
<tr><td><kbd>Esc</kbd></td><td>back to the explorer</td></tr>
</table>

<h2 id="files">Files it writes</h2>
<table>
<tr><th>File</th><th>Written by</th><th>What it holds</th></tr>
<tr><td><code>.just-tui-cluster-config</code></td><td>you, or <kbd>F2</kbd>–<kbd>F4</kbd></td><td>Slurm defaults for a scope or a recipe. Worth committing.</td></tr>
<tr><td><code>.just-tui-cluster-state</code></td><td>just-tui</td><td>What each recipe was last submitted with. Gitignored.</td></tr>
<tr><td><code>.just-tui-cluster-history</code></td><td>just-tui</td><td>Every submission ever made, one JSON line each. Gitignored, trimmed to 500.</td></tr>
<tr><td><code>logs/…</code></td><td>Slurm</td><td>Job output, mirroring the module tree. Gitignored.</td></tr>
<tr><td><code>logs/….args</code></td><td>just-tui</td><td>An expansion's manifest, one line of arguments per array task.</td></tr>
</table>
"""

write("index.html", landing(
    "just-tui — a terminal explorer for just recipes, with Slurm",
    "just-tui lays a justfile out like a file explorer, submits recipes to Slurm, "
    "and watches the jobs that result.",
    LANDING))

for name, title, crumb, badge, body, prev, nxt in [
    ("docs.html", "Introduction", "Getting started", "Getting started", DOCS,
     ("index.html", "Overview"), ("recipes.html", "The explorer")),
    ("recipes.html", "The explorer", "Browsing recipes", "Browsing recipes", RECIPES,
     ("docs.html", "Introduction"), ("slurm.html", "Submitting a recipe")),
    ("slurm.html", "Submitting a recipe", "Slurm", "Slurm integration", SLURM,
     ("recipes.html", "The explorer"), ("jobs.html", "The job browser")),
    ("jobs.html", "The job browser", "Jobs", "Monitoring", JOBS,
     ("slurm.html", "Submitting a recipe"), ("keys.html", "Every key")),
    ("keys.html", "Every key", "Reference", "Reference", KEYS,
     ("jobs.html", "The job browser"), None),
]:
    write(name, doc_page(name, title, crumb, badge, body, prev, nxt))
