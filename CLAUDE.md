# lazydatabricks

A lazygit-style terminal UI for Databricks jobs, pipelines and compute, in Rust.

## Commands

- `cargo nextest run`: tests (use this, not `cargo test`)
- `cargo clippy --all-targets -- -D warnings`: must be clean; lints are `deny`, so this is the
  compile gate
- `cargo fmt --check`: formatting
- `cargo deny check`, `cargo machete`: dependency advisories, licenses, bans, unused crates
- `bacon clippy-all` / `bacon nextest`: watch mode
- `cargo run -- <args>`: run the TUI. It needs the Databricks CLI logged in to a profile.
  `--config <file>` (or `LAZYDATABRICKS_CONFIG`) keeps your real config clean;
  `LAZYDATABRICKS_LOG=debug` writes `lazydatabricks.log` next to the config file.
- `bd ready`: what is buildable next

The gate: fmt, clippy, nextest, all green before claiming anything works. The Stop hook in
`.claude/settings.json` runs it at the end of every turn and blocks while it is red. CI runs the
same gate plus deny and machete.

## Workflow

1. Work order is the milestones in `docs/spec.md`. Tests are written per milestone, not at the
   end.
2. Track the work in beads (`bd`).
3. Build it with TDD, one behaviour at a time: red, green, refactor.
4. When a design settles, fold the decisions into the tracked docs: `README.md` for behaviour,
   `docs/spec.md` for the design and the reasoning behind it, `docs/invariants.md` for rules
   with the tests that pin them, and `CLAUDE.md` for the module map.

Beads are local only. They are gitignored and never pushed (never `bd dolt push`).

## Decided, do not ask

Everything in `docs/conventions/` and in the `Cargo.toml` comments is BK's standing preference:
crates, layout, architecture, testing, errors, style, release. Brainstorming, grilling and
planning sessions treat it as decided. Ask only about features, the domain, and real conflicts
between a feature and a convention; when you raise a conflict, name the convention.

## Hard rules

- Never weaken `[lints]` in `Cargo.toml` to make code compile. `#[allow(clippy::...)]` goes on
  one item only, with a one-line comment saying why. Ask BK before relaxing
  `arithmetic_side_effects` or `as_conversions`.
- No `unsafe` (`unsafe_code = "forbid"`).
- Pure core, thin IO shell: only `src/main.rs`, `src/api/mod.rs`, `src/api/auth.rs`,
  `config::load` and `src/shell.rs` do IO. `App::update` returns `Command`s; `main` runs them.
- Never break an invariant in `docs/invariants.md`. An invariant without a test is a bug.
- Never accept a snapshot you have not read.
- No crate outside the `Cargo.toml` catalog without asking BK. Never an alternative for a job a
  listed crate does.
- No test needs network, a live workspace, or the user's real config.

## Commits

Conventional Commits, one commit per milestone. release-plz builds `CHANGELOG.md` from the
subjects, so a subject describes the change for a user: never a milestone number (`M62`),
never a bead id, never "this commit". `feat: y copies the install line when a newer release
exists`, not `feat: M62 ...`. The milestone lives in `docs/spec.md` and the bead, not in git.

Releases publish to crates.io as well as GitHub Releases (`publish = true` in
`release-plz.toml`). See `docs/releasing.md`.

## Map

```
src/
  main.rs           clap, logging, terminal, draw/update loop, runs Commands;
                    the only crossterm import; anyhow only here           [IO]
  cli.rs            command line flags
  config.rs         config.toml: parse and render [pure], load [IO]
  error.rs          AppError (thiserror)
  shell.rs          browser, clipboard, custom command lines               [IO]
  api/
    mod.rs          Client: reqwest, pagination, the API log               [IO]
    auth.rs         host from ~/.databrickscfg, token from the CLI         [IO]
    models.rs       serde mirrors of the REST shapes                       [pure]
  app/              Message, Command, App::update                          [pure]
    message.rs      Message enum
    focus.rs        panels, screen modes, main-panel tabs
    keys.rs         keymap, lazygit defaults, config overrides
    list.rs         a list with a cursor
    filter.rs       who "me" is, which items are visible
    menu.rs         the x menu and its confirmations
    custom.rs       custom commands and their placeholders
  ui/               draw(&App, &mut Frame)                                 [pure]
    side.rs         [1] Status, [2] Jobs, [3] Pipelines
    main_panel.rs   [0] tabs
    chrome.rs       borders, numbered titles, n of m
    hints.rs        contextual hint bar
    apilog.rs       API log panel
    help.rs         ? overlay
    menu.rs         x menu overlay
    popup.rs        custom command output
    theme.rs        glyphs, colours, time formatting
tests/
  fixtures/         real Databricks API response shapes
```

Module boundaries may shift; the pure/IO split does not.

## Conventions

Read the one that matches what you are about to do:

- `docs/conventions/architecture.md`: before adding a module, a trait, or anything with IO.
- `docs/conventions/testing.md`: before writing or changing a test.
- `docs/conventions/errors.md`: before adding an error variant, a log line, or output.
- `docs/conventions/style.md`: before writing code; the lint cheat sheet is there.
- `docs/conventions/crates.md`: before adding or uncommenting a dependency.
- `docs/releasing.md`: before touching versions, tags, or release config.
