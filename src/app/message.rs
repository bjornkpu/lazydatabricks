//! Everything that can happen to the app, as data. The terminal library's event types stop at
//! `main`; the rest of the program only sees these.

use std::time::Duration;

use crate::api::models::{Job, Run};

/// A key press, decoupled from the terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Tab,
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    CtrlC,
}

/// One REST call, as shown in the API log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCall {
    pub method: &'static str,
    /// Path and query exactly as sent.
    pub path: String,
    /// `None` when the request never got a response.
    pub status: Option<u16>,
    pub duration: Duration,
}

/// One thing that happened. `App::update` folds these into state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Key(Key),
    /// Periodic heartbeat from the input thread; drives the spinner and debounces fetches.
    Tick,
    JobsLoaded(Vec<Job>),
    /// The fetch failed. The string is the full error chain, ready to display.
    JobsFailed(String),
    RunsLoaded {
        job_id: i64,
        runs: Vec<Run>,
    },
    RunsFailed {
        job_id: i64,
        error: String,
    },
    ApiCalled(ApiCall),
    /// Who the token belongs to, as an email.
    MeLoaded(String),
    MeFailed(String),
}

/// Side effects `update` asks `main` to perform. `update` itself never does IO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    FetchJobs,
    FetchRuns { job_id: i64 },
}
