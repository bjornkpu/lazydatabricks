//! lazydatabricks: a lazygit-style TUI for Databricks.
//!
//! This file is wiring only. It owns the terminal, converts terminal events into `Message`s,
//! spawns fetch tasks for the `Command`s `update` returns, and runs the draw/update loop.
//! Nothing outside this file imports crossterm.

mod api;
mod app;
mod ui;

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use jiff::tz::TimeZone;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::{App, Command, Key, Message, TICK};

/// Upper bound on jobs fetched across pages. Becomes config at M7.
const MAX_JOBS: usize = 200;
/// Messages buffered between producers and the update loop.
const CHANNEL_CAPACITY: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    let profile =
        std::env::var("DATABRICKS_CONFIG_PROFILE").unwrap_or_else(|_| "DEFAULT".to_owned());
    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    // Auth before the terminal is taken over: the CLI may print or open a browser, and its
    // errors should land in a normal shell.
    let client = Arc::new(api::Client::from_profile(&profile, tx.clone())?);
    let app = App::new(&profile, client.host(), TimeZone::system());
    tokio::spawn(fetch_jobs(Arc::clone(&client), tx.clone()));
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
                Command::FetchJobs => {
                    tokio::spawn(fetch_jobs(Arc::clone(&client), tx.clone()));
                }
                Command::FetchRuns { job_id } => {
                    tokio::spawn(fetch_runs(Arc::clone(&client), tx.clone(), job_id));
                }
            }
        }
    }
}

/// One jobs fetch, reported back as a message. Never touches `App`.
async fn fetch_jobs(client: Arc<api::Client>, tx: mpsc::Sender<Message>) {
    let message = match client.list_jobs(MAX_JOBS).await {
        Ok(jobs) => Message::JobsLoaded(jobs),
        Err(error) => Message::JobsFailed(format!("{error:#}")),
    };
    // A closed channel means the app already quit; nobody is left to tell.
    let _ = tx.send(message).await;
}

/// Who the token belongs to, for the "mine" filter.
async fn fetch_me(client: Arc<api::Client>, tx: mpsc::Sender<Message>) {
    let message = match client.me().await {
        Ok(email) => Message::MeLoaded(email),
        Err(error) => Message::MeFailed(format!("{error:#}")),
    };
    let _ = tx.send(message).await;
}

async fn fetch_runs(client: Arc<api::Client>, tx: mpsc::Sender<Message>, job_id: i64) {
    let message = match client.list_runs(job_id).await {
        Ok(runs) => Message::RunsLoaded { job_id, runs },
        Err(error) => Message::RunsFailed {
            job_id,
            error: format!("{error:#}"),
        },
    };
    let _ = tx.send(message).await;
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
                if tx.blocking_send(Message::Tick).is_err() {
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
