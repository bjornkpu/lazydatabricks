//! Everything that can happen to the app, as data. The terminal library's event types stop at
//! `main`; the rest of the program only sees these.

use std::time::Duration;

use serde::Deserialize;

use crate::api::models::{Job, Run};
use crate::error::AppError;

/// A key press, decoupled from the terminal library. Parsed from config via `FromStr` in
/// `keys.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
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
    JobsFailed(AppError),
    RunsLoaded {
        job_id: i64,
        runs: Vec<Run>,
    },
    RunsFailed {
        job_id: i64,
        error: AppError,
    },
    ApiCalled(ApiCall),
    /// Who the token belongs to, as an email.
    MeLoaded(String),
    MeFailed(AppError),
}

/// Side effects `update` asks `main` to perform. `update` itself never does IO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    FetchJobs { max: usize },
    FetchRuns { job_id: i64 },
}
