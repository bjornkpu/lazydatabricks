# lazydatabricks

A lazygit-style terminal UI for Databricks jobs and pipelines. Numbered side panels, a tabbed
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

Needs Rust stable and the [Databricks CLI](https://docs.databricks.com/dev-tools/cli/) logged in
to a profile in `~/.databrickscfg`. Authentication is delegated to the CLI, so OAuth, Azure AD,
PATs and keyring storage all work without configuration here.

```
databricks auth login -p dev
cargo install --path .
lazydatabricks -p dev
```

Read-only by default. `--allow-actions` (or `allow_actions = true` in config) enables run-now and
cancel in the `x` menu, each behind a confirmation that names the target.

## Keys

`?` shows the bindings for the focused panel. The defaults follow lazygit:

| Key | Action |
|---|---|
| `0`–`3`, `Tab` | focus a panel |
| `j`/`k`, `g`/`G` | move the cursor |
| `h`/`l`, `[`/`]` | switch main-panel tabs |
| `/` | filter by name; `Enter` keeps it, `Esc` clears it |
| `m` | toggle mine only (by `dev` tag or creator) |
| `x` | actions menu for the selected job |
| `r` / `R` | refresh the focused panel / everything |
| `s` | cycle sort: activity, name, created |
| `o` / `y` | open in browser / copy URL |
| `+` | cycle screen mode: normal, half, full |
| `@` | toggle the API log |
| `q` | quit |

## Config

Optional, every field has a default. The Profile tab shows the path; on Windows it is
`%APPDATA%\lazydatabricks\config\config.toml`, elsewhere `~/.config/lazydatabricks/config.toml`.

```toml
profile = "dev"            # env DATABRICKS_CONFIG_PROFILE and --profile win over this
mine_only = true
filter = "gold"
dev_tag = "bjorn_punsvik"  # tag value that marks a job as mine; derived from the email if unset
allow_actions = false
theme = "dark"             # or "light"
sort = "activity"          # or "name", "created"; newest first, ties by name
max_jobs = 200
jobs_ttl_secs = 300        # background refresh interval
runs_ttl_secs = 120

[keys]                     # each list replaces that action's defaults
next_tab = ["l", "ø", "right"]
prev_tab = ["h", "æ", "left"]
```

Actions: `quit`, `screen_mode`, `toggle_log`, `filter`, `mine_only`, `refresh`, `refresh_all`,
`next_panel`, `open`, `back`, `down`, `up`, `first`, `last`, `next_tab`, `prev_tab`, `menu`,
`help`, `browse`, `copy`. Key names: one character, or `tab`, `up`, `down`, `left`, `right`,
`enter`, `esc`, `backspace`, `ctrl+c`.

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
