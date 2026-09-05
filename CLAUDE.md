# lazydatabricks

Rust TUI for Databricks. Also serves as a bootstrap template for future Rust projects.
Conventions follow https://www.namtao.com/rust/.

## Commands

Work order is `docs/spec.md` milestones M0..M59. Tests are written per milestone, not at the end.

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
(one item, never module-wide). The spec (§2) names `arithmetic_side_effects` and `as_conversions`
as the two that may be relaxed if they obstruct rather than teach. Ask BK before doing so.

## Dependencies

Chosen crates are listed as comments under `[dependencies]` in `Cargo.toml`, audited against
crates.io. Uncomment one when its milestone needs it. Don't add an alternative crate for a job a
listed one does. Ask before adding anything not on the list.

## Style

- Modern idiomatic Rust, edition 2024. Concretely: `let ... else` over nested `match`; let
  chains (`if let ... && ...`); iterators and closures over index loops; `?` over manual
  matching; `impl Trait` in argument and return position; `TryFrom`/`try_into` for narrowing;
  `std::sync::LazyLock` over `lazy_static`/`once_cell`; `thiserror` for typed errors,
  `anyhow` only at `main`; `#[must_use]` on pure functions returning values; `&str` and slices
  in parameters, owned types in fields; no `Rc<RefCell<_>>` in app state; no `.clone()` added
  just to satisfy the borrow checker without a comment saying why. When unsure what is
  idiomatic, check the Rust API Guidelines and current ratatui examples, not old blog posts.
- Follow the ponytail rule: smallest working change. No speculative abstractions, no traits
  with one implementation, no config for constants.
- Typestate pattern (see namtao page) for states that must not be mixed at runtime.
- Errors: `Result<_, AppError>` everywhere (`src/error.rs`, thiserror), `anyhow` only in `main`. Never swallow errors.
- Every non-trivial branch, parser, or state transition leaves one test behind.

## Architecture (LLM-first: everything must be checkable as text)

Elm-style. Three pure pieces, one thin IO shell:

- `Message` — our own enum (`Key(..)`, `Tick`, `JobsLoaded(..)`, ...), the Elm/iced term.
  crossterm `Event`s are converted to `Message` at the boundary in `main`; nothing else
  imports crossterm. Async tasks never touch `App`; they send `Message`s down an mpsc channel.
- `App::update(&mut self, Message) -> Vec<Command>` — the only place state mutates, no IO.
  Side effects come back as `Command`s (`Quit`, `FetchJobs`, `FetchRuns`, `RunNow`, `CancelRun`) that `main` executes;
  `Command::Quit` instead of calling `exit`. `main` spawns the first jobs fetch itself.
- `ui::draw(&App, &mut Frame)` — pure render. No state mutation.
- `DatabricksApi` trait — the only network boundary. Two impls: real reqwest client, and a fake
  fed from JSON fixtures in `tests/fixtures/`. This is the one trait allowed to exist with a
  single production implementation, because the fake is the point. Cheap under the mpsc
  design: the fake just sends `Message::JobsLoaded(fixture)`.

Use the typestate pattern for screens whose transitions must not be mixed at runtime.

### Testing layers

1. **State tests** — feed `Vec<Message>` to `App::update`, assert on `App`. Most tests go here.
2. **Snapshot tests** — render `ui::draw` into ratatui `TestBackend` (80x24 unless the test says
   otherwise) and `insta::assert_snapshot!`. The `.snap` files are ASCII screens; read them to
   "see" the UI. Review changes with `cargo insta review`, or `cargo insta accept` when the
   diff is intended. Never accept a snapshot you haven't read.
3. **Fixture tests** — the fake `DatabricksApi` returns fixtures; tests cover deserialisation
   and the `update` reaction to loaded data. Fixtures are real API response shapes.
4. **Headless run** — not built. Snapshot and state tests covered every milestone, and herdr
   lets Claude drive the real binary in a pane and read the screen back. Revisit only if a bug
   needs a scripted end-to-end run; it would need the fixture-backed `DatabricksApi` fake.
5. **Logs** — `LAZYDATABRICKS_LOG=debug` writes `tracing` output to `lazydatabricks.log` next
   to the config file via `tracing-appender`, never stdout. Requests, statuses and executed
   commands are logged; `warn!` on every failed response.

Not now: pty/tmux end-to-end tests, proptest on the state machine. Add when a real bug
motivates them.

### Definition of done for a change

clippy clean, fmt clean, `cargo nextest run` green, snapshots reviewed. Nothing needs network or a live workspace. If a state
transition or parser changed and no test changed, something is missing.
