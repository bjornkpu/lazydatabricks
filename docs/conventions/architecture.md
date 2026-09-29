# Architecture

## Pure core, thin IO shell

The shell gathers facts, a pure function decides, the shell executes. Facts in, plan out.

- The core is pure. It takes and returns plain data: no filesystem, no network, no
  processes, no clock, no environment variables. Anything it needs from the world arrives as
  a parameter.
- The shell does all IO: reading files, spawning processes, HTTP, the terminal, logging.
- `src/main.rs` is wiring: parse arguments, start logging, gather facts through the shell, call
  the core, execute the result through the shell. `anyhow` lives here and nowhere else.

In lazydatabricks there is no `domain/` and `io/` split by folder. The pure core is `src/app/`
(state and `update`), `src/ui/` (render), `src/api/models.rs` (REST shapes), config parsing in
`src/config.rs`, and `src/error.rs`. The shell is `src/main.rs`, the HTTP client and token
lookup in `src/api/mod.rs` and `src/api/auth.rs`, `config::load`, and `src/shell.rs` (browser,
clipboard, custom commands).

Because the core is plain data, it is tested directly with no mocks and no fakes. When a
decision is hard to test, the IO has leaked into it: move the IO out and pass its result in.

A decision with several effects returns them as data (a `Vec<Step>`, a plan struct) for the
shell to execute. Tests assert on the plan; one integration test proves the executor runs it.
Here the plan is the `Vec<Command>` that `App::update` returns.

## Traits: the boundary rule

No traits with one implementation, with one exception: a boundary trait per external service
that is slow, costly, or non-deterministic (a network API, a GPU model, an LLM). It has the
real implementation and a fake, and the fake is the point: tests never touch the service.

```rust
pub trait Forecast {
    async fn today(&self, city: &str) -> Result<Weather, AppError>;
}

pub struct HttpForecast { client: reqwest::Client, base: String }
impl Forecast for HttpForecast { /* real call */ }

#[cfg(test)]
pub struct FakeForecast(pub Weather);
#[cfg(test)]
impl Forecast for FakeForecast {
    async fn today(&self, _city: &str) -> Result<Weather, AppError> { Ok(self.0.clone()) }
}
```

In lazydatabricks the boundary is Databricks, and the trait for it is `DatabricksApi`, with a
fake fed from the JSON fixtures in `tests/fixtures/`. It is not built yet: `api::Client` is a
concrete struct that only `src/main.rs` calls, and tests never reach it because `App::update`
only returns `Command`s. Fixtures go straight to serde and, as `Message`s, to `update`. Add the
trait and the fake when something needs to run the real loop without a workspace (see the
headless run in `testing.md`). Under the mpsc design the fake only has to send
`Message::JobsLoaded(fixture)`.

Local things are not boundaries. git, SQLite and the filesystem are used for real in tests,
inside a temp dir (see `testing.md`).

## Typestate

When a value moves through states that must never be mixed at runtime (unvalidated then
validated, draft then sent), make each state its own type and make the transition a function
that consumes one and returns the next. The compiler then rejects the mix-up. Use it for
screens whose transitions must not be mixed.

## Paths

The config file is `config.toml` in the platform config dir (`directories`), or the file given
by `--config` or `LAZYDATABRICKS_CONFIG`. The log file sits next to it. Authentication is the
Databricks CLI's: the profile comes from `~/.databrickscfg` and the token from
`databricks auth token`. Never cache a token to disk.

## Terminal UI (Elm style)

Three pure pieces and one IO shell:

- `Message` (`src/app/message.rs`): our own enum for everything that can happen (`Key(..)`,
  `Tick`, `JobsLoaded(..)`). crossterm events become `Message`s in `src/main.rs`, the only
  file that imports crossterm (INV-1).
- `App::update(&mut self, Message) -> Vec<Command>` (`src/app/mod.rs`): the only place state
  changes. No IO. Side effects come back as `Command`s (`Quit`, `FetchJobs`, `FetchRuns`,
  `RunNow`, `CancelRun`, ...).
- `ui::draw(&App, &mut Frame)` (`src/ui/mod.rs`): pure render, never mutates.
- `src/main.rs`: owns the terminal, runs the draw/update loop, executes `Command`s, and spawns
  the first jobs fetch itself. Background work (fetches, long jobs) runs in tokio tasks that
  never touch `App`; they send `Message`s down an mpsc channel. `Command::Quit` ends the loop;
  nothing calls `exit`, so the terminal is always restored.

This differs from the default layout, where the loop lives in its own terminal module: here
`src/main.rs` is both the wiring and the terminal shell.
