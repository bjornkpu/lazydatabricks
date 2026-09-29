# Testing

TDD, one behaviour at a time: write the test, watch it fail for the right reason, write the
least code that passes, refactor while green. Every non-trivial branch, parser, or state
transition leaves a test behind. If a parser or decision changed and no test changed,
something is missing.

Run with `cargo nextest run`, never `cargo test`. This crate has no cargo features, so
`--all-features` changes nothing.

## Layers

- **Pure unit tests**, in-module (`#[cfg(test)] mod tests`) next to the code. Plain data in,
  assert on plain data out.
- **Plan tests**: for decisions that return steps, assert the exact step sequence for each
  branch.
- **State tests**: feed a `Vec<Message>` to `App::update`, assert on `App` and the returned
  `Command`s. Most tests in lazydatabricks live here.
- **Screen snapshots**: render `ui::draw` into ratatui's `TestBackend` (80x24 unless the test
  says otherwise) and `insta::assert_snapshot!(terminal.backend())`. The `.snap` file is the
  screen as text; read it to see the UI.
- **Fixture tests**: `tests/fixtures/` holds real Databricks API response shapes. Tests cover
  their deserialisation and the `update` reaction to the loaded data.
- **Text snapshots**: `insta` for output a user reads (tables, help, rendered text). Inline
  (`@"..."`) when short, files when long.
- **Integration tests** in `tests/`: run the real binary through
  `env!("CARGO_BIN_EXE_lazydatabricks")`, with its config in a temp dir. lazydatabricks has
  none yet: the binary needs a logged-in Databricks CLI, and every test must pass without one.
- **Headless run**: not built. Snapshot and state tests covered every feature so far, and herdr
  lets Claude drive the real binary in a pane and read the screen back. Build it only when a bug
  needs a scripted end-to-end run; it needs the fixture-backed `DatabricksApi` fake from
  `architecture.md`.

## Snapshots

Review with `cargo insta pending-snapshots` or `cargo insta review`. Accept only after
reading the new content. Never accept a snapshot you have not read, and never accept one to
make a red test green without understanding the diff.

## Integration test isolation

Nothing in a test may touch the user's real config, repos, or home. Point the tool's config at
a temp dir (`--config` or `LAZYDATABRICKS_CONFIG`). When a test runs git, isolate it from the
user's git config so hooks and signing never run:

```rust
fn command(&self, program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(self.repo())
        .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "BK")
        .env("GIT_AUTHOR_EMAIL", "bk@example.com")
        .env("GIT_COMMITTER_NAME", "BK")
        .env("GIT_COMMITTER_EMAIL", "bk@example.com")
        .env("LAZYDATABRICKS_CONFIG", self.root.join("config.toml"));
    cmd
}
```

Start every file in `tests/` with `#![cfg(test)]`, so the `clippy.toml` test allowances
(`unwrap`, `expect`, `panic`, indexing) apply to its helpers too.

## What never runs in a test

Network calls, a live Databricks workspace, GPUs, model files, paid APIs, or external CLIs
whose output is not deterministic (`databricks`, `gh`, `az`, an LLM). Their argument building
and reply parsing are pure and tested with plain strings; the call itself sits behind a
boundary (see `architecture.md`).

## Later, when a bug motivates it

- `proptest` for parsers and state machines, once a bug shows example tests missed a case.
- pty or tmux end-to-end tests.
- `cargo mutants` as a manual check for code no test would notice changing. Not in CI; it is
  slow.
