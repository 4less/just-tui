# just-tui

A terminal file explorer for [`just`](https://github.com/casey/just) recipes. Modules are
directories, recipes are files, and the right-hand split shows each recipe's documentation
above its real source.

```
 just  ~/git/4less/just-tui/justfile                             16 recipes  1 module  2 global  ? help
╭ Explorer (15) ────────────────╮╭ Documentation ──────────────────────────────────────────────────────╮
│▾ just-tui/                    ││ log n="15"                                                          │
││ ▸ demo/                      ││                                                                     │
││ • build  Build the debug bi… ││ Show a compact log of the last commits                              │
││ • check  Format, lint, and … ││                                                                     │
││ • clean  Remove build artef… ││ Parameters                                                          │
││ • default ★  Show every rec… ││   n  default "15"                                                   │
││ • fmt  Rewrite the source w… ││                                                                     │
││ • install  Install the bina… ││ Invoke                                                              │
││ • release  Build the optimi… │╰─────────────────────────────────────────────────────────────────────╯
││ • run  Launch the explorer … │╭ git.just:3 ─────────────────────────────────────────────────────────╮
││ • test  Run the test suite   ││  3 │ # Show a compact log of the last commits                       │
│⌂ ▸ docker/                    ││  4 │ log n="15":                                                    │
│⌂ ▾ git/                       ││  5 │     git log --oneline --graph --decorate -n {{n}}              │
││ • log ★  Show a compact log… ││                                                                     │
││ • st  Show what changed, st… ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
│                               ││                                                                     │
╰───────────────────────────────╯╰─────────────────────────────────────────────────────────────────────╯
 ↑↓ move →← open/close ⏎ run s slurm a args n dry-run / search ⇥ pane ? help
```

## Install

```sh
cargo install --path .
```

Requires `just` on `PATH` — just-tui reads `just --dump --dump-format json` and the
justfiles that dump points at.

## Use

```sh
just-tui                 # browse the justfile found from the current directory
just-tui ../other        # browse another project
just-tui -f build.just
just-tui --list          # plain listing, no TUI
just-tui --no-global     # project recipes only
just-tui --global-dir ~/recipes
```

## Global recipes

Every `.just` file in `~/.justx/` becomes its own root in the explorer, marked `⌂`, below
the project. `just --global-justfile`'s file (`~/.config/just/justfile` or `~/.user.justfile`)
is picked up too, if you have one.

```
│▾ just-tui/                    ← the project, open by default
││ • build  Build the debug binary
│⌂ ▸ docker/                    ← ~/.justx/docker.just
│⌂ ▾ git/                       ← ~/.justx/git.just
││ • log ★  Show a compact log of the last commits
```

Searching matches the file name too, so `/git` narrows to the git library and `/gitlog`
finds that one recipe.

**Global recipes run in your current directory**, not in `~/.justx` — just-tui passes
`--working-directory`, so `git::log` acts on the repo you are standing in. They are browsed,
documented, searched, run and submitted exactly like project recipes.

Global libraries start collapsed, and just-tui still opens if there is no justfile here at
all — you get the global roots alone.

## Keys

| Key | Action |
| --- | --- |
| `j` `k` `↑` `↓` | move in the focused pane |
| `J` `K` | next / previous recipe, skipping modules |
| `h` `l` `←` `→` | collapse / expand a module |
| `Enter` `Space` | run a recipe, toggle a module, follow an alias |
| `r` | run the selected recipe |
| `a` | run it with extra arguments |
| `n` | dry run (`just --dry-run`) |
| `/` | fuzzy search names, literal search in docs |
| `Tab` `Shift-Tab` | cycle pane focus |
| `g` `G` `PgUp` `PgDn` `Ctrl-u` `Ctrl-d` | scroll |
| `e` `c` | expand / collapse every module |
| `p` | show or hide private recipes |
| `m` | fold the `[group(…)]` layer in or out |
| `v` | doc above code, or beside it |
| `w` | wrap long source lines |
| `f` | zoom the focused pane |
| `s` | submit the recipe to Slurm |
| `S` | browse Slurm jobs and read their logs |
| `H` | past submissions, to load old settings back |
| `y` | copy the recipe source (OSC 52, works over ssh) |
| `o` | open the justfile at that line in `$EDITOR` |
| `R` | reload |
| `?` | help |
| `q` | clear the search, then quit |

Running a recipe leaves the TUI, streams the output to the normal terminal, and returns on
any key — so interactive recipes keep working.

## Submitting to Slurm

`s` opens a submit form for the selected recipe. It submits the *recipe*, not a generated
script:

```
sbatch --chdir=<justfile dir> --job-name=… --output=… <your flags> --wrap "just demo::greet"
```

### Job names

The name is built from the recipe **and its arguments**, so two runs of one recipe stay
apart in `squeue` and in `logs/`:

| recipe | args | job name | log file |
| --- | --- | --- | --- |
| `build` | | `build` | `logs/build-%j.out` |
| `demo::greet` | `name=ada` | `demo-greet-name-ada` | `logs/demo/greet-name-ada-%j.out` |

The `name` field at the top of the form overrides that. Leave it empty and the name is
generated; type one and it is used verbatim (tidied into `a-safe-file-name`), for the job
and for its log.

Nothing is inferred from the justfile. Fields start empty unless a config file or a previous
run filled them in.

**Partitions, accounts and QoS are auto-detected.** On opening the form, just-tui reads
`scontrol show partition`, `sinfo` and `sacctmgr`, so `←`/`→` on those fields cycles through
what the cluster actually offers, with each partition's limits shown beside it
(`24 cpu/node · 89.8G /node · max 02:00:00`). Requests that exceed the selected partition are
flagged before you submit. Detection is skipped silently on a machine without Slurm — the
fields stay free text.

### One recipe over many inputs

`each` expands a recipe into one Slurm array task per input:

```ini
[level3::run::s01_msa]
each = data/level3/*.fna     # a glob…
args = {}                    # …and where each match lands

[level3::run::s02_refilter]
each = @arms.txt             # …or one line of arguments per row
```

`{}` works like `xargs -I{}`. Without it the value is appended, which is how `just` takes a
positional parameter. With `@file`, blank lines and `#` comments are skipped, so a run list
can be kept under version control and commented.

On submit the expansion is written to a **manifest** beside the logs — one line of arguments
per task — and a single array is submitted that reads it:

```
logs/level3/run/s01_msa-245df667.args     ← the manifest
  data/level3/focal_target.fna              task 0
  data/level3/nonfocal_cong.fna             task 1
  …

sbatch --array=0-36 --wrap 'just level3::run::s01_msa $(sed -n "$((SLURM_ARRAY_TASK_ID+1))p" logs/…/s01_msa-245df667.args)'
```

The form says what it will do before you commit to it — `expands 37 jobs · first:
data/level3/focal_target.fna` — and names the manifest it would write.

The manifest is named after **its own contents**, so resubmitting an unchanged expansion
writes the same file, and a changed one gets a new name rather than overwriting a manifest
an array is still reading its way through. Task order is sorted, so a task number means the
same thing every time.

Each task writes `logs/…/<recipe>-%A_%a.out`, and the job browser lists tasks separately and
shows each one's own line of the manifest, so `23474919_7` reads
`level3::run::s01_msa data/level3/nonfocal_cong.fna` rather than the template. Set `array`
to `%4` alongside `each` to cap how many run at once; a range typed there is ignored, since
the expansion sets it.

Two things to know: every task gets the **same** `--mem`, `--cpus` and `--time`, so this
suits work that is uniform across inputs; and rerunning a few failed tasks means resubmitting
the array with `--array=3,7,19` rather than pressing `s`.

### Logs

Always under `logs/` in the justfile's base directory, mirroring the module structure:

| recipe | log file |
| --- | --- |
| `build` | `logs/build-%j.out` |
| `demo::greet` | `logs/demo/greet-%j.out` |
| `level3::db::tree` | `logs/level3/db/tree-%j.out` |

Array jobs use `-%A_%a` instead of `-%j`. The directory is created before submitting.

## Groups

A recipe carrying `[group('x')]` is filed under a collapsible folder inside its module —
**next to** the modules, never instead of them, since that is what `just --list` does too:

```
│▾ project/                     ← the module
││ ▸ tools/                     ← a real `mod`, untouched
││ • default ★  Show every rec… ← ungrouped recipes stay put, above the folders
││ ▾ build  2 recipes           ← [group('build')]
││ │ • compile  Build the binary
││ │ • release  Optimised build
││ ▾ data  2 recipes            ← [group('data')]
││ │ • fetch  Download the inputs
││ │ • clean  Drop the cache
```

The layer only appears where it actually divides something: a module whose recipes fall into
two or more buckets (ungrouped counts as one). A module where everything shares a single
group, or none at all, is left exactly as it was.

Groups are display only — a namepath never contains one, so running, submitting and config
scopes are unaffected. `m` folds the layer away and back; groups expand, collapse, search and
navigate like any other folder.

## Looking at jobs

`S` opens the job browser: `squeue` for what is queued or running, `sacct` for what has
finished, merged into one list, newest first.

```
╭ Slurm jobs — 4 shown of 4 in the last 7 days ──────────────────────────────────────────╮
│ ▸ 4211       PENDING    greet-name-ada     0:00      Resources                         │
│   4210       RUNNING    nightly-run        00:12:33  qib-compute                       │
│   4190       FAILED     tree-force-1       00:00:35  exit 1                            │
│   4180       COMPLETED  build              00:03:50  ok                                │
│                                                                                        │
│   submitted 2026-09-08T22:00:00  ·  level3::db::tree force=1  ·  qib-compute · mem 128G │
│   $ sbatch --partition=qib-compute --mem=128G --wrap "just level3::db::tree force=1"    │
│╭ stderr — logs/level3/db/tree-force-1-4190.err ────────────────────────────────────────╮│
││ loading 4.2M rows                                                                     ││
││ slurmstepd: error: Exceeded job memory limit                                          ││
│╰───────────────────────────────────────────────────────────────────────────────────────╯│
╰ ↑↓ job · ⇥ stdout · ⏎ open log · u reuse settings · f all · d 7d · r reload · esc back ─╯
```

While the browser is open it **refreshes itself**: the clock on a running job ticks every
second, and every five seconds `squeue` and `sstat` are re-asked for states and usage. `p`
pauses that; `r` reloads everything including `sacct`.

A running job shows what it is **actually using** against what it reserved:

```
23474919  RUNNING   level3-run-s01_msa-nonfocal_tgt…  00:01:32   233G/1280G  18%   64c  93%
23474878  RUNNING   level3-run-s01_msa-nonfocal_cong…  06:39:02  1240G/1280G  97%   64c  98%
```

`MaxRSS` and `AveCPU` come from one `sstat` call for every running job at once, not one per
job. CPU % is CPU time consumed against CPU time reserved — `AveCPU / (elapsed × cpus)` —
so 100% means every allocated core was busy throughout.

Colours read as strain: memory is red near its limit, and **CPU is the other way round** —
red means allocated cores are idling, green means they are busy. Say the word if you want
CPU coloured like memory instead.

Finding a log can mean waiting on `scontrol` and walking a large `logs/` tree, so it happens
on a background thread. The pane spins while it works and the list stays live — arrow keys,
filters and the clock all keep going. One loader runs at a time: holding a cursor key down
would otherwise put a thread and a directory walk behind every row it passed over. Whatever
ends up selected is read once the current one comes back, and answers for a job the cursor
has already left are dropped.

The bottom pane is the **tail of the selected job's log**, `stderr` first, with anything that
looks like an error picked out in red. The log file is found by asking `scontrol` while the
job is still known to the controller, and otherwise by looking for a file ending in `-<job id>`
under the job's working directory — so jobs submitted outside just-tui are readable too.

| in the browser | |
| --- | --- |
| `↑` `↓` | move between jobs |
| `Tab` | switch between `stderr` and `stdout` |
| `Enter` `o` | open the whole log in `$PAGER` |
| `u` | load the settings this job ran with back into the submit form |
| `f` | filter: all / running / failed |
| `d` | how far back to look: 1 / 7 / 30 / 90 days |
| `s` | submit the same job again (only once it has stopped) |
| `x` | kill the job (`scancel`), after confirming |
| `X` | kill it and submit the same job again |
| `p` | pause or resume the five-second refresh |
| `r` | reload · `y` copy the log path · `PgUp`/`PgDn`/`g`/`G` scroll |

### Killing and rerunning

Three keys, all reading the settings back from the history record, so a job that needs
another attempt does not have to be retyped:

| | |
| --- | --- |
| `s` | submit the same job again — for one that has **stopped**: completed, failed, cancelled, out of memory |
| `x` | cancel the job |
| `X` | cancel it **and** submit the same job again — for one still queued or running |

`s` is refused while a job is still on the queue, since two copies of it is almost never
what was meant; `X` is the key for that. Either way the job comes back with a new id and is
recorded as its own submission.

Both ask first and take nothing but `y`:

```
╭ Kill this job and run it again? ─────────────────────────────╮
│  scancel 23474919   level3-run-s02_refilter-tgt_filt_rank    │
│                                                              │
│  then submit the same job again:                             │
│  $ sbatch --partition=qib-compute --cpus-per-task=64 --mem…  │
╰ y do it · any other key leaves the job alone ────────────────╯
```

`s` and `X` need a history record, so they only offer themselves for jobs just-tui
submitted. `x` works on anything Slurm lets you cancel.

## Old settings: `.just-tui-cluster-history`

Every submission is appended to `.just-tui-cluster-history` beside the justfile — the job id,
the time, the exact `sbatch` line, and every field the form held. Unlike
`.just-tui-cluster-state`, which only keeps the *last* run of each recipe and is overridden by
any config file, this file forgets nothing.

- `H` lists every past submission; `Enter` loads one back into the submit form.
- `F6` in the form lists only that recipe's past runs — and since the title already names the
  recipe, those rows show the **arguments** instead, which is what tells two runs apart.
- `u` in the job browser jumps from a job straight to the settings it ran with.

So a job that failed three weeks ago can be found, read, and resubmitted with one field
changed. The file is machine-written and gitignored; it is trimmed to the last 500 entries.

### Defaults: `.just-tui-cluster-config`

```ini
# defaults for every recipe in this scope
partition = qib-compute
time      = 48:00:00
cpus      = 32
mem       = 64G

[level3::db::tree]   # one recipe, by its full path
mem  = 128G
time = 96:00:00
```

Keys: `partition` (or `queue`), `account`, `qos`, `cpus`, `mem`, `time`, `nodes`, `gpus`
(or `gres`), `array`, `extra`, `args`, `each` (or `glob`, `inputs`). An empty or absent key means the flag is not passed.

Precedence, weakest first — **the nearer config always wins**, and every config beats the
automatically recorded state:

1. what that recipe was last submitted with (`.just-tui-cluster-state`)
2. `~/.just-tui-cluster-config`
3. `<justfile dir>/.just-tui-cluster-config`
4. each module directory's `.just-tui-cluster-config`, outermost first
5. a `[recipe::path]` section in any of the above
6. whatever you type in the form

So a module setting `partition = bigmem` overrides the project's `partition = general` for
every recipe in that module — and editing a config immediately overrides what you last
submitted with. The state file only fills in fields no config sets. The form shows where the
selected field's value came from, next to it.

### Editing configs from the TUI

| in the form | |
| --- | --- |
| `F2` | write the current values as **module** defaults |
| `F3` | write them as **project** defaults |
| `F4` | write them as a `[recipe]` section, for this recipe only |
| `F5` | pick a scope and open its config in `$EDITOR` (created from a template if absent) |
| `Del` | clear the selected field |

Saving or editing re-reads the files at once, so the form updates in place.

Submitting also records what was used in `.just-tui-cluster-state` (machine-written,
gitignored) so the next submit starts where you left off — but only for fields no config
file sets.

## What the panes show

**Explorer** — the justfile as a tree: the root, its `mod`s, recipes, and aliases, plus a
`⌂` root for each global recipe file. The
default recipe is starred, private recipes are hidden until `p`, and a search highlights the
characters it matched.

**Documentation** — the full comment block above the recipe (not just the last line, which
is all `just --list` gives you), parameters with their defaults and whether they are
variadic, dependencies split into those that run before and after the body, attributes,
settings and variables for modules, and the exact `just …` command to invoke it.

**Source** — the recipe as written in the justfile, with line numbers matching the file, and
highlighting for interpolations, strings, shell variables and comments. When a recipe cannot
be located in its file the body is rebuilt from just's own dump and marked
`reconstructed`.

## Development

```sh
just build      # cargo build
just test       # cargo test
just check      # fmt, clippy, test
just run ../any # browse another project's justfile
```

`--snapshot WxH` renders one frame as plain text and exits; `--keys` replays keystrokes
first (named keys in brackets: `--keys "s[right][down]12G"`). The README screenshot above is
generated that way.

### Layout

| | |
| --- | --- |
| `just.rs`, `model.rs` | run `just --dump`, parse its JSON |
| `source.rs` | find a recipe's real text in its justfile |
| `tree.rs` | the explorer tree, and the search filter |
| `highlight.rs`, `theme.rs` | justfile syntax colouring |
| `app/` | state, navigation, key handling |
| `ui/` | one module per pane, plus the overlays |
| `slurm/` | cluster detection, settings, `sbatch`, `squeue`/`sacct` |
| `config.rs`, `submit.rs` | `.just-tui-cluster-config`, the submit form |
| `history.rs` | `.just-tui-cluster-history`, every submission ever made |
| `terminal.rs`, `snapshot.rs` | the real terminal, and rendering without one |
# just-tui
