# Crates

The chosen crate for each job, and why. `Cargo.toml` carries the short form of this list; this
file is the authority when they disagree. Uncomment a crate when a task needs it. Never add an
alternative for a job a listed crate does. Ask BK before adding anything not listed.

Versions are pinned to the major only (`"1"`, `"0.30"`); `Cargo.lock` holds the exact version.
The list was audited against crates.io (version, release cadence, downloads) on 2026-09-04.

## In use

| Job | Crate | Why |
| --- | --- | --- |
| TUI rendering | `ratatui` | The maintained TUI library; `TestBackend` makes screens snapshot-testable. |
| TUI events | `crossterm` | ratatui's default backend; works on Windows. Imported in `src/main.rs` alone (INV-1). |
| Error type | `thiserror` | One `AppError` enum with derived messages. |
| Error context in `main` | `anyhow` | `.context()` at the binary boundary; never below `main`. |
| (De)serialisation | `serde` (`derive`), `serde_json` | Databricks REST bodies, the CLI's token output, `--json`. |
| Config file | `toml` | `config.toml`, read and rendered back for the Config tab. |
| HTTP | `reqwest` (`json`, `query`) | The Databricks REST API, only from the IO shell. |
| Async runtime | `tokio` (`rt-multi-thread`, `macros`, `sync`) | Fetch tasks and the mpsc channel they report on. |
| Dates and times | `jiff` (`serde`) | Epoch millis to local time and relative age; correct time zones and spans. |
| Command line | `clap` (`derive`) | Derive keeps arguments, help and parsing in one struct. |
| Platform dirs | `directories` | The platform config dir for `config.toml`. |
| Logging | `tracing`, `tracing-appender`, `tracing-subscriber` (`env-filter`) | Structured logs to a file, never stdout, level set at runtime via `LAZYDATABRICKS_LOG`. |
| Snapshot tests (dev) | `insta` | Rendered screens and output reviewed as text. |

## Commented until needed

| Job | Crate | Rule |
| --- | --- | --- |
| TUI text input | `tui-input` | Only if the hand-rolled filter input stops being enough. |
| Iterator extras | `itertools` | Only when std iterators fall short. |
| Parallelism | `rayon` | Only once a measurement shows the need. |
| Benchmarks (dev) | `criterion` | Only once a measurement is needed. |

## Rejected

| Crate | Instead | Why |
| --- | --- | --- |
| `chrono` | `jiff` | jiff handles time zones and spans correctly with a smaller API surface. |
| `once_cell`, `lazy_static` | `std::sync::LazyLock` | In std since 1.80. |
| `color-eyre` | `anyhow` | Pretty reports add little in a CLI and nothing in a TUI. |
| `git2` | spawn `git` | libgit2 lags git and needs native builds. `gix` only if spawning becomes a measured problem. Banned in `deny.toml`. |
| `openssl` | rustls | No system OpenSSL to install or patch. `openssl-sys` banned in `deny.toml`. |
| `assert_cmd` | `std::process::Command` + `env!("CARGO_BIN_EXE_<name>")` | std covers it. |

## Adding a license

`deny.toml` allows permissive licenses (MIT, Apache-2.0, BSD, BSL-1.0, CDLA-Permissive-2.0,
ISC, Unicode-3.0, Unlicense, Zlib) and MPL-2.0, which only asks that changes to MPL files stay open. The
GPL family stays out: it would make the whole binary GPL. When a chosen crate needs a license
not on the list, add it with a comment naming that crate. A crate with no license gets a
`[[licenses.clarify]]` or `exceptions` entry with a reason, or is replaced. Never add a
license to unblock a crate that is not chosen.
