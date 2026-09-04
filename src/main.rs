//! lazydatabricks: a lazygit-style TUI for Databricks.
//!
//! This file is wiring only. It owns the terminal, converts terminal events into `Message`s and
//! runs the draw/update loop. Nothing outside this file imports crossterm.

mod app;
mod ui;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::{App, Flow, Key, Message};

fn main() -> Result<()> {
    // `ratatui::run` enters the alternate screen and raw mode, installs a panic hook that
    // restores both, and restores again on normal return. `clippy::exit` is denied, so the loop
    // returns instead of exiting; that is what lets the restore run.
    ratatui::run(run)
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::default();
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
