//! lazydatabricks: a lazygit-style TUI for Databricks.
//!
//! This file is wiring only. It owns the terminal, converts terminal events into `Message`s,
//! spawns the fetch task and runs the draw/update loop. Nothing outside this file imports
//! crossterm.

mod api;
mod app;
mod ui;

use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::app::{App, Flow, Key, Message};

/// Upper bound on jobs fetched across pages. Becomes config at M7.
const MAX_JOBS: usize = 200;
/// How long the input thread waits for a key before sending a `Tick`; also the spinner rate.
const TICK: Duration = Duration::from_millis(100);
/// Messages buffered between producers and the update loop.
const CHANNEL_CAPACITY: usize = 64;

#[tokio::main]
async fn main() -> Result<()> {
    let profile =
        std::env::var("DATABRICKS_CONFIG_PROFILE").unwrap_or_else(|_| "DEFAULT".to_owned());
    // Auth before the terminal is taken over: the CLI may print or open a browser, and its
    // errors should land in a normal shell.
    let client = api::Client::from_profile(&profile)?;
    let app = App::new(&profile, client.host());
    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    tokio::spawn(fetch_jobs(client, tx.clone()));
    spawn_input(tx);
    // `ratatui::init` enters the alternate screen and raw mode and installs a panic hook that
    // restores both. `clippy::exit` is denied, so the loop returns instead of exiting; that is
    // what lets `restore` run on the error path too.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, app, rx).await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut DefaultTerminal,
    mut app: App,
    mut rx: mpsc::Receiver<Message>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(&app, frame))?;
        let Some(message) = rx.recv().await else {
            bail!("input thread stopped");
        };
        match app.update(message) {
            Flow::Continue => {}
            Flow::Quit => return Ok(()),
        }
    }
}

/// One fetch, reported back as a message. Never touches `App`.
async fn fetch_jobs(client: api::Client, tx: mpsc::Sender<Message>) {
    let message = match client.list_jobs(MAX_JOBS).await {
        Ok(jobs) => Message::JobsLoaded(jobs),
        Err(error) => Message::JobsFailed(format!("{error:#}")),
    };
    // A closed channel means the app already quit; nobody is left to tell.
    let _ = tx.send(message).await;
}

/// Reads terminal events on a plain thread, since crossterm's reader blocks. Sends a `Tick`
/// whenever `TICK` passes without a key. Exits when the channel closes or the terminal read
/// fails; `run` then sees a closed channel and bails.
fn spawn_input(tx: mpsc::Sender<Message>) {
    std::thread::spawn(move || {
        while let Ok(message) = next_message() {
            if let Some(message) = message
                && tx.blocking_send(message).is_err()
            {
                return;
            }
        }
    });
}

/// Blocks for at most `TICK`, then converts what happened into a `Message`. `None` for events
/// the app has no use for.
fn next_message() -> Result<Option<Message>> {
    if !event::poll(TICK)? {
        return Ok(Some(Message::Tick));
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
        KeyCode::Enter => Key::Enter,
        _ => return Ok(None),
    };
    Ok(Some(Message::Key(key)))
}
