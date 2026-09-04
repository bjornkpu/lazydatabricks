//! Everything that can happen to the app, as data. The terminal library's event types stop at
//! `main`; the rest of the program only sees these.

use crate::api::models::Job;

/// A key press, decoupled from the terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
}

/// One thing that happened. `App::update` folds these into state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Key(Key),
    /// Periodic heartbeat from the input thread; drives the spinner.
    Tick,
    JobsLoaded(Vec<Job>),
    /// The fetch failed. The string is the full error chain, ready to display.
    JobsFailed(String),
}
