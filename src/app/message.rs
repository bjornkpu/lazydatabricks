//! Everything that can happen to the app, as data. The terminal library's event types stop at
//! `main`; the rest of the program only sees these.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::custom::Output;
use crate::api::models::{Cluster, Job, Pipeline, Run, RunOutput};
use crate::error::AppError;

/// A key press, decoupled from the terminal library. Parsed from config via `FromStr` in
/// `keys.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
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
    Home,
    End,
    /// A letter with Control held, `ctrl+d` in config.
    Ctrl(char),
}

/// One REST call, as shown in the API log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiCall {
    pub method: String,
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
    /// Wall-clock time, sent alongside ticks so relative ages can be rendered without IO.
    Clock(jiff::Timestamp),
    JobsLoaded(Vec<Job>),
    JobsFailed(AppError),
    /// `jobs/get` for the job whose Detail tab is open.
    JobLoaded(Job),
    JobFailed {
        job_id: i64,
        error: AppError,
    },
    PipelinesLoaded(Vec<Pipeline>),
    PipelinesFailed(AppError),
    /// `pipelines/get` for the pipeline whose JSON tab is open, as sent.
    PipelineLoaded {
        pipeline_id: String,
        spec: serde_json::Value,
    },
    PipelineFailed {
        pipeline_id: String,
        error: AppError,
    },
    ComputeLoaded(Vec<Cluster>),
    ComputeFailed(AppError),
    /// A cluster or warehouse start was accepted; `cluster_id` is the compute row's id.
    ClusterStarted {
        cluster_id: String,
    },
    /// A cluster terminate or warehouse stop was accepted.
    ClusterTerminated {
        cluster_id: String,
    },
    /// Newest runs across the workspace, for the age and glyph on each job row.
    RecentRunsLoaded(Vec<Run>),
    RecentRunsFailed(AppError),
    RunsLoaded {
        job_id: i64,
        runs: Vec<Run>,
    },
    RunsFailed {
        job_id: i64,
        error: AppError,
    },
    /// `runs/get` for the run being viewed.
    RunDetailLoaded(Run),
    RunDetailFailed {
        run_id: i64,
        error: AppError,
    },
    /// `runs/get-output` for one failed task; `run_id` is the task's.
    RunOutputLoaded {
        run_id: i64,
        output: RunOutput,
    },
    RunOutputFailed {
        run_id: i64,
        error: AppError,
    },
    ApiCalled(ApiCall),
    /// A popup custom command finished: what it printed, or why it could not run.
    ShellFinished {
        name: String,
        output: Result<String, AppError>,
    },
    /// A terminal custom command finished and the TUI is back; `detail` is its exit status.
    ShellExited {
        name: String,
        detail: String,
    },
    /// The draw found the main panel's text this many lines taller than its viewport. The
    /// scroll clamps to it; `update` cannot know the terminal size on its own.
    ScrollLimit(usize),
    /// The lines of the main panel's text that match the search, from the draw, for `n`/`N`.
    Matches(Vec<usize>),
    /// `--job`: put the cursor on the first job whose name contains this, now or when the list
    /// arrives.
    SelectJob(String),
    /// The newest released version from GitHub, or why it could not be read.
    UpdateChecked(Result<String, AppError>),
    /// Who the token belongs to, as an email.
    MeLoaded(String),
    MeFailed(AppError),
    /// `run-now` accepted; the run exists but nothing is known about it yet.
    RunStarted {
        job_id: i64,
        run_id: i64,
    },
    RunCancelled {
        job_id: i64,
        run_id: i64,
    },
    /// `jobs/delete` accepted; the job is gone.
    JobDeleted {
        job_id: i64,
    },
    /// `jobs/update` accepted the new pause status.
    SchedulePaused {
        job_id: i64,
        paused: bool,
    },
    /// `runs/repair` accepted; the failed tasks run again inside the same run id.
    RunRepaired {
        job_id: i64,
        run_id: i64,
    },
    UpdateStarted {
        pipeline_id: String,
        update_id: String,
    },
    PipelineStopped {
        pipeline_id: String,
    },
    ActionFailed(AppError),
}

/// Side effects `update` asks `main` to perform. `update` itself never does IO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Quit,
    FetchJobs {
        max: usize,
    },
    FetchRecentRuns {
        max: usize,
    },
    FetchPipelines {
        max: usize,
    },
    FetchCompute {
        max: usize,
    },
    StartCluster {
        cluster_id: String,
    },
    TerminateCluster {
        cluster_id: String,
    },
    StartWarehouse {
        warehouse_id: String,
    },
    StopWarehouse {
        warehouse_id: String,
    },
    FetchRuns {
        job_id: i64,
    },
    FetchRunDetail {
        run_id: i64,
    },
    /// Error and traceback of one task run.
    FetchRunOutput {
        run_id: i64,
    },
    /// Full settings of one job, for the Detail and JSON tabs.
    FetchJob {
        job_id: i64,
    },
    /// Full spec of one pipeline, for its JSON tab.
    FetchPipeline {
        pipeline_id: String,
    },
    /// `jobs/delete`, after the name was typed back.
    DeleteJob {
        job_id: i64,
    },
    /// Pause or resume a job's schedule via `jobs/update`.
    SetSchedulePaused {
        job_id: i64,
        paused: bool,
    },
    /// `run-now`, with `job_parameters` when `params` is not empty.
    RunNow {
        job_id: i64,
        params: BTreeMap<String, String>,
    },
    /// Re-run the failed tasks of a finished run.
    RepairRun {
        job_id: i64,
        run_id: i64,
    },
    CancelRun {
        job_id: i64,
        run_id: i64,
    },
    StartUpdate {
        pipeline_id: String,
    },
    StopPipeline {
        pipeline_id: String,
    },
    /// Hand a URL to the browser.
    OpenUrl(String),
    /// Put text on the clipboard.
    Copy(String),
    /// Put the focused panel's visible rows on the clipboard, as text. `main` renders them.
    CopyVisible,
    /// Ring the terminal bell: something on screen just failed.
    Bell,
    /// Show this profile's workspace, opening it first if it is not open. `main` owns the
    /// workspaces; `App` only asks.
    SwitchProfile(String),
    /// Show text in `$PAGER`; `main` steps out of the TUI for it.
    Page(String),
    /// Open the config file in `$EDITOR`; `main` knows the path and steps out of the TUI.
    EditConfig,
    /// Ask GitHub for the newest release.
    CheckUpdate,
    /// Run a custom command, placeholders already expanded. `Terminal` output means `main`
    /// hands over the screen; `Popup` captures and replies with `ShellFinished`.
    Shell {
        name: String,
        command: String,
        output: Output,
    },
}
