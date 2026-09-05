# lazydatabricks — design spec

A keyboard-first TUI for Databricks ops, in Rust.

Status: design approved, not yet implemented.
Reference implementation: the Python `lazydatabricks` (PyPI 1.0.0), used for information
architecture only. Its three known defects are designed around here rather than inherited.

---

## 1. Purpose

One screen that answers "what is my data platform doing right now", driven entirely from the
keyboard, as a single binary with no Python runtime.

Two goals, in this order:

1. **Learn Rust TUI development properly.** Milestones are shaped so each one introduces one
   new concept, and each ends at something runnable.
2. **Replace the Python tool for daily use.** Reached at milestone 5, because that is where
   filtering lands — the thing the Python tool never had.

### Non-goals

- Not a Databricks admin console. No user management, no permissions, no billing.
- Not a notebook editor or a SQL client.
- No web UI, no daemon, no telemetry.
- Not multi-workspace-at-once. One profile per process; switch by restarting.

---

## 2. Constraints this repo already imposes

`Cargo.toml` denies `clippy::pedantic` and `clippy::nursery` wholesale, plus a panic-hostile
list: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `arithmetic_side_effects`,
`as_conversions`, `unreachable`, `unimplemented`, `todo`, `string_slice`, `panic_in_result_fn`,
`exit`, `unchecked_time_subtraction`. `clippy.toml` relaxes the panic family in tests only.

This is a strict policy. It is also the single biggest influence on the code you will write,
because ordinary TUI code violates three of these constantly. Budget for it in milestones 0–2;
it stops being friction once the patterns below are habit.

**Be aware of the tradeoff.** `arithmetic_side_effects` and `as_conversions` are the two that
will slow you most, because ratatui's geometry is `u16` and list indices are `usize`, so every
layout calculation is a fallible conversion. If the policy stops teaching and starts obstructing,
relax those two specifically rather than dropping pedantic — but try it strict first.

### Patterns that satisfy it

Indexing — `indexing_slicing` forbids `items[i]`:

```rust
// no
let job = &self.jobs[selected];
// yes
let Some(job) = self.jobs.get(selected) else { return };
```

Arithmetic — `arithmetic_side_effects` forbids bare `+` and `-` on integers. All cursor movement
goes through saturating or checked operations:

```rust
fn select_next(&mut self) {
    let last = self.jobs.len().saturating_sub(1);
    self.selected = self.selected.saturating_add(1).min(last);
}

fn select_prev(&mut self) {
    self.selected = self.selected.saturating_sub(1);
}
```

Conversions — `as_conversions` forbids `x as u16`. Convert fallibly, with a deliberate fallback:

```rust
fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)   // unwrap_or is allowed; unwrap is not
}
```

Strings — `string_slice` forbids `&s[..n]`, which would panic mid-codepoint anyway:

```rust
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
```

Time — `unchecked_time_subtraction` forbids `Instant - Instant`:

```rust
let age = Instant::now().saturating_duration_since(self.fetched_at);
```

Stubbing — `todo` and `unimplemented` are denied, so you cannot scaffold with `todo!()`. Return a
real error variant instead, or do not write the function yet.

Terminal restore — `Drop` must not panic, and `panic` is denied anyway, so the guard swallows
errors deliberately:

```rust
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
```

---

## 3. Architecture

**Elm-style message loop.** One `App` owns all state. A `Message` enum describes every possible
change. Async tasks never touch `App`; they send `Message`s down an mpsc channel. Rendering is a
pure function of `&App`.

Chosen because it fits ratatui's immediate-mode rendering exactly, and because it puts all
mutation in one place — which is where Rust's ownership rules are pleasant rather than painful.

```
┌──────────────┐  crossterm events   ┌─────────────┐
│  event task  │────────────────────▶│             │
└──────────────┘                     │             │
┌──────────────┐  ApiResult(..)      │  App state  │──▶ draw(&App, &mut Frame)
│  api tasks   │────────────────────▶│             │
└──────────────┘                     │             │
       ▲                             └─────────────┘
       └──── spawned by update() ──────────┘
```

The loop:

```rust
loop {
    terminal.draw(|f| ui::draw(&app, f))?;
    let Some(msg) = rx.recv().await else { break };
    match app.update(msg) {
        Flow::Continue => {}
        Flow::Quit => break,
    }
}
```

`update` returns a `Flow` rather than calling `exit` — `clippy::exit` is denied, and returning
cleanly means `Drop` restores the terminal.

### Screen layout — the lazygit shell

Adopt lazygit's and lazydocker's shell wholesale. It is a proven layout, users of either tool
already know the keys, and it removes a pile of design decisions.

Four elements, all of them load-bearing:

1. **Numbered side panels**, stacked in the left column. Title shows its number: `[2]─Jobs`.
   The focused panel's title is accented; the rest are dim. Each shows `n of m` bottom-right.
2. **One main panel** on the right, numbered `[0]`, with **tabs** that are views onto whatever is
   selected in the focused side panel. This is lazydocker's model, and it is what makes the layout
   scale: adding a view means adding a tab, not a pane.
3. **API log** bottom-right — every REST call with status and timing. This is lazygit's command
   log, and it is the single most useful thing you can have while learning an unfamiliar API.
   Toggle with `@`.
4. **Contextual hint bar** at the bottom, showing only the actions valid for the focused panel,
   ending in `Keybindings: ?`. Never a static list.

```
┌─[1]─Status────────────────┐┌─[0]─Runs - Detail - Tasks - Config ───────────┐
│ ✓ dev → adb-7405…         ││ Run ID      Started       Duration   Result   │
│ mine only · 22 of 85      ││ 5085189276  09/04 04:00   1m12s      SUCCESS  │
└───────────────────────────┘│ 3073792914  09/03 13:33   58s        SUCCESS  │
┌─[2]─Jobs──────────────────┐│ 7900024692  09/02 04:00   1m04s      FAILED   │
│ 2m  okonomi_gold        ✓ ││                                               │
│ 4h  aktorer_ingest      ✗ ││                                               │
│ 1d  ems_ingest          ✓ ││                                               │
│                 1 of 22 ──┘│                                               │
┌─[3]─Pipelines─────────────┐│                                               │
│ 3h  felles_gold         ✓ ││                                               │
│                 1 of 20 ──┘└───────────────────────────────────────────────┘
└───────────────────────────┘┌─API log───────────────────────────────────────┐
                             │ GET /api/2.2/jobs/list?limit=25    200  84ms  │
                             │ GET /api/2.2/jobs/runs/list?job…   200 131ms  │
                             └───────────────────────────────────────────────┘
 Actions: x │ Filter: / │ Mine: m │ Keybindings: ?                     v0.1.0
```

Details worth copying exactly, because they are why the layout feels good:

- **Relative age in the leftmost column** (`2m`, `4h`, `1d`, `1w`, `1M`) — dense and scannable in
  a way absolute timestamps are not. Absolute times belong in the detail tab.
- **Status as a glyph, not a word.** `✓` green, `✗` red, `◐` yellow for running. Colour carries
  the state; the glyph makes it work when colour does not.
- **The focused panel keeps its selection highlighted when unfocused**, dimmed. Losing the cursor
  on `Tab` is disorienting.
- **Screen modes.** `+` cycles the focused panel through normal → half → full screen. Cheap to
  implement, and the thing you will reach for constantly on a laptop display.

### Panel model

The side panel set is data, not hardcoded layout:

```rust
enum Panel { Status, Jobs, Pipelines }        // 1, 2, 3 — reserve 4+ for §10

struct MainTabs { tabs: Vec<Tab>, active: usize }
```

Tabs available in `[0]` depend on the focused side panel:

| Focused panel | Main panel tabs |
|---|---|
| Status | Profile, Config |
| Jobs | Runs, Detail, Tasks, Config |
| Pipelines | Updates, Detail, Config |

Rendering stays a pure function of `App`; the panel enum just selects which draw function runs.
Because tabs are a `Vec`, adding clusters later is a `Panel` variant plus a tab list — no layout
surgery.

### Module layout

```
src/
  main.rs           wiring only: guard, channels, loop
  app/
    mod.rs          App struct, update(), Flow
    message.rs      Message enum
    focus.rs        which pane has focus, drill-down state
  ui/
    mod.rs          draw(&App, &mut Frame) — splits the frame, dispatches
    chrome.rs       panel borders, numbered titles, "n of m" counters
    hints.rs        contextual bottom bar
    apilog.rs       API log panel
    side/
      status.rs     [1]
      jobs.rs       [2]
      pipelines.rs  [3]
    main/
      runs.rs       [0] Runs tab
      detail.rs     [0] Detail tab
      tasks.rs      [0] Tasks tab
      config.rs     [0] Config tab
    help.rs         keybindings overlay
    theme.rs        colors, glyphs, relative-time formatting
  api/
    mod.rs          Client: bearer token + reqwest, pagination
    auth.rs         token from the databricks CLI
    jobs.rs         list_jobs, list_runs, get_run
    pipelines.rs    list_pipelines
    models.rs       serde structs mirroring the REST shapes
  config.rs         TOML config, profiles, defaults
  error.rs          AppError, thiserror
```

Keep files small enough to hold in your head. When `ui/jobs.rs` grows past a few hundred lines it
is doing more than drawing a list.

---

## 4. Data model

Shapes below were verified against a live workspace, not taken from docs. Deserialize only the
fields you use; add `#[serde(default)]` liberally, because Databricks omits empty fields rather
than nulling them.

`GET /api/2.2/jobs/list?limit=25`

```json
{
  "jobs": [{
    "job_id": 1025322370191789,
    "creator_user_name": "bjorn.punsvik@enova.no",
    "run_as_user_name": "bjorn.punsvik@enova.no",
    "settings": {
      "name": "[bjorn_punsvik] okonomi_gold",
      "timeout_seconds": 7200,
      "max_concurrent_runs": 4,
      "tags": { "dev": "bjorn_punsvik", "domain": "okonomi" },
      "format": "MULTI_TASK"
    }
  }],
  "next_page_token": "CAIo0JeenYM0Sg80MzEwMTU5NzM2MjUyMDA="
}
```

`GET /api/2.2/jobs/runs/list?job_id=<id>&limit=25`

```json
{
  "runs": [{
    "job_id": 1025322370191789,
    "run_id": 50851892761073,
    "creator_user_name": "bjorn.punsvik@enova.no",
    "state": {
      "life_cycle_state": "TERMINATED",
      "result_state": "SUCCESS",
      "state_message": "",
      "user_cancelled_or_timedout": false
    },
    "start_time": 1788170893271,
    "end_time": 1788170925431,
    "execution_duration": 0
  }]
}
```

Note: `job_id` and `run_id` are `i64`, and indices are `usize`. Under `as_conversions` every
crossing is a `try_from`. Keep ids as `i64` end to end and never use them as indices.

Timestamps are **epoch milliseconds**, not seconds. Convert once, at the API boundary, into
`jiff::Zoned` (jiff chosen over chrono, see Cargo.toml); never let raw millis reach the UI layer.

`life_cycle_state` and `result_state` are closed sets in practice but open in the API. Model them
as enums with a `#[serde(other)] Unknown` variant so a new Databricks state cannot break parsing.

### Verified endpoints

| Purpose | Endpoint | Response root |
|---|---|---|
| Jobs | `GET /api/2.2/jobs/list` | `jobs[]`, `next_page_token` |
| Runs | `GET /api/2.2/jobs/runs/list?job_id=` | `runs[]` |
| Run detail | `GET /api/2.2/jobs/runs/get?run_id=` | run object |
| Pipelines | `GET /api/2.0/pipelines?max_results=` | `statuses[]` |
| Clusters | `GET /api/2.1/clusters/list` | `clusters[]`, `next_page_token` |
| Warehouses | `GET /api/2.0/sql/warehouses` | `warehouses[]` |
| Trigger (M9) | `POST /api/2.2/jobs/run-now` | `run_id` |
| Cancel (M9) | `POST /api/2.2/jobs/runs/cancel` | `{}` |

There is **no official Databricks Rust SDK** — no `databricks` or `databricks-sdk` crate exists.
Hand-roll these with reqwest and serde. Six endpoints is not a burden, and it removes any risk of
the dependency drift that broke the Python tool.

### Pagination

`limit` is a **page size, not a cap**. The Python tool got this wrong and silently fetched the
entire workspace on every refresh. Follow `next_page_token` explicitly, and stop at a configured
maximum:

```rust
async fn list_all_jobs(&self, max: usize) -> Result<Vec<Job>, AppError> { /* loop on token */ }
```

---

## 5. Filtering

The reason this project beats the Python tool, so design it in rather than bolting it on.

The workspace holds 85 jobs and 77 pipelines across roughly five developers, who share base names
(`aktorer_ingest`, `ems_ingest`, …). An unfiltered list is unusable — not because of duplicates,
but because five people's identically-named jobs interleave.

Three filter sources, in order of quality:

1. **Tag** — `settings.tags.dev == "bjorn_punsvik"`. The real signal, since the platform already
   tags by developer. Prefer this.
2. **Creator** — `creator_user_name == me`. Correct for ownership, and available on runs too.
   Resolve `me` once at startup from `GET /api/2.0/preview/scim/v2/Me`.
3. **Name substring** — a fallback for ad-hoc searching.

**Do not send `name` to the jobs API.** It is an exact, case-insensitive match, not a substring
filter, despite what the Python tool's docstring claims — passing a prefix returns zero rows.
Filter jobs client-side; 85 rows makes that free.

Pipelines are different: `GET /api/2.0/pipelines` accepts `filter=name LIKE '%pattern%'` with real
wildcard support, so filter those server-side.

Filter state lives in `App`, is applied in a single method, and is reflected in the status bar so
it is never a mystery why a list looks short:

```rust
fn visible_jobs(&self) -> impl Iterator<Item = &Job> {
    self.jobs.iter().filter(|j| self.filter.matches(j))
}
```

---

## 6. Authentication

Shell out to the Databricks CLI:

```rust
let out = Command::new("databricks")
    .args(["auth", "token", "-p", profile])
    .output()?;
let token: TokenResponse = serde_json::from_slice(&out.stdout)?;
```

Returns `{ access_token, token_type, expiry, expires_in: 3600 }`.

This works with OAuth U2M, Azure AD, PATs, and keyring-backed storage — because the CLI already
solved it. Twenty lines instead of an OAuth subsystem, and it sidesteps exactly the bug that made
the Python tool unusable: it hard-guarded on a literal PAT and rejected `auth_type = databricks-cli`
profiles outright.

**Tokens expire in 3600s**, so mint on startup and refresh when `expiry` is within 5 minutes.
Never cache to disk — the CLI's keyring is the source of truth.

Read the workspace host from `~/.databrickscfg` for the selected profile. If the CLI is missing,
fail with a clear message naming the install command — do not fall back to prompting for a token.

---

## 7. Keymap

Take lazygit's and lazydocker's bindings as-is wherever an equivalent action exists. Anyone who
uses either tool should be able to drive this without reading anything, and muscle memory is worth
more than any improvement you might invent. `?` is always the authority, and it is contextual.

**Navigation — identical to lazygit:**

| Key | Action |
|---|---|
| `1`–`3` | focus side panel by number |
| `0` | focus the main panel |
| `Tab` | next panel |
| `j` / `k`, `↓` / `↑` | move selection |
| `h` / `l`, `←` / `→` | previous / next tab in the main panel |
| `[` / `]` | previous / next tab (lazygit's alternate binding; support both) |
| `g` / `G` | first / last item |
| `ctrl+d` / `ctrl+u` | page down / up |
| `Enter` | focus the main panel on this item |
| `Esc` | back up one level, or close an overlay |
| `+` | cycle screen mode: normal → half → full |
| `@` | toggle the API log |
| `?` | contextual keybindings overlay |
| `q`, `ctrl+c` | quit |

**Actions:**

| Key | Action |
|---|---|
| `/` | incremental filter on the focused panel |
| `m` | toggle "mine only" |
| `r` | refresh the focused panel |
| `R` | refresh everything |
| `x` | open the context menu for the selected item (lazygit's convention) |
| `o` | open the item in the browser |
| `y` | copy the item's URL or id |

Milestone 9 puts triggering and cancelling **inside the `x` menu** rather than on bare keys. This
is deliberate and follows lazygit: destructive or expensive actions live behind a menu that names
the target, so they cannot be fired by a stray keypress. A bare `x` opening a menu is safe; a bare
`x` triggering a job run is not.

**Deliberate divergences from lazygit**, and why:

- `m` is "mine only" here; in lazygit it is merge. There is nothing to merge, and ownership
  filtering is this tool's most-used action.
- `r` / `R` are refresh; lazygit has no direct equivalent worth preserving.
- lazygit's `p`/`P` (pull/push) and `c` (commit) have no analogue and stay unbound, so they remain
  available later without breaking anyone's habits.

Every binding is overridable from config at M7 — lazygit allows this and it matters for
non-US keyboards, where `[` and `]` need AltGr on a Norwegian layout.

---

## 8. Milestones

Each is independently runnable and independently useful. Do not start the next until the current
one builds clean under `cargo clippy --all-targets` and `cargo nextest run` is green.

### M0 — Terminal skeleton
Opens an alternate screen, draws a bordered box, quits on `q`, restores the terminal even on
panic. No network, no state.

*Teaches:* RAII and `Drop`, `anyhow` at the boundary, why `exit` is denied, `TestBackend`.
*Done when:* a forced panic still leaves your shell usable, and one insta snapshot of the box passes.
*Add:* `ratatui 0.30`, `crossterm 0.29`, `anyhow`; dev: `insta`.

### M1 — Your job list, blocking
Mint a token via the CLI, `GET /api/2.2/jobs/list`, render names in a list. UI freezes during the
fetch; that is expected and gets fixed in M2.

*Teaches:* `serde` derive, `?` propagation, `std::process::Command`, `Option` handling.
*Done when:* your real job names appear.
*Add:* `reqwest 0.13` (blocking), `serde`, `serde_json`.

### M2 — Async, non-blocking
Introduce tokio. Fetches move to spawned tasks that send `Message::JobsLoaded(..)` down an mpsc
channel. A spinner renders while loading. This is the milestone that establishes the architecture.

*Teaches:* `async`/`await`, mpsc channels, `Send`, why state lives in one place.
*Done when:* the UI still responds to `q` mid-fetch.
*Add:* `tokio` (`rt-multi-thread`, `macros`, `sync`), reqwest async.

### M3 — The lazygit shell
The chrome, with real data in one panel only. Numbered side panels `[1]` Status, `[2]` Jobs,
`[3]` Pipelines. Focus via `1`–`3` and `Tab`, accented title on the focused panel, `n of m`
counter bottom-right of each, contextual hint bar along the bottom, `+` for screen modes. The
main panel `[0]` renders a placeholder for now.

Do the chrome before the content. It is the part that makes every later milestone feel finished,
and getting `Constraint`-based layout wrong is much easier to see with placeholders than with real
tables. This is also the milestone where `as_conversions` bites hardest — layout maths is all
`u16`, selection is all `usize`.

*Teaches:* `Constraint` layout, enums as state machines, `Option<usize>` selection, fallible
integer conversion at the UI boundary.
*Done when:* `1`/`2`/`3`/`Tab` move focus, counters are right, and the hint bar changes per panel.

### M4 — Main panel tabs and the API log
`[0]` gets tabs driven by the focused side panel — Jobs shows Runs / Detail / Tasks / Config.
`h`/`l` and `[`/`]` switch them, `Enter` from a side panel focuses `[0]`. Add the API log panel
with per-request method, path, status and duration, toggled by `@`.

The API log is worth building this early: it makes every subsequent milestone debuggable, and it
teaches you the Databricks API faster than reading its docs.

*Teaches:* `Vec`-driven dynamic UI, timing with `Instant`, threading a log channel through async
tasks without shared mutable state.
*Done when:* selecting a job and pressing `l` walks its tabs, and `@` shows the calls that filled
them.

### M5 — Filter and search
`/` for incremental name filter, `m` for mine-only via tag then creator. Status bar shows the
active filter and the visible/total count.

*Teaches:* iterators and closures, `&str` vs `String`, borrow lifetimes in filter chains.
*Done when:* 85 jobs become your 22. **This is where it replaces the Python tool.**
*Add:* `tui-input 0.15`.

### M6 — Cache and refresh
TTL cache per resource. Background refresh on an interval. Never refetch on every keystroke.
Status bar shows data age.

*Teaches:* `Instant`/`Duration`, `saturating_duration_since`, `Arc` for shared read-only data.
*Done when:* navigating for a minute produces no new API calls.

### M7 — Config
`~/.config/lazydatabricks/config.toml`: default profile, default filter, poll interval, theme,
key overrides. Every field optional with a sane default. Key overrides matter on a Norwegian
layout, where lazygit's `[` and `]` need AltGr.

*Teaches:* `serde` defaults, layered config, `Option` vs default semantics.
*Done when:* deleting the config file changes nothing about startup.
*Add:* `toml`, `directories`.

### M8 — Errors as UI
An `AppError` enum via `thiserror`. Every failure renders in a status line or error pane. Auth
failure, expired token, network timeout, and malformed JSON each get a distinct, actionable
message. Nothing panics, ever.

*Teaches:* `thiserror`, error taxonomy, `Display` for humans vs `Debug` for logs.
*Done when:* revoking your token mid-session shows a real message instead of dying.
*Add:* `thiserror`.

### M9 — Actions behind the `x` menu
`x` opens a context menu for the selected item, lazygit-style, listing the actions valid for it —
trigger a run, cancel a run. The menu names the target explicitly, and destructive entries ask for
confirmation on top. An `Action` trait unifies the entries. Read-only stays the default; actions
need `--allow-actions` or a config opt-in.

No action is ever on a bare key. A stray `x` opens a menu, which is harmless; a stray `x` that
triggers a production job is not.

*Teaches:* trait objects and `dyn`, POST bodies, optimistic UI update then reconcile.
*Done when:* triggering a job takes at least two deliberate keystrokes and shows you its name
first.
*Add:* `clap` (`derive`) — the first milestone that needs real argument parsing, for
`--profile`, `--filter`, and `--allow-actions`.

### M10 — Polish
Contextual `?` overlay listing only the focused panel's bindings, lazygit-style. Themes. A real
`--help`. Snapshot and state tests already exist from M0 onward (see CLAUDE.md); this milestone
fills gaps, not the whole suite.

*Teaches:* snapshot review as a workflow at scale.
*Done when:* `cargo nextest run` catches a layout regression you introduce on purpose.

### M11 — Pipelines
`[3]` stops being chrome. `GET /api/2.0/pipelines` fills it, paged on `next_page_token` like
jobs. Each row is a health glyph and the name; the glyph comes from the newest entry in
`latest_updates` (`✓` completed, `✗` failed or cancelled, `◐` in progress) and falls back to the
pipeline's own `state`, with `·` for one that has never run. The main panel gets **Updates** and
**Detail** tabs for the pipeline in context. Updates come from the `latest_updates` the list call
already returns, so no second request per selection.

Filtering is client-side here too, despite §5. The server-side `filter=name LIKE` would mean one
request per keystroke, which M6 forbade, and eighty rows filter for free. "Mine" is the creator
alone: the list response carries no tags. One filter text applies to both lists, `/` edits it
from whichever list is in context, and the status line counts the list you are looking at
(`23 of 80 pipelines`). `r` on Status refreshes both lists.

Shapes worth knowing: `pipeline_id` and `update_id` are UUID strings, not `i64`, and
`creation_time` is RFC 3339, not epoch millis. jiff's `serde` feature parses it directly.

*Teaches:* a second resource on the same skeleton, string ids next to integer ids, two
timestamp encodings in one API.
*Done when:* your pipelines list matches `databricks pipelines list-pipelines`, and `m` cuts it
to yours.
*Not yet:* the `Config` tab. Start and stop landed in M14, the row age in M12.

### M12 — Age and result on job rows
The sketch's `2m okonomi_gold ✓` row, at last. `GET /api/2.2/jobs/runs/list` **without** `job_id`
returns the newest runs across the workspace, newest first, so a handful of pages gives every
active job its latest run in one sweep instead of one call per job. Per-job fetches (M4) refresh
the entry when they land. Jobs with no run in that window show `·` and no age; that is honest,
not a bug.

Ages need wall-clock time, and `update` does no IO, so the input thread sends
`Message::Clock(Timestamp)` with every tick. `App.now` is state like anything else and tests pin
it. The age column is one unit wide: `now`, `2m`, `4h`, `1d`, `1w`, `3M`.

*Teaches:* a second use of an endpoint you already had, time as a message.
*Done when:* your most recently run job shows a minutes-old age and a `✓`.

### M13 — Runs cursor and run detail
The main panel becomes a real third pane. With `[0]` focused, `j`/`k` move a cursor in the runs
table (`Load<Selectable<Run>>`, the same cursor type the side lists use), `Enter` opens the run via
`GET /api/2.2/jobs/runs/get?run_id=` with its state message, page URL and tasks, and `Esc` backs
out one level at a time: run, then panel. `x` in the main panel cancels the run under the cursor
only; from the side panel it still lists every active run. `o` and `y` use the run's URL when a
run is under the cursor.

`runs/get` carries `tasks[]` and `run_page_url`; list responses do not. One `Run` type with
`#[serde(default)]` on both covers it.

*Teaches:* drill-down as state (`viewing_run`), not as a new screen; one model for two shapes.
*Done when:* `0`, `j`, `Enter` shows the tasks of a real run and `Esc` twice puts you back on the
job list.

### M14 — Pipeline actions
`x` on a pipeline offers *Start update*, and *Stop* while an update is in progress
(`POST /api/2.0/pipelines/{id}/updates` and `/stop`). Same confirmation, same `--allow-actions`
gate, same optimistic update: the pipeline shows the new update queued, or its state as stopping,
until the refetch says otherwise.

*Teaches:* the menu is data, so a second resource is two more variants.
*Done when:* the confirmation names the pipeline and a read-only session refuses at Enter.

### M15 — Logs
`LAZYDATABRICKS_LOG=debug` writes `tracing` output to `lazydatabricks.log` next to the config
file through `tracing-appender`, never to stdout, which belongs to the UI. Every request logs
method, path, status and milliseconds; every executed `Command` logs; failed responses `warn!`
with the body. Any `tracing` filter works, such as `lazydatabricks::api=trace`.

*Teaches:* `tracing` with a non-blocking file writer, and why the guard must live until exit.
*Done when:* one session leaves a log you could debug an API problem from.

### M16 — Sort
Both lists were in API order: jobs newest-created first, pipelines by UUID, which is random. Now
`sort` in config and `s` at runtime pick **activity** (newest run or update first, never-run
items last), **name**, or **created**. Ties always break on name so the order is stable. Pipelines
have no creation time in the list response, so `created` is name order there. Lists re-sort when
a newer run arrives and the cursor follows the item by id, so the row under you never changes
because the order did. Panel titles say which order is on.

*Teaches:* `sort_by_cached_key` with `Reverse<Option<_>>` to put unknowns last.
*Done when:* `s` cycles the three orders and the title follows.

M17 to M21 come out of a persona review: eight imagined users (on-call engineer, platform admin,
junior analyst, lazygit power user, team lead, colour-blind user, SRE, ML engineer) walked the
snapshots and reported what they missed. Six of eight opened the browser for the same reason: the
run detail shows *which* task failed, never *why*.

### M17 — Task errors
Run detail gains the failing task's error. `runs/get` already carries `tasks[].run_id` and a
per-task `state.state_message`; for every task whose result is not `SUCCESS`, the app also calls
`GET /api/2.2/jobs/runs/get-output?run_id=<task run id>` and shows `error` and `error_trace`
under the task table, wrapped, newest task first. Outputs are cached by task run id for as long
as the run is open, so a refresh of the run does not refetch tracebacks that already arrived.
Fetch failures show inline in the same place, never as a notice.

*Teaches:* one message per remote value, keyed so late replies for a run you left are dropped.
*Done when:* a failed multi-task run shows the Python exception without leaving the terminal.

### M18 — Stale beats blank
A failed refresh used to blank the panel with the error and, because `jobs_fetched_at` stayed
stale, refetch on the very next tick: with the VPN down that is a request every 100 ms. Now a
failure counts as a fetch for TTL purposes, so the next attempt waits a whole TTL (or `r`), the
last good list stays on screen, and the error sits in the panel's bottom border. The error
replaces the list only when there was never a list. A stale runs cache keeps showing too. A 401
drops the cached token, so the next request after `databricks auth login` mints a fresh one
instead of resending the rejected bearer for up to 55 minutes.

*Teaches:* the TTL is the backoff. `ponytail:` no exponential backoff and no `Retry-After`;
one attempt per TTL is already gentler than any rate limit, and `r` is the manual retry.
*Done when:* pulling the network cable leaves the job list visible, one error line, one request
per TTL, and plugging it back in recovers on the next refresh.

### M19 — Status filter
`f` cycles **all**, **failed only**, **active only** for both lists, and `status` in config
starts there. Failed means the newest known run has a result other than `SUCCESS`; active means
it is still in an active life-cycle state. Pipelines use the latest update, else the pipeline
state, the same rule as the glyph. Data is already on screen, so no new endpoint. The status
line says `failed only · 3 of 90 jobs`, and the hint bar shows the key. `mine only` and the
text filter still apply on top.

*Teaches:* a filter is a predicate over what the app already knows.
*Done when:* the 08:30 check is `f` and a glance, not twelve pages of `j`.

### M20 — Repair and parameters
Two more `x` entries for jobs. **Repair run** appears when the run under the cursor (or the
newest run, from the side panel) ended in anything but `SUCCESS`, and sends
`POST /api/2.2/jobs/runs/repair` with `rerun_all_failed_tasks: true`; the run shows as pending
until the refetch. **Run with parameters** opens a one-line prompt for `key=value` pairs
separated by spaces, and `Enter` sends `run-now` with them as `job_parameters`; `Esc` cancels.
Typing the parameters is the confirmation, so there is no second `y`. Both sit behind the same
`--allow-actions` gate.

*Teaches:* the menu is data (M14) and the prompt is one more `InputMode`, not a widget library.
*Done when:* a failed task can be re-run alone, and a notebook can be started with `date=2026-09-01`
without opening the browser.

### M21 — Fit and finish
Small things every persona tripped on:

- `date_format` in config, `strftime` syntax, default `%d.%m %H:%M`. `09/01` read as January
  to every Norwegian in the room. Invalid formats fail at config load, not at render.
- Side-list names truncate with `…` instead of clipping silently, so `[team] nightly_gold` never
  masquerades as `[team] nightly_bronze`.
- `ctrl+d` / `ctrl+u` page the focused list, as §7 promised. Config keys accept any `ctrl+<x>`.
- Narrow main panels (half mode, small terminals) drop the Run ID column from the runs table
  and keep Result. Never truncate an identifier; drop the column instead.

*Teaches:* a config value validated at load is a config value that cannot crash a render.
*Done when:* a Norwegian reads the dates right, half mode still shows `✗ FAILED`, and `ctrl+d`
moves ten rows.

M22 to M31 finish the persona list. Everything the eight reviewers asked for is either here or
named in §10 with the reason it stays out.

### M22 — Live runs
A running run shows its elapsed time (`now - start_time`, ticking) instead of `-`, and while any
run on screen is active the runs table refetches every 5 s regardless of `runs_ttl_secs`, then
drops back to the TTL once everything is terminal. Pipeline rows get the same one-unit age as
jobs, from the latest update. Queued, pending, blocked and waiting runs get `◌` so a list can tell
"waiting for a cluster" from "running" without opening the job.

*Teaches:* the poll rate is state, derived from what is on screen.
*Done when:* a 40 s job goes `◌` to `◐` to `✓` in the table without a key press.

### M23 — Alerts
Every refresh diffs the newest run per job and the latest update per pipeline against what was
known. A transition into a failed state rings the terminal bell (`Command::Bell`) and puts
`✗ <name> failed` in the hint bar until the next key. The recent-runs sweep also asks for
`active_only=true`, so a structured-streaming run started three weeks ago still shows `◐` after
two hundred newer batch runs.

*Teaches:* change detection is a fold over two maps; the bell is one more `Command`.
*Done when:* leaving it open on a second monitor and a job dies, you hear it.

### M24 — Actions at runtime
`A` asks "Enable actions for this session?" and `y` flips `allow_actions` without a restart; `A`
again turns it off silently. On-call means the read-only session is the one that needs to cancel
something at 03:10.

*Teaches:* a confirmation is an `InputMode`, not a widget.
*Done when:* a read-only session can cancel a run after one `A` and one `y`.

### M25 — Ownership
The `/` filter also matches the creator and run-as names, so `/olav` is the leaver audit. Mine
matches, in order: the `dev` tag, a `[<tag>]` prefix on the name (what asset bundles write in
development mode), the creator, and any of `me_aliases` in config as creator or run-as, for jobs
deployed by a service principal. Pipelines use the same rule minus the tag.

*Teaches:* "mine" is a predicate with a config hook, not a hard-coded convention.
*Done when:* a bundle-deployed prod job with a service-principal creator shows under `m`.

### M26 — Copy the table
`Y` copies the focused panel's visible rows as plain text, one line per row, the same text the
screen shows. Paste it into Teams and standup is done.

*Teaches:* the render already produced the text; reuse it.
*Done when:* `Y` on the Jobs panel puts `1d ✗ nightly_bronze_ingest` lines on the clipboard.

### M27 — Platform polish
Small things that make the tool feel native on each desk:

- `o` honours `$BROWSER`; `y` uses `wl-copy` under Wayland (`WAYLAND_DISPLAY` set), else `xclip`.
- `NO_COLOR` or `theme = "mono"`: no colours, no `DIM`; focus is a double border, the cursor is
  reverse video. Glyphs already carry the state.
- Two actions bound to one key is a config error at load, not a coin toss at runtime.
- `?` scrolls with `j`/`k` when the list is taller than the terminal.
- The hint bar inside a run says `Back: Esc │ Browser: o`, not `Open: Enter`.
- No `~/.databrickscfg` or no such profile prints the exact `databricks auth login --host …`
  to run, not "file not found".
- `--config <path>` and `LAZYDATABRICKS_CONFIG` pick the config file, for a team-shared one.
- When `max_jobs` truncates, the status line says `200+ jobs (truncated)` instead of lying.

*Done when:* a Wayland user, a Windows analyst and a colour-blind reviewer all get past minute one.

### M28 — `--json`
`lazydatabricks jobs`, `lazydatabricks runs <job_id>` and `lazydatabricks pipelines` print the
same models the TUI holds as JSON and exit, for `fzf`, `jq` and scripts. Same auth and
`max_jobs`; no filters, `jq` is the filter; no terminal taken over.

*Teaches:* the API layer was already separate from the UI; this proves it.
*Done when:* `lazydatabricks jobs | jq '.[].settings.name'` works.

### M29 — Job detail and clusters
The Detail tab fetches `GET /api/2.2/jobs/get` for the selected job (debounced like runs) and
adds Schedule, Deployment (bundle path when `deployment.kind` is `BUNDLE`), Edit mode, and one
line per task with its type and notebook or file path, plus which cluster it runs on: `job
cluster` or `all-purpose <id>`. A fourth side panel `[4] Clusters` lists `GET
/api/2.1/clusters/list` with state and source, and `x` offers start and terminate behind the same
gate.

*Teaches:* the fourth panel is the third panel again, which is the point of the shape.
*Done when:* the ML engineer can start their interactive cluster and see which notebook a job
runs without the browser.

### M30 — Multi-profile
`-p dev,prod` or `profiles = [...]` in config opens every profile in one process; `p` cycles
them. Each profile keeps its own `App` and client; `main` tags every message with the profile it
came from, so a late reply for `dev` never lands in `prod`. Only the active profile receives keys
and ticks.

*Teaches:* `App` needed no change; the shell owns the plurality.
*Done when:* the platform admin watches four workspaces from one pane.

### M31 — Release
A GitHub Actions workflow builds Windows, macOS and Linux binaries on a tag and attaches them to
a release. The README's install section starts with the download, not with `cargo`.

*Done when:* an analyst with no Rust toolchain runs it.

### M32 — Compute
A serverless workspace has no clusters, and Databricks exposes no "warm serverless" state for
jobs or pipelines; the one serverless thing with a visible warm or cold state is a SQL warehouse.
So `[4]` becomes **Compute**: `clusters/list` plus `GET /api/2.0/sql/warehouses`, folded into
one row type, one glyph rule (`●` running, `◐` starting or stopping, `·` stopped, `✗` error),
and `x` offering start and stop for warehouses like start and terminate for clusters. When both
lists come back empty the panel folds away, `4` and `Tab` skip it, and no config is needed.
Warehouses are shared infrastructure, so `mine only` keeps them; a serverless workspace whose
two warehouses were created by a colleague still shows them behind `m`. `compute = false` in
config turns the panel off outright, for people who never start compute from a terminal.

*Teaches:* a fold to one shape beats a second panel; absence is a state the layout can react to.
*Done when:* a serverless workspace shows its warehouses with their state and nothing else in
`[4]`, and a workspace with neither shows three side panels.

### M33 — Main panel scrolling
lazygit's main view scrolls; ours cut a traceback at the panel edge and pointed at the browser.
Every text view in `[0]` (run detail, job, pipeline and compute detail, profile) becomes one
paragraph that `j`/`k`, `ctrl+d`/`ctrl+u`, `g`/`G` scroll when `[0]` is focused; the runs table
keeps its cursor. Run detail folds its fields, task table and wrapped error output into that one
text, so a long trace scrolls instead of vanishing. `update` cannot know the terminal size, so
the scroll is unbounded in `App` and the draw returns how far the text really can go; `main`
feeds that back as `Message::ScrollLimit` and the clamp stays a state transition. Any change of
what the panel shows (side cursor, tab, entering or leaving a run) starts at the top.

*Teaches:* when the pure core needs a fact only the renderer has, the renderer reports it as a
message rather than the core guessing at layout.
*Done when:* `G` on a failed run lands on the last line of the trace and `k` moves up one line.

### M34 — Custom commands
lazygit's `customCommands`, and the reason its users never ask for more menu items. A
`[[commands]]` entry in config has a `name`, an optional `key`, a `context` (`jobs`, `runs`,
`pipelines`, `compute`, `any`), a `command` with `{{job_id}}`-style placeholders, an `output`
mode and a `confirm` flag. Matching entries end the `x` menu; a key fires one directly. Config
load rejects a key that a binding already owns. Placeholders come from the selection; an entry
that cannot be filled here stays out of the menu, and its key says why. `popup` captures the
output into a scrolling overlay (`y` copies it). `terminal` is the new plumbing: `main` parks
the input thread, leaves the alternate screen, runs the line with inherited stdio, waits for
Enter and re-enters, then feeds `ShellExited` back through `update` like any message. The shell
is `cmd /C` on Windows and `sh -c` elsewhere. Custom commands bypass `allow_actions`; writing one
is the opt-in.

*Teaches:* a program that can hand its terminal to another and take it back; the loop's own
messages (`ScrollLimit`, `ShellExited`) queue ahead of the channels instead of bypassing `update`.
*Done when:* `databricks jobs get {{job_id}}` on `J` shows the job's JSON in a popup, and a
`terminal` command gets the keyboard and returns on Enter.

### M35 — Expand the panel in context
lazygit's `expandFocusedSidePanel`. Four side panels on a 24-row terminal left each list four
rows. The list whose selection `[0]` shows now takes two height shares against one for each
other list (lazygit's `expandedSidePanelWeight` default), and it is the *context* panel rather
than the focused one, so moving into `[0]` and back does not reflow the column. Status keeps its
fixed two lines. `expand_focused = false` in config restores the even split for anyone who would
rather the layout never moved.

*Teaches:* layout is a function of state like everything else; a ratio in a constraint is the
whole feature.
*Done when:* focusing `[3]` grows Pipelines and shrinks Jobs, and pressing `0` changes nothing.

### M36 — Portrait mode
lazygit's portrait layout. A terminal that looks taller than wide (a half-screen split on a
laptop) gets the side column on top and `[0]` below, each half the height, instead of a
one-third-wide side column that fits nothing. The test is `width < 2 × height` because a
terminal cell is about twice as tall as it is wide, so that is where a window turns visually
portrait. No config: the shape of the window is the setting. `+` still takes a panel full screen.

*Teaches:* read the terminal size every frame and let the layout follow; nothing else has to know.
*Done when:* 50×40 stacks the panels and 80×24 does not.

### M37 — Pause and resume schedules
lazydocker's `p` pause, for the one thing an on-call person does to a job at night: stop it
firing. *Pause schedule* and *Resume schedule* join the `x` menu for a job, behind the same
`allow_actions` gate and confirmation as run-now. `jobs/update` replaces `schedule` wholesale,
so the client reads the job first and sends the cron and time zone back with the new
`pause_status`; a job without a schedule gets a plain error naming it. The list response carries
no schedule, so an unopened job offers both items and Databricks settles which applies; once
`jobs/get` has been seen, only the right one shows. A paused job carries `‖` before its name in
`[2]`, and the Detail tab already said "(paused)".

*Teaches:* a PATCH-shaped API that is really a PUT wants a read before the write, and that
belongs in the client, not in `update`.
*Done when:* a paused job stops appearing in the morning's runs and shows `‖` in the list.

### M38 — Status dashboard
lazygit's `statusPanelView: dashboard`. `[1]` gains a third line, `◐ 2 running · ✗ 3 failed ·
● 1 compute up`, counted over every job's newest run and every compute row rather than the
filtered view, so it answers "is anything on fire" before a single key is pressed. The counts are
a pure function of state already in `App`; the panel grows one row; `Y` on Status copies the line.

*Teaches:* the cheapest dashboard is a fold over data you already hold.
*Done when:* a failed nightly shows `✗ 1 failed` while the list is filtered to something else.

### M39 — Profile switcher
lazygit's recent-repos menu. `p` used to cycle the profiles named at launch; now it opens a
menu of every `[section]` in `~/.databrickscfg`, cursor on the current one. Choosing an open
profile shows its workspace; choosing a closed one opens it on the spot, same as `-p` would have,
and a profile the CLI cannot mint a token for reports why in the hint bar. `App` learns the list
once at start; `main` still owns the workspaces and the switch, so `App` never holds two
profiles' state. The menu is the `x` machinery with a different title and no actions gate.

*Teaches:* the second use of a menu is when it earns its generality; before that, one enum
variant was enough.
*Done when:* a consultant with six workspaces reaches any of them in two keys without restarting.

### M40 — JSON tab
lazydocker's Config tab. Jobs and pipelines get a third tab, **JSON**: the job's settings once
`jobs/get` has been seen (the Detail tab's fetch, shared), the pipeline's spec from
`pipelines/get` the first time the tab opens. Pretty-printed with `null` members stripped, since
the model fills every absent field with `None` and a page of nulls says nothing. It scrolls like
every other text view. The pipeline spec stays untyped JSON: the tab shows what Databricks sent,
and nothing else reads it.

*Teaches:* when the view is "the raw thing", do not type the raw thing.
*Done when:* the JSON tab of a bundle-deployed job shows its `deployment` block and no `null`.

### M41 — Output tab
lazydocker's Logs tab, as far as a REST API allows. A fourth Jobs tab, **Output**, lists every
task of the viewed run with what `runs/get-output` returned for it: the notebook result, the
logs of a Python or JAR task, then the error and its trace. Detail already fetched output for
failed tasks; Output asks for the rest while it is open, one call per task, once. Truncation is
written out rather than hidden. The text is built in `App` as plain lines, so the same string can
go to a pager next. Log *tailing* stays deferred: the API returns the last 5 MB on request, not a
stream, and `o` opens the page that does stream.

*Teaches:* when the API gives a snapshot, show a snapshot and say where the stream is.
*Done when:* a notebook that exits with JSON shows it under `▸ task  SUCCESS`.

### M42 — Page it
lazydocker's "view logs in pager". Enter on the JSON or Output tab hands the tab's text to
`$PAGER` (`less`, or `more` on Windows) the way M34 hands the terminal to a command: the text
goes to one scratch file in the temp dir, the pager runs with the screen, Enter brings the TUI
back. Nothing new in `App` beyond `Command::Page(String)`; the tabs already build their text as
plain lines for exactly this.

*Teaches:* once the terminal hand-off exists, every "view this properly" feature is one command.
*Done when:* a 3 000-line traceback is searchable with `/` in less.

### M43 — Copy menu
lazygit's `y` opens a menu of attributes to copy; ours copied the URL and nothing else. Now `y`
opens **Copy**: the URL first, so `y` Enter still pastes a link into Teams, then the run id when
one is under the cursor, the job, pipeline or compute id, the name, and the JSON once the tab has
fetched it. The menu is the `x` machinery with a `CopyText` entry that carries its own text, so
choosing never looks anything up again. No actions gate: a clipboard is not a write.

*Teaches:* the third use of the menu cost nothing; the entry carries its payload.
*Done when:* `y j Enter` on a run puts its id on the clipboard for a support ticket.

### M44 — The log as something you can run
The API log was put there to teach the API (§3). The copy menu now ends with the newest call as
`databricks api get '/api/2.2/jobs/runs/list?…' -p dev` and as a `curl` line with a
`$DATABRICKS_TOKEN` placeholder, so what the TUI just did can be pasted into a shell, a script or
a bug report. The log itself is not a focusable panel; the newest call is the one you just caused,
and that is the one worth copying. On the Status panel, with nothing selected, it is all `y` offers.

*Teaches:* an observability panel earns its space when its rows can leave the program.
*Done when:* `1 y Enter` after a refresh pastes a CLI line that returns the same JSON.

### M45 — The `:` prompt
lazygit's `:` runs a shell command; ours runs one `databricks` CLI line, since that is the shell
command anyone here wants. `:jobs get {{job_id}}` becomes `databricks jobs get 1 -p dev`, the
same placeholders as M34 filled from the selection, the output in the same popup. A placeholder
with nothing to fill it keeps the prompt open with the reason. Half the value of custom commands
for no config, and the way to find out which lines are worth a `[[commands]]` entry.

*Teaches:* a prompt is a custom command typed late; reuse the expansion, add one input mode.
*Done when:* `:runs list --job-id {{job_id}}` shows the same runs the table does.

### M46 — Config tab
§3 promised Status a Config tab; with eighteen settings it is overdue. The tab shows the
*effective* configuration as TOML, defaults filled in, headed by where the file was (or was not)
found. `Config` gains `Serialize`, which forces `Key` to learn its config spelling (`ctrl+d`,
`up`) as the inverse of parsing, and a test renders a config and reads it back as itself. Enter
pages the text like the other tabs. No "edit here": the file is the interface, and M47 opens it.

*Teaches:* a struct that can be read from a file should be able to write itself back; the
round-trip test catches every asymmetry.
*Done when:* someone who never wrote a config sees every key they could set, with its default.

### M47 — Edit the config
lazygit and lazydocker both bind `e` on the project panel to "edit config". `e` here opens
`config.toml` in `$VISUAL`, `$EDITOR`, or the platform's plain editor through the M34 hand-off,
then says that changes apply after a restart. No live reload: the file feeds the keymap, the
filters and the theme of every open workspace, and rebuilding those mid-session buys little over
`q` and up-arrow. The Config tab next door shows what to type.

*Teaches:* an honest "restart to apply" beats a reload that is right most of the time.
*Done when:* `e`, add `mine_only = true`, save, quit, relaunch: the filter is on.

### M48 — Name replacements
lazydocker's `replacements`, for the same reason: Databricks Asset Bundles prefix every dev
deployment with `[dev bjorn_punsvik] `, and in a 25-column list that prefix is most of the row.
`[name_replacements]` in config is a table of literal `from = to` pairs applied in key order to
names in the three side lists. Display only: the filter matches, the copy menu copies and the
JSON shows the full name, so nothing is hidden, only shortened.

*Teaches:* a display transform is a function on the way to the screen, never a change to state.
*Done when:* `"[dev bk] " = ""` turns `[dev bk] nightly_bronze_ingest` into a row that fits.

### M49 — Filter menu
lazygit's `ctrl+s` filter options. `f` cycles three statuses and `m` toggles mine, and both are
fine until a fourth filter arrives; `F` lists them as a menu with the cursor on the status in
force, plus "clear text filter" while `/` has text. Choosing applies at once and reads back the
Status panel's filter line as the notice. Menu entries that are state rather than IO return no
commands, so `MenuItem::command` became `commands`, a list, which bulk actions will want next.

*Teaches:* a menu that mutates state and returns nothing is still the same menu.
*Done when:* someone who never found `m` reads "Mine only: on" and presses Enter.

### M50 — Range select
lazygit's `v`. The cursor list gains an anchor: `v` sets it, moving extends the range from it,
`v` or `Esc` ends it, and a refetch that replaces the rows drops it rather than guess. Rows in
the range take the unfocused highlight so the cursor still reads as the cursor, and the hint bar
turns into `3 selected │ Actions: x`. `Esc` takes the range before it takes anything else.
Nothing acts on the range yet; that is M51, and it will find `selected_items` waiting.

*Teaches:* selection is list state, not app state; one `Option<usize>` on `Selectable` carries it.
*Done when:* `v j j` highlights three rows and `Esc` leaves the cursor where it was.

### M51 — Bulk actions
lazydocker's `b`, without the extra key: over a range of two or more, `x` offers the same
actions as for one row, each once per row and named with its count: "Run now: 3 jobs", "Cancel
2 active runs", "Stop 4 running compute". A `Bulk` entry carries its commands, so `commands()`
being a list (M49) pays off here. The confirmation names the count, the same `allow_actions` gate
holds, and firing ends the range so Enter twice cannot double it. Custom commands stay single-row:
their placeholders name one thing.

*Teaches:* the bulk version of an action is the action mapped over a list; design the single
version so that the map is all there is.
*Done when:* an on-call person cancels every active run of six jobs with `v`, five `j`, `x`, Enter,
`y`.

### M52 — Search in `[0]`
lazygit distinguishes filter (hide what does not match) from search (highlight, keep the
context). Side lists filter; `[0]` now searches. `/` with the main panel focused types a search,
every text view highlights the lines holding it, and `n`/`N` scroll to the next or previous one,
wrapping. The draw already reported the scroll limit back (M33); it now reports the matching line
indexes too, as one `Drawn` struct, so `n` lands on a row the renderer actually highlighted rather
than one `App` guessed at. The runs table is not searched: the side filter already narrows jobs,
and a run is found by scrolling three rows.

*Teaches:* when the renderer must be the source of truth, widen the report it already sends.
*Done when:* `/Traceback` in a failed run and `n` lands on each trace in turn.

### M53 — Compare two runs
lazygit's `W` marks a commit to diff against. `W` on an open run marks it; the Detail of any other
run then carries a section: the marked run's result and duration against this one's with the
delta, then every task both runs share, earlier figure first. "Why did tonight fail when last
night passed" becomes one screen: the task that went red and the one that took four minutes
longer. The mark survives leaving the run and the job; `W` on the marked run clears it. Nothing
is fetched: both runs came through `runs/get` already.

*Teaches:* a comparison is two values already held and one render; resist a "diff" data model.
*Done when:* the on-call person marks yesterday's run, opens today's, and reads the delta.

### M54 — Duration sparkline
lazydocker draws a stats graph under a container; a job has one metric worth a graph, how long
its runs took. The runs table gives its last row to a ratatui `Sparkline` of the listed runs'
durations, oldest to newest, when the panel is at least eight rows tall and two runs have
finished. §10 deferred this because the browser draws it; the browser draws it on another page,
and the widget is already in the crate. No config: the row goes back to the table when there is
no room.

*Teaches:* a deferred feature whose cost dropped to zero is no longer deferred.
*Done when:* a job whose runs went from 40 s to 4 min shows a slope under its table.

### M55 — Update check
Both tools check for a new version; M31 made releases, so there is something to check. `u` asks
GitHub's releases API for the newest tag and compares it numerically with the built version;
a newer one is a notice and then a yellow `· v0.2.0 out` on the Status line until exit, an equal
one says "up to date", a failure says why. `check_updates = true` runs it at start. Off by
default: a request to a third party on every launch is the person's call, not the program's. The
call lives beside `Client` but outside it, since it is not a Databricks request and does not
belong in the API log.

*Teaches:* a third-party call is opt-in, off the API log, and one function.
*Done when:* `u` on an old binary names the newer version and where to get it.

### M56 — Quit polish
The small lazygit settings that people miss when they are gone: `confirm_on_quit` makes `q` ask
(the confirmation reuses the `x` machinery with a `Quit` entry; `ctrl+c` never asks),
`quit_on_top_level_return` makes `Esc` quit once there is nothing left to back out of, and `Home`
and `End` join `g` and `G` as first and last, in config spelling too. Both settings stay off by
default: a TUI that quits on a stray `Esc` is a surprise the first time. `ctrl+z` suspend is not
here: it needs a signal the standard library cannot raise, and every terminal already has a
second tab.

*Teaches:* the last five percent of a port is settings, and each one is a test.
*Done when:* `q` asks, `Esc` `Esc` from `[0]` quits, and `End` lands on the last job.

---

## 9. Testing

Tests are written with each milestone, not deferred. The `DatabricksApi` trait has a fixture-backed
fake so `update` is testable without network (see CLAUDE.md). Three layers, cheapest first:

1. **Model tests** — filtering, sorting, duration formatting, state transitions. Pure functions,
   no IO. Most of your assertions live here.
2. **API deserialization tests** — commit real (anonymized) JSON responses as fixtures and assert
   they parse. This is the layer that protects you from Databricks changing a field, which is the
   class of failure that killed the Python tool.
3. **Snapshot render tests** — render `App` into a `TestBackend` of fixed size and snapshot the
   text buffer with `insta`. Catches layout regressions.

`clippy.toml` already permits `unwrap`/`expect`/`panic`/indexing in tests, so test code can be
direct. Use that freedom.

No test should require network or a live workspace.

---

## 10. Deferred

Deliberately out of the first thirty-two milestones. Revisit only if you actually want them:

- Model-serving pane (more of the same as compute)
- Log tailing for a run: `o` opens the run page, which streams logs better than a TUI can
- Mouse support
- Billing / usage views: need a SQL warehouse and `system.billing.usage`
- Notebook or SQL browsing
- Editing job JSON in `$EDITOR` and lazygit-style custom commands
- Dependency graph between jobs: the browser draws it already (duration sparklines landed in M54)

---

## 11. Open questions

1. **Where does `me` come from?** `GET /api/2.0/preview/scim/v2/Me` is the clean answer but is
   another endpoint and a preview API. Alternative: read the `sub` claim from the OAuth JWT you
   already mint. Decide at M5.
2. **Tag key stability.** Filtering leans on `settings.tags.dev`. Confirm that convention holds
   across the whole workspace, not just your own jobs, before making it the default.
3. **Pipelines drill-down.** Pipelines have updates rather than runs, so the three-pane shape may
   not transfer directly. Defer the decision until M4 is working for jobs. *Resolved at M11:* the
   shape transfers as-is, with an Updates tab in place of Runs, fed by `latest_updates`.


