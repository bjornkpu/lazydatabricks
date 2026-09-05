//! lazydatabricks: a lazygit-style TUI for Databricks.
//!
//! This file is wiring only. It owns the terminal, converts terminal events into `Message`s,
//! spawns fetch tasks for the `Command`s `update` returns, and runs the draw/update loop.
//! Nothing outside this file imports crossterm.

mod api;
mod app;
mod cli;
mod config;
mod error;
mod shell;
mod ui;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, bail};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

use crate::app::{App, Command, Key, Message, TICK};
use crate::error::AppError;

/// Messages buffered between producers and the update loop.
const CHANNEL_CAPACITY: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let config_path = cli
        .config
        .or_else(|| std::env::var_os("LAZYDATABRICKS_CONFIG").map(Into::into));
    let mut loaded = config::load(config_path)?;
    // https://no-color.org: any non-empty value turns colours off.
    if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
        loaded.config.theme = config::Theme::Mono;
    }
    // Kept alive until exit so the last log lines are flushed.
    let _log_guard = init_tracing(loaded.path.parent().unwrap_or_else(|| Path::new(".")))?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");
    // Flags beat environment beat config beat the CLI's own default profile name. Commas
    // separate several profiles; each gets its own workspace.
    let profiles: Vec<String> = cli
        .profile
        .or_else(|| std::env::var("DATABRICKS_CONFIG_PROFILE").ok())
        .map(|list| {
            list.split(',')
                .map(|profile| profile.trim().to_owned())
                .filter(|profile| !profile.is_empty())
                .collect()
        })
        .or_else(|| (!loaded.config.profiles.is_empty()).then(|| loaded.config.profiles.clone()))
        .or_else(|| loaded.config.profile.clone().map(|profile| vec![profile]))
        .unwrap_or_else(|| vec!["DEFAULT".to_owned()]);
    loaded.config.allow_actions |= cli.allow_actions;
    if cli.filter.is_some() {
        loaded.config.filter = cli.filter;
    }
    // Auth before the terminal is taken over: the CLI may print or open a browser, and its
    // errors should land in a normal shell.
    let mut workspaces = Vec::new();
    for profile in &profiles {
        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
        let client = Arc::new(api::Client::from_profile(profile, tx.clone())?);
        let app = App::new(profile, client.host(), TimeZone::system(), &loaded);
        workspaces.push(Workspace {
            app,
            client,
            tx,
            rx,
        });
    }
    if let Some(command) = cli.command {
        let Some(first) = workspaces.first() else {
            bail!("no profile to query");
        };
        return print_json(&first.client, command, loaded.config.max_jobs).await;
    }
    for workspace in &workspaces {
        workspace.start();
    }
    let (input_tx, input_rx) = mpsc::channel(CHANNEL_CAPACITY);
    spawn_input(input_tx);
    // `ratatui::init` enters the alternate screen and raw mode and installs a panic hook that
    // restores both. `clippy::exit` is denied, so the loop returns instead of exiting; that is
    // what lets `restore` run on the error path too.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, workspaces, input_rx).await;
    ratatui::restore();
    result
}

/// One profile: its state, its client and the channel its fetches report on. Messages never
/// cross workspaces, so a late reply for `dev` cannot land in `prod`.
struct Workspace {
    app: App,
    client: Arc<api::Client>,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
}

impl Workspace {
    /// The fetches every workspace starts with.
    fn start(&self) {
        let max = self.app.max_jobs;
        tokio::spawn(fetch_jobs(Arc::clone(&self.client), self.tx.clone(), max));
        tokio::spawn(fetch_recent_runs(
            Arc::clone(&self.client),
            self.tx.clone(),
            max,
        ));
        tokio::spawn(fetch_pipelines(
            Arc::clone(&self.client),
            self.tx.clone(),
            max,
        ));
        tokio::spawn(fetch_clusters(
            Arc::clone(&self.client),
            self.tx.clone(),
            max,
        ));
        tokio::spawn(fetch_me(Arc::clone(&self.client), self.tx.clone()));
    }

    /// Runs one side effect. `Quit` and `NextProfile` belong to the shell, so they come back.
    fn execute(&self, command: Command) -> Option<Command> {
        let client = || Arc::clone(&self.client);
        let tx = || self.tx.clone();
        match command {
            Command::Quit | Command::NextProfile => return Some(command),
            Command::FetchJobs { max } => {
                tokio::spawn(fetch_jobs(client(), tx(), max));
            }
            Command::FetchRecentRuns { max } => {
                tokio::spawn(fetch_recent_runs(client(), tx(), max));
            }
            Command::FetchPipelines { max } => {
                tokio::spawn(fetch_pipelines(client(), tx(), max));
            }
            Command::FetchClusters { max } => {
                tokio::spawn(fetch_clusters(client(), tx(), max));
            }
            Command::StartCluster { cluster_id } => {
                tokio::spawn(start_cluster(client(), tx(), cluster_id));
            }
            Command::TerminateCluster { cluster_id } => {
                tokio::spawn(terminate_cluster(client(), tx(), cluster_id));
            }
            Command::FetchRuns { job_id } => {
                tokio::spawn(fetch_runs(client(), tx(), job_id));
            }
            Command::FetchRunDetail { run_id } => {
                tokio::spawn(fetch_run_detail(client(), tx(), run_id));
            }
            Command::FetchJob { job_id } => {
                tokio::spawn(fetch_job(client(), tx(), job_id));
            }
            Command::FetchRunOutput { run_id } => {
                tokio::spawn(fetch_run_output(client(), tx(), run_id));
            }
            Command::RunNow { job_id, params } => {
                tokio::spawn(run_now(client(), tx(), job_id, params));
            }
            Command::RepairRun { job_id, run_id } => {
                tokio::spawn(repair_run(client(), tx(), job_id, run_id));
            }
            Command::CancelRun { job_id, run_id } => {
                tokio::spawn(cancel_run(client(), tx(), job_id, run_id));
            }
            Command::StartUpdate { pipeline_id } => {
                tokio::spawn(start_update(client(), tx(), pipeline_id));
            }
            Command::StopPipeline { pipeline_id } => {
                tokio::spawn(stop_pipeline(client(), tx(), pipeline_id));
            }
            Command::OpenUrl(url) => {
                tokio::spawn(desktop(tx(), move || shell::open_url(&url)));
            }
            Command::Copy(text) => {
                tokio::spawn(desktop(tx(), move || shell::copy(&text)));
            }
            Command::CopyVisible => {
                let text = ui::visible_text(&self.app);
                tokio::spawn(desktop(tx(), move || shell::copy(&text)));
            }
            Command::Bell => {
                // BEL goes straight to the terminal; the next draw is unaffected.
                let mut out = std::io::stdout();
                let _ = out.write_all(b"\x07").and_then(|()| out.flush());
            }
        }
        None
    }
}

/// The draw/update loop over the active workspace. Keys and ticks go to the active one only;
/// an inactive workspace's fetches wait in its channel until it is shown again.
async fn run(
    terminal: &mut DefaultTerminal,
    mut workspaces: Vec<Workspace>,
    mut input: mpsc::Receiver<Message>,
) -> Result<()> {
    let count = workspaces.len();
    let mut active = 0;
    loop {
        let Some(workspace) = workspaces.get_mut(active) else {
            bail!("no workspace {active}");
        };
        terminal.draw(|frame| ui::draw(&workspace.app, frame))?;
        let message = tokio::select! {
            message = input.recv() => message,
            message = workspace.rx.recv() => message,
        };
        let Some(message) = message else {
            bail!("all message producers stopped");
        };
        for command in workspace.app.update(message) {
            tracing::debug!(?command);
            match workspace.execute(command) {
                Some(Command::Quit) => return Ok(()),
                Some(Command::NextProfile) => {
                    active = active.saturating_add(1).checked_rem(count).unwrap_or(0);
                }
                Some(_) | None => {}
            }
        }
    }
}

/// A subcommand: one listing as pretty JSON on stdout, then exit. The terminal is never taken
/// over, so this pipes into `jq` and `fzf`.
async fn print_json(client: &api::Client, command: cli::Sub, max: usize) -> Result<()> {
    let json = match command {
        cli::Sub::Jobs => serde_json::to_string_pretty(&client.list_jobs(max).await?)?,
        cli::Sub::Runs { job_id } => {
            serde_json::to_string_pretty(&client.list_runs(job_id).await?)?
        }
        cli::Sub::Pipelines => serde_json::to_string_pretty(&client.list_pipelines(max).await?)?,
    };
    println!("{json}");
    Ok(())
}

/// Logs to a file next to the config when `LAZYDATABRICKS_LOG` is set (`debug`, `info`, or a
/// full `tracing` filter). Never to stdout: that is the UI's.
fn init_tracing(dir: &Path) -> Result<Option<WorkerGuard>> {
    let Ok(filter) = std::env::var("LAZYDATABRICKS_LOG") else {
        return Ok(None);
    };
    std::fs::create_dir_all(dir)?;
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never(dir, "lazydatabricks.log"));
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&filter)?)
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Ok(Some(guard))
}

/// One jobs fetch, reported back as a message. Never touches `App`.
async fn fetch_jobs(client: Arc<api::Client>, tx: mpsc::Sender<Message>, max: usize) {
    let message = match client.list_jobs(max).await {
        Ok(jobs) => Message::JobsLoaded(jobs),
        Err(error) => Message::JobsFailed(error),
    };
    // A closed channel means the app already quit; nobody is left to tell.
    let _ = tx.send(message).await;
}

async fn fetch_recent_runs(client: Arc<api::Client>, tx: mpsc::Sender<Message>, max: usize) {
    let message = match client.list_recent_runs(max).await {
        Ok(runs) => Message::RecentRunsLoaded(runs),
        Err(error) => Message::RecentRunsFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn fetch_pipelines(client: Arc<api::Client>, tx: mpsc::Sender<Message>, max: usize) {
    let message = match client.list_pipelines(max).await {
        Ok(pipelines) => Message::PipelinesLoaded(pipelines),
        Err(error) => Message::PipelinesFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn fetch_clusters(client: Arc<api::Client>, tx: mpsc::Sender<Message>, max: usize) {
    let message = match client.list_clusters(max).await {
        Ok(clusters) => Message::ClustersLoaded(clusters),
        Err(error) => Message::ClustersFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn start_cluster(client: Arc<api::Client>, tx: mpsc::Sender<Message>, cluster_id: String) {
    let message = match client.start_cluster(&cluster_id).await {
        Ok(()) => Message::ClusterStarted { cluster_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn terminate_cluster(
    client: Arc<api::Client>,
    tx: mpsc::Sender<Message>,
    cluster_id: String,
) {
    let message = match client.terminate_cluster(&cluster_id).await {
        Ok(()) => Message::ClusterTerminated { cluster_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

/// Who the token belongs to, for the "mine" filter.
async fn fetch_me(client: Arc<api::Client>, tx: mpsc::Sender<Message>) {
    let message = match client.me().await {
        Ok(email) => Message::MeLoaded(email),
        Err(error) => Message::MeFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn fetch_runs(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64) {
    let message = match client.list_runs(job_id).await {
        Ok(runs) => Message::RunsLoaded { job_id, runs },
        Err(error) => Message::RunsFailed { job_id, error },
    };
    let _ = tx.send(message).await;
}

async fn fetch_run_detail(client: Arc<api::Client>, tx: mpsc::Sender<Message>, run_id: i64) {
    let message = match client.get_run(run_id).await {
        Ok(run) => Message::RunDetailLoaded(run),
        Err(error) => Message::RunDetailFailed { run_id, error },
    };
    let _ = tx.send(message).await;
}

async fn fetch_job(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64) {
    let message = match client.get_job(job_id).await {
        Ok(job) => Message::JobLoaded(job),
        Err(error) => Message::JobFailed { job_id, error },
    };
    let _ = tx.send(message).await;
}

async fn fetch_run_output(client: Arc<api::Client>, tx: mpsc::Sender<Message>, run_id: i64) {
    let message = match client.get_run_output(run_id).await {
        Ok(output) => Message::RunOutputLoaded { run_id, output },
        Err(error) => Message::RunOutputFailed { run_id, error },
    };
    let _ = tx.send(message).await;
}

async fn run_now(
    client: Arc<api::Client>,
    tx: mpsc::Sender<Message>,
    job_id: i64,
    params: BTreeMap<String, String>,
) {
    let message = match client.run_now(job_id, &params).await {
        Ok(run_id) => Message::RunStarted { job_id, run_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn repair_run(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64, run_id: i64) {
    let message = match client.repair_run(run_id).await {
        Ok(()) => Message::RunRepaired { job_id, run_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn cancel_run(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64, run_id: i64) {
    let message = match client.cancel_run(run_id).await {
        Ok(()) => Message::RunCancelled { job_id, run_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn start_update(client: Arc<api::Client>, tx: mpsc::Sender<Message>, pipeline_id: String) {
    let message = match client.start_update(&pipeline_id).await {
        Ok(update_id) => Message::UpdateStarted {
            pipeline_id,
            update_id,
        },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

async fn stop_pipeline(client: Arc<api::Client>, tx: mpsc::Sender<Message>, pipeline_id: String) {
    let message = match client.stop_pipeline(&pipeline_id).await {
        Ok(()) => Message::PipelineStopped { pipeline_id },
        Err(error) => Message::ActionFailed(error),
    };
    let _ = tx.send(message).await;
}

/// Runs a desktop hand-off (browser, clipboard) off the async threads; only failures are worth a
/// message, the notice for success was already shown optimistically.
async fn desktop(
    tx: mpsc::Sender<Message>,
    work: impl FnOnce() -> Result<(), AppError> + Send + 'static,
) {
    let result = match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => Err(AppError::Internal(error.to_string())),
    };
    if let Err(error) = result {
        let _ = tx.send(Message::ActionFailed(error)).await;
    }
}

/// Reads terminal events on a plain thread, since crossterm's reader blocks. Sends a `Tick`
/// every `TICK` of wall-clock time, keys or no keys, so the app's tick count stays a usable
/// clock. Exits when the channel closes or the terminal read fails.
fn spawn_input(tx: mpsc::Sender<Message>) {
    std::thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let until_tick = TICK.saturating_sub(last_tick.elapsed());
            let Ok(key) = read_key(until_tick) else {
                return;
            };
            if let Some(message) = key
                && tx.blocking_send(message).is_err()
            {
                return;
            }
            if last_tick.elapsed() >= TICK {
                last_tick = Instant::now();
                if tx.blocking_send(Message::Tick).is_err()
                    || tx.blocking_send(Message::Clock(Timestamp::now())).is_err()
                {
                    return;
                }
            }
        }
    });
}

/// Waits up to `timeout` for a terminal event and converts a key press into a `Message`.
/// `None` for timeouts and events the app has no use for.
fn read_key(timeout: std::time::Duration) -> Result<Option<Message>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    let Event::Key(key) = event::read()? else {
        return Ok(None);
    };
    // Windows reports key releases too; acting on both would double every key press.
    if key.kind != KeyEventKind::Press {
        return Ok(None);
    }
    let key = match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => Key::Ctrl(c),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Tab => Key::Tab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        _ => return Ok(None),
    };
    Ok(Some(Message::Key(key)))
}
