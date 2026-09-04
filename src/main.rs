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

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, bail};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::{App, Command, Key, Message, TICK};
use crate::error::AppError;

/// Messages buffered between producers and the update loop.
const CHANNEL_CAPACITY: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let mut loaded = config::load()?;
    // Flags beat environment beat config beat the CLI's own default profile name.
    let profile = cli
        .profile
        .or_else(|| std::env::var("DATABRICKS_CONFIG_PROFILE").ok())
        .or_else(|| loaded.config.profile.clone())
        .unwrap_or_else(|| "DEFAULT".to_owned());
    loaded.config.allow_actions |= cli.allow_actions;
    if cli.filter.is_some() {
        loaded.config.filter = cli.filter;
    }
    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    // Auth before the terminal is taken over: the CLI may print or open a browser, and its
    // errors should land in a normal shell.
    let client = Arc::new(api::Client::from_profile(&profile, tx.clone())?);
    let app = App::new(&profile, client.host(), TimeZone::system(), &loaded);
    tokio::spawn(fetch_jobs(Arc::clone(&client), tx.clone(), app.max_jobs));
    tokio::spawn(fetch_recent_runs(
        Arc::clone(&client),
        tx.clone(),
        app.max_jobs,
    ));
    tokio::spawn(fetch_pipelines(
        Arc::clone(&client),
        tx.clone(),
        app.max_jobs,
    ));
    tokio::spawn(fetch_me(Arc::clone(&client), tx.clone()));
    spawn_input(tx.clone());
    // `ratatui::init` enters the alternate screen and raw mode and installs a panic hook that
    // restores both. `clippy::exit` is denied, so the loop returns instead of exiting; that is
    // what lets `restore` run on the error path too.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, app, rx, client, tx).await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut DefaultTerminal,
    mut app: App,
    mut rx: mpsc::Receiver<Message>,
    client: Arc<api::Client>,
    tx: mpsc::Sender<Message>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(&app, frame))?;
        let Some(message) = rx.recv().await else {
            bail!("all message producers stopped");
        };
        for command in app.update(message) {
            match command {
                Command::Quit => return Ok(()),
                Command::FetchJobs { max } => {
                    tokio::spawn(fetch_jobs(Arc::clone(&client), tx.clone(), max));
                }
                Command::FetchRecentRuns { max } => {
                    tokio::spawn(fetch_recent_runs(Arc::clone(&client), tx.clone(), max));
                }
                Command::FetchPipelines { max } => {
                    tokio::spawn(fetch_pipelines(Arc::clone(&client), tx.clone(), max));
                }
                Command::FetchRuns { job_id } => {
                    tokio::spawn(fetch_runs(Arc::clone(&client), tx.clone(), job_id));
                }
                Command::FetchRunDetail { run_id } => {
                    tokio::spawn(fetch_run_detail(Arc::clone(&client), tx.clone(), run_id));
                }
                Command::RunNow { job_id } => {
                    tokio::spawn(run_now(Arc::clone(&client), tx.clone(), job_id));
                }
                Command::CancelRun { job_id, run_id } => {
                    tokio::spawn(cancel_run(Arc::clone(&client), tx.clone(), job_id, run_id));
                }
                Command::OpenUrl(url) => {
                    tokio::spawn(desktop(tx.clone(), move || shell::open_url(&url)));
                }
                Command::Copy(text) => {
                    tokio::spawn(desktop(tx.clone(), move || shell::copy(&text)));
                }
            }
        }
    }
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

async fn run_now(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64) {
    let message = match client.run_now(job_id).await {
        Ok(run_id) => Message::RunStarted { job_id, run_id },
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
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Key::CtrlC,
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
