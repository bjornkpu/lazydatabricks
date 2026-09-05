# lazydatabricks

A lazygit-style terminal UI for Databricks jobs, pipelines and compute (clusters and SQL
warehouses; the panel hides itself in a workspace that has neither). Numbered side panels, a tabbed
main panel, an API log, and a `mine only` filter that turns a shared workspace's ninety jobs into
your twenty.

```
┌─[1]─Status──────────────┐┌─[0]─Runs - Detail─────────────────────────────────┐
│✓ dev → adb-1.azuredatabr││Run ID           Started      Duration  Result     │
│mine only · 27 of 90     ││50851892761075   09/01 10:08  -         ◐ RUNNING  │
└─────────────────────────┘│50851892761073   08/31 10:08  1m12s     ✓ SUCCESS  │
┌─[2]─Jobs────────────────┐│50851892761074   08/30 10:08  57s       ✗ FAILED   │
│[bk] okonomi_gold        ││                                                   │
│[bk] ems_gold_v1         │└───────────────────────────────────────────────────┘
└───────────────────1 of 27┘┌─API log──────────────────────────────────────────┐
┌─[3]─Pipelines───────────┐│GET  /api/2.2/jobs/list?limit=25         200  84ms│
│◐ [bk] aktorer_ingest    ││GET  /api/2.2/jobs/runs/list?job_id=1&l… 200 131ms│
└───────────────────1 of 12┘└───────────────────────────────────────────────────┘
 Select: j/k │ Filter: / │ Mine: m │ Actions: x │ Quit: q │ Keys: ?         v0.1.0
```

## Install

Download the binary for your platform from the
[releases page](https://github.com/bjornkpu/lazydatabricks/releases) and put it on your `PATH`,
or build it with Rust stable: `cargo install --path .`.

You also need the [Databricks CLI](https://docs.databricks.com/dev-tools/cli/) logged in to a
profile in `~/.databrickscfg`. Authentication is delegated to the CLI, so OAuth, Azure AD, PATs
and keyring storage all work without configuration here. The first run without a profile prints
the exact login command.

```
databricks auth login --host https://<workspace-url> -p dev
lazydatabricks -p dev
lazydatabricks -p dev,prod    # several workspaces in one window; p cycles them
```

Read-only by default. `--allow-actions` (or `allow_actions = true` in config) enables run-now,
run with parameters, repair and cancel in the `x` menu, each behind a confirmation that names
the target. Opening a failed run shows the failing task's error and traceback.

## Keys

`?` shows the bindings for the focused panel. The defaults follow lazygit:

| Key | Action |
|---|---|
| `0`–`4`, `Tab` | focus a panel |
| `j`/`k`, `g`/`G` | move the cursor, or scroll a detail view in `[0]` |
| `ctrl+d`/`ctrl+u` | ten rows or lines down / up |
| `h`/`l`, `[`/`]` | switch main-panel tabs |
| `/` | filter by name or owner; `Enter` keeps it, `Esc` clears it |
| `m` | toggle mine only (`dev` tag, `[tag]` name prefix, creator, or `me_aliases`) |
| `f` | cycle status: all, failed only, active only |
| `x` | actions menu: run, run with parameters, pause/resume schedule, repair, cancel; start/stop pipeline; start/terminate cluster; start/stop warehouse |
| `r` / `R` | refresh the focused panel / everything |
| `s` | cycle sort: activity, name, created |
| `o` / `y` | open in browser (`$BROWSER` if set) / copy URL (`wl-copy` on Wayland) |
| `Y` | copy the focused panel's rows as text, for Teams |
| `A` | enable actions for this session (asks first); again to disable |
| `+` | cycle screen mode: normal, half, full |
| `@` | toggle the API log |
| `q` | quit |

## Config

Optional, every field has a default. The Profile tab shows the path; on Windows it is
`%APPDATA%\lazydatabricks\config\config.toml`, elsewhere `~/.config/lazydatabricks/config.toml`.
`--config <path>` or `LAZYDATABRICKS_CONFIG` points at a shared one instead.

```toml
profile = "dev"            # env DATABRICKS_CONFIG_PROFILE and --profile win over this
profiles = ["dev", "prod"] # open several; wins over profile
mine_only = true
filter = "gold"
status = "all"             # or "failed", "active"; f cycles it
compute = true             # false hides [4] Compute and never fetches clusters or warehouses
expand_focused = true      # the side panel in context is twice as tall as the others
dev_tag = "bjorn_punsvik"  # tag value that marks a job as mine; derived from the email if unset
me_aliases = ["sp-1234"]   # service principals whose jobs count as mine
allow_actions = false
theme = "dark"             # or "light", "mono"; NO_COLOR in the environment also picks mono
sort = "activity"          # or "name", "created"; newest first, ties by name
date_format = "%d.%m %H:%M" # strftime, checked at startup
max_jobs = 200
jobs_ttl_secs = 300        # background refresh interval
runs_ttl_secs = 120

[keys]                     # each list replaces that action's defaults
next_tab = ["l", "ø", "right"]
prev_tab = ["h", "æ", "left"]

[[commands]]               # your own shell lines; see Custom commands below
name = "Job JSON"
key = "J"                  # optional; must not be bound to anything else
context = "jobs"           # jobs, runs, pipelines, compute or any (default)
command = "databricks jobs get {{job_id}} -p {{profile}}"
output = "popup"           # or "terminal": the TUI steps aside until you press Enter
confirm = false
```

## Custom commands

Anything the Databricks CLI can do, one key away, lazygit style. Each `[[commands]]` entry
shows up at the end of the `x` menu when its `context` applies, and fires directly on its `key`.
Placeholders are filled from the selection: `{{host}}` and `{{profile}}` always; `{{job_id}}`,
`{{name}}` and `{{url}}` for a job, plus `{{run_id}}` when a run is under the cursor in `[0]`;
`{{pipeline_id}}` and `{{cluster_id}}` for those panels. A command whose placeholders have
nothing to fill them stays out of the menu. `output = "popup"` captures stdout and stderr into
an overlay you can scroll and copy with `y`; `output = "terminal"` leaves the TUI so the command
owns the screen, for anything interactive or long. Lines run through `cmd /C` on Windows and
`sh -c` elsewhere. Custom commands do not need `allow_actions`: writing one into config is the
opt-in.

Actions: `quit`, `screen_mode`, `toggle_log`, `filter`, `mine_only`, `status_filter`,
`refresh`, `refresh_all`, `next_panel`, `open`, `back`, `down`, `up`, `page_down`, `page_up`,
`first`, `last`, `next_tab`, `prev_tab`, `menu`, `help`, `browse`, `copy`, `sort`. Key names:
one character, `ctrl+` and a character, or `tab`, `up`, `down`, `left`, `right`, `enter`,
`esc`, `backspace`.

## Scripting

`lazydatabricks jobs`, `lazydatabricks runs <job_id>` and `lazydatabricks pipelines` print the
listing as JSON and exit, so `lazydatabricks -p prod jobs | jq '.[].settings.name'` works.

## Debugging

`LAZYDATABRICKS_LOG=debug lazydatabricks` writes every request, response status and executed
command to `lazydatabricks.log` next to the config file. Accepts any `tracing` filter, such as
`lazydatabricks::api=trace`. Nothing is ever logged to the terminal.

## Development

```
cargo nextest run           # tests, including insta snapshots of rendered screens
cargo clippy --all-targets  # pedantic + nursery, denied; this is the compile gate
cargo fmt --check
bacon                       # watch mode; n for nextest, c for clippy
```

Elm-style: `Message` in, `App::update` folds it and returns `Command`s, `ui::draw(&App)` renders.
Nothing in `App` or `ui` does IO, so every behaviour is a state test or a text snapshot under
`src/ui/snapshots/`. `CLAUDE.md` and `docs/spec.md` hold the conventions and the design.
