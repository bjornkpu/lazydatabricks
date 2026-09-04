//! lazydatabricks: a lazygit-style TUI for Databricks.
//!
//! This file is wiring only. It owns the terminal, converts terminal events into `Message`s and
//! runs the draw/update loop. Nothing outside this file imports crossterm.

mod api;
mod app;
mod ui;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::api::models::Job;
use crate::app::{App, Flow, Key, Message};

/// Upper bound on jobs fetched across pages. Becomes config at M7.
const MAX_JOBS: usize = 200;

fn main() -> Result<()> {
    let profile =
        std::env::var("DATABRICKS_CONFIG_PROFILE").unwrap_or_else(|_| "DEFAULT".to_owned());
    // M1: one blocking fetch before the terminal is taken over, so errors print to a normal
    // shell. M2 moves this into a task that sends `Message::JobsLoaded`.
    let jobs = api::Client::from_profile(&profile)?.list_jobs(MAX_JOBS)?;
    // `ratatui::run` enters the alternate screen and raw mode, installs a panic hook that
    // restores both, and restores again on normal return. `clippy::exit` is denied, so the loop
    // returns instead of exiting; that is what lets the restore run.
    ratatui::run(|terminal| run(terminal, jobs))
}

fn run(terminal: &mut DefaultTerminal, jobs: Vec<Job>) -> Result<()> {
    let mut app = App::default();
    app.update(Message::JobsLoaded(jobs));
    loop {
        terminal.draw(|frame| ui::draw(&app, frame))?;
        let Some(message) = read_message()? else {
            continue;
        };
        match app.update(message) {
            Flow::Continue => {}
            Flow::Quit => return Ok(()),
        }
    }
}

/// Blocks for the next terminal event and converts it to a `Message`. `None` for events the app
/// has no use for.
fn read_message() -> Result<Option<Message>> {
    let Event::Key(key) = event::read()? else {
        return Ok(None);
    };
    // Windows reports key releases too; acting on both would double every key press.
    if key.kind != KeyEventKind::Press {
        return Ok(None);
    }
    let key = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        _ => return Ok(None),
    };
    Ok(Some(Message::Key(key)))
}
