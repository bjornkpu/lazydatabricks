# lazydatabricks

Rust TUI for Databricks. Also serves as a bootstrap template for future Rust projects.
Conventions follow https://www.namtao.com/rust/.

## Commands

- `cargo nextest run` — tests (use this, not `cargo test`)
- `cargo clippy --all-targets` — must be clean; lints are `deny`, so this is the compile gate
- `cargo fmt --check` — formatting
- `bacon clippy` / `bacon nextest` — watch mode during development
- `cargo run` — run the app

Before claiming anything works: clippy, fmt, nextest, all green.

## Lints

`[lints.clippy]` in `Cargo.toml` is pedantic + nursery + panic-denying lints. Never weaken it
to make code compile. No `unwrap`/`expect`/`panic`/`todo`/indexing/`as` casts in non-test code.
`clippy.toml` allows them in tests; prototype there.

`#[allow(clippy::...)]` needs a one-line comment saying why, and must be as narrow as possible
(one item, never module-wide).

## Dependencies

Preferred crates are listed as comments under `[dependencies]` in `Cargo.toml`. Uncomment one
when needed. Don't add an alternative crate for a job a listed one does. Ask before adding
anything not on the list.

## Style

- Follow the ponytail rule: smallest working change. No speculative abstractions, no traits
  with one implementation, no config for constants.
- Typestate pattern (see namtao page) for states that must not be mixed at runtime.
- Errors: `Result` everywhere, `color-eyre` at the top level when added. Never swallow errors.
- Every non-trivial branch, parser, or state transition leaves one test behind.

## Architecture (LLM-first: everything must be checkable as text)

Elm-style. Three pure pieces, one thin IO shell:

- `Event` — our own enum (`Key(..)`, `Tick`, `JobsLoaded(..)`, ...). crossterm events are
  converted to `Event` at the boundary in `main`; nothing else imports crossterm.
- `update(&mut State, Event)` — pure state transition, no IO. All logic lives here.
- `view(&State, &mut Frame)` — pure render. No state mutation.
- `DatabricksApi` trait — the only network boundary. Two impls: real HTTP client, and a fake
  fed from JSON fixtures in `tests/fixtures/`. This is the one trait allowed to exist with a
  single production implementation, because the fake is the point.

Use the typestate pattern for screens whose transitions must not be mixed at runtime.

### Testing layers

1. **State tests** — feed `Vec<Event>` to `update`, assert on `State`. Most tests go here.
2. **Snapshot tests** — render `view` into ratatui `TestBackend` (80x24 unless the test says
   otherwise) and `insta::assert_snapshot!`. The `.snap` files are ASCII screens; read them to
   "see" the UI. Review changes with `cargo insta review`, or `cargo insta accept` when the
   diff is intended. Never accept a snapshot you haven't read.
3. **Fixture tests** — the fake `DatabricksApi` returns fixtures; tests cover deserialisation
   and the `update` reaction to loaded data. Fixtures are real API response shapes.
4. **Headless run** (add once the first screen renders) — `--headless --keys "j j Enter q"
   --fixtures <dir> --dump` prints the final screen as text; `--dump-state` prints `State` as
   JSON. Use this to verify end-to-end from the shell.
5. **Logs** (add when the first real-API bug hides) — `tracing` to a file via
   `tracing-appender`, never stdout. `LAZYDATABRICKS_LOG=debug`.

Not now: pty/tmux end-to-end tests, proptest on the state machine. Add when a real bug
motivates them.

### Definition of done for a change

clippy clean, fmt clean, `cargo nextest run` green, snapshots reviewed. If a state
transition or parser changed and no test changed, something is missing.
