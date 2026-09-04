//! Serde mirrors of the Databricks REST shapes. Only the fields we use, and `#[serde(default)]`
//! on everything optional, because Databricks omits empty fields rather than nulling them.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Deserializer};

/// `GET /api/2.2/jobs/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JobsList {
    #[serde(default)]
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One job. Ids are `i64` end to end and are never used as indices.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Job {
    #[serde(rename = "job_id")]
    pub id: i64,
    #[serde(default)]
    pub creator_user_name: String,
    #[serde(default)]
    pub run_as_user_name: String,
    #[serde(default)]
    pub settings: JobSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct JobSettings {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
    #[serde(default)]
    pub max_concurrent_runs: Option<u32>,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default)]
    pub format: Option<String>,
}

/// `GET /api/2.2/jobs/runs/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RunsList {
    #[serde(default)]
    pub runs: Vec<Run>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Run {
    #[serde(rename = "run_id")]
    pub id: i64,
    #[serde(default)]
    pub job_id: i64,
    #[serde(default)]
    pub state: RunState,
    #[serde(default, deserialize_with = "epoch_millis")]
    pub start_time: Option<Timestamp>,
    #[serde(default, deserialize_with = "epoch_millis")]
    pub end_time: Option<Timestamp>,
}

impl Run {
    /// What a run we just started looks like before Databricks tells us anything about it.
    #[must_use]
    pub fn placeholder(job_id: i64, run_id: i64) -> Self {
        Self {
            id: run_id,
            job_id,
            state: RunState {
                life_cycle_state: LifeCycleState::Pending,
                result_state: None,
                state_message: "just started".to_owned(),
            },
            start_time: None,
            end_time: None,
        }
    }
}

/// `POST /api/2.2/jobs/run-now` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RunNowResponse {
    pub run_id: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RunState {
    #[serde(default)]
    pub life_cycle_state: LifeCycleState,
    #[serde(default)]
    pub result_state: Option<ResultState>,
    #[serde(default)]
    pub state_message: String,
}

/// Closed set in practice, open in the API: anything new lands in `Unknown` instead of
/// breaking the parse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifeCycleState {
    Queued,
    Pending,
    Running,
    Terminating,
    Terminated,
    Skipped,
    InternalError,
    Blocked,
    Waiting,
    #[default]
    #[serde(other)]
    Unknown,
}

impl LifeCycleState {
    /// Still going, so it can be cancelled.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued
                | Self::Pending
                | Self::Running
                | Self::Terminating
                | Self::Blocked
                | Self::Waiting
        )
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Terminating => "TERMINATING",
            Self::Terminated => "TERMINATED",
            Self::Skipped => "SKIPPED",
            Self::InternalError => "INTERNAL_ERROR",
            Self::Blocked => "BLOCKED",
            Self::Waiting => "WAITING",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResultState {
    Success,
    Failed,
    Timedout,
    Canceled,
    MaximumConcurrentRunsReached,
    ExcludedFromRun,
    SuccessWithFailures,
    UpstreamFailed,
    UpstreamCanceled,
    Disabled,
    #[serde(other)]
    Unknown,
}

impl ResultState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Timedout => "TIMEDOUT",
            Self::Canceled => "CANCELED",
            Self::MaximumConcurrentRunsReached => "MAX_CONCURRENT",
            Self::ExcludedFromRun => "EXCLUDED",
            Self::SuccessWithFailures => "SUCCESS_WITH_FAILURES",
            Self::UpstreamFailed => "UPSTREAM_FAILED",
            Self::UpstreamCanceled => "UPSTREAM_CANCELED",
            Self::Disabled => "DISABLED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// `GET /api/2.0/pipelines` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PipelinesList {
    #[serde(default)]
    pub statuses: Vec<Pipeline>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One Lakeflow / Delta Live Tables pipeline. Ids are UUID strings here, not integers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pipeline {
    #[serde(rename = "pipeline_id")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: PipelineState,
    #[serde(default)]
    pub creator_user_name: String,
    /// The most recent updates, newest first. Omitted entirely for pipelines never run.
    #[serde(default)]
    pub latest_updates: Vec<PipelineUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PipelineUpdate {
    #[serde(rename = "update_id")]
    pub id: String,
    #[serde(default)]
    pub state: UpdateState,
    /// RFC 3339 here, unlike the epoch millis on runs. jiff's serde handles it.
    #[serde(default)]
    pub creation_time: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PipelineState {
    Deploying,
    Starting,
    Running,
    Stopping,
    Deleted,
    Recovering,
    Failed,
    Resetting,
    Idle,
    #[default]
    #[serde(other)]
    Unknown,
}

impl PipelineState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deploying => "DEPLOYING",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Stopping => "STOPPING",
            Self::Deleted => "DELETED",
            Self::Recovering => "RECOVERING",
            Self::Failed => "FAILED",
            Self::Resetting => "RESETTING",
            Self::Idle => "IDLE",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdateState {
    Queued,
    Created,
    WaitingForResources,
    Initializing,
    Resetting,
    SettingUpTables,
    Running,
    Stopping,
    Completed,
    Failed,
    Canceled,
    #[default]
    #[serde(other)]
    Unknown,
}

impl UpdateState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Created => "CREATED",
            Self::WaitingForResources => "WAITING_FOR_RESOURCES",
            Self::Initializing => "INITIALIZING",
            Self::Resetting => "RESETTING",
            Self::SettingUpTables => "SETTING_UP_TABLES",
            Self::Running => "RUNNING",
            Self::Stopping => "STOPPING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Canceled => "CANCELED",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Finished, one way or another.
    #[must_use]
    pub const fn is_done(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }
}

/// `GET /api/2.0/preview/scim/v2/Me`: who the token belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ScimMe {
    #[serde(rename = "userName")]
    pub user_name: String,
}

/// Databricks sends epoch milliseconds and uses `0` for "not yet". Converted here, once, so no
/// raw millis reach the UI.
fn epoch_millis<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Timestamp>, D::Error> {
    let millis = Option::<i64>::deserialize(deserializer)?;
    millis
        .filter(|ms| *ms > 0)
        .map(Timestamp::from_millisecond)
        .transpose()
        .map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOBS_LIST: &str = include_str!("../../tests/fixtures/jobs_list.json");
    const RUNS_LIST: &str = include_str!("../../tests/fixtures/runs_list.json");
    const SCIM_ME: &str = include_str!("../../tests/fixtures/scim_me.json");
    const PIPELINES_LIST: &str = include_str!("../../tests/fixtures/pipelines_list.json");

    #[test]
    fn parses_jobs_list_fixture() {
        let page: JobsList = serde_json::from_str(JOBS_LIST).unwrap();
        assert_eq!(page.jobs.len(), 2);
        let first = &page.jobs[0];
        assert_eq!(first.id, 1_025_322_370_191_789);
        assert_eq!(first.settings.name, "[someone] okonomi_gold");
        assert_eq!(first.creator_user_name, "someone@example.com");
        assert_eq!(first.settings.timeout_seconds, Some(7200));
        assert_eq!(first.settings.max_concurrent_runs, Some(4));
        assert_eq!(first.settings.tags["domain"], "okonomi");
        assert_eq!(first.settings.format.as_deref(), Some("MULTI_TASK"));
        assert_eq!(page.jobs[1].settings.timeout_seconds, None);
        assert!(page.jobs[1].settings.tags.is_empty());
        assert_eq!(
            page.next_page_token.as_deref(),
            Some("CAIo0JeenYM0Sg80MzEwMTU5NzM2MjUyMDA=")
        );
    }

    #[test]
    fn missing_fields_default() {
        let page: JobsList = serde_json::from_str(r#"{"jobs":[{"job_id":7}]}"#).unwrap();
        assert_eq!(page.jobs[0].settings.name, "");
        assert_eq!(page.next_page_token, None);
    }

    #[test]
    fn empty_object_is_empty_page() {
        let page: JobsList = serde_json::from_str("{}").unwrap();
        assert!(page.jobs.is_empty());
    }

    #[test]
    fn parses_runs_list_fixture() {
        let page: RunsList = serde_json::from_str(RUNS_LIST).unwrap();
        assert_eq!(page.runs.len(), 3);
        let ok = &page.runs[0];
        assert_eq!(ok.id, 50_851_892_761_073);
        assert_eq!(ok.state.life_cycle_state, LifeCycleState::Terminated);
        assert_eq!(ok.state.result_state, Some(ResultState::Success));
        assert_eq!(ok.start_time.unwrap().as_millisecond(), 1_788_170_893_271);
        assert_eq!(ok.end_time.unwrap().as_millisecond(), 1_788_170_965_431);
        let running = &page.runs[2];
        assert_eq!(running.state.life_cycle_state, LifeCycleState::Running);
        assert_eq!(running.state.result_state, None);
        assert_eq!(running.end_time, None, "end_time 0 means not finished");
    }

    #[test]
    fn parses_pipelines_list_fixture() {
        let page: PipelinesList = serde_json::from_str(PIPELINES_LIST).unwrap();
        assert_eq!(page.statuses.len(), 3);
        let never_run = &page.statuses[0];
        assert_eq!(never_run.id, "0120d44b-406a-42a6-b072-5796077af583");
        assert_eq!(never_run.state, PipelineState::Idle);
        assert!(never_run.latest_updates.is_empty());
        let felles = &page.statuses[1];
        assert_eq!(felles.name, "[someone] felles_gold");
        assert_eq!(felles.latest_updates[0].state, UpdateState::Completed);
        assert_eq!(
            felles.latest_updates[0].creation_time.unwrap().to_string(),
            "2026-08-24T14:50:43.651Z"
        );
        assert_eq!(felles.latest_updates[1].state, UpdateState::Failed);
        assert_eq!(page.statuses[2].state, PipelineState::Running);
        assert_eq!(page.next_page_token.as_deref(), Some("CAEQ"));
    }

    #[test]
    fn parses_scim_me_fixture() {
        let me: ScimMe = serde_json::from_str(SCIM_ME).unwrap();
        assert_eq!(me.user_name, "someone.example@example.com");
    }

    #[test]
    fn unknown_states_do_not_break_parsing() {
        let run: Run = serde_json::from_str(
            r#"{"run_id":1,"state":{"life_cycle_state":"BRAND_NEW","result_state":"SHRUG"}}"#,
        )
        .unwrap();
        assert_eq!(run.state.life_cycle_state, LifeCycleState::Unknown);
        assert_eq!(run.state.result_state, Some(ResultState::Unknown));
        assert_eq!(run.start_time, None);
    }
}
