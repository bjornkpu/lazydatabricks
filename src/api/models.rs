//! Serde mirrors of the Databricks REST shapes. Only the fields we use, and `#[serde(default)]`
//! on everything optional, because Databricks omits empty fields rather than nulling them.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Deserializer, Serialize};

/// `GET /api/2.2/jobs/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct JobsList {
    #[serde(default)]
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One job. Ids are `i64` end to end and are never used as indices.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Job {
    #[serde(rename = "job_id")]
    pub id: i64,
    #[serde(default)]
    pub creator_user_name: String,
    #[serde(default)]
    pub run_as_user_name: String,
    #[serde(default, deserialize_with = "epoch_millis")]
    pub created_time: Option<Timestamp>,
    #[serde(default)]
    pub settings: JobSettings,
}

/// Job settings. The list response carries the first five fields; `jobs/get` fills the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
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
    #[serde(default)]
    pub schedule: Option<CronSchedule>,
    /// Who deployed it: `BUNDLE` with the bundle's metadata path, or nothing for the UI.
    #[serde(default)]
    pub deployment: Option<Deployment>,
    /// `UI_LOCKED` for bundle-managed jobs, `EDITABLE` otherwise.
    #[serde(default)]
    pub edit_mode: Option<String>,
    #[serde(default)]
    pub tasks: Vec<TaskSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CronSchedule {
    #[serde(default)]
    pub quartz_cron_expression: String,
    #[serde(default)]
    pub timezone_id: String,
    #[serde(default)]
    pub pause_status: Option<String>,
}

impl CronSchedule {
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.pause_status.as_deref() == Some("PAUSED")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Deployment {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub metadata_file_path: Option<String>,
}

/// One task as configured. Only the task types worth a line on screen are named; anything else
/// shows as its key alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct TaskSettings {
    #[serde(default)]
    pub task_key: String,
    #[serde(default)]
    pub notebook_task: Option<NotebookTask>,
    #[serde(default)]
    pub spark_python_task: Option<SparkPythonTask>,
    #[serde(default)]
    pub python_wheel_task: Option<PythonWheelTask>,
    #[serde(default)]
    pub pipeline_task: Option<PipelineTask>,
    #[serde(default)]
    pub run_job_task: Option<RunJobTask>,
    /// All-purpose cluster: the cost question every platform admin asks first.
    #[serde(default)]
    pub existing_cluster_id: Option<String>,
    #[serde(default)]
    pub job_cluster_key: Option<String>,
    /// Serverless environments.
    #[serde(default)]
    pub environment_key: Option<String>,
}

impl TaskSettings {
    /// `notebook /Repos/x/y`, `python src/main.py`, `wheel pkg:entry`, `pipeline <id>`,
    /// `job <id>`, or `-`.
    #[must_use]
    pub fn kind(&self) -> String {
        self.notebook_task
            .as_ref()
            .map(|task| format!("notebook {}", task.notebook_path))
            .or_else(|| {
                self.spark_python_task
                    .as_ref()
                    .map(|task| format!("python {}", task.python_file))
            })
            .or_else(|| {
                self.python_wheel_task
                    .as_ref()
                    .map(|task| format!("wheel {}:{}", task.package_name, task.entry_point))
            })
            .or_else(|| {
                self.pipeline_task
                    .as_ref()
                    .map(|task| format!("pipeline {}", task.pipeline_id))
            })
            .or_else(|| {
                self.run_job_task
                    .as_ref()
                    .map(|task| format!("job {}", task.job_id))
            })
            .unwrap_or_else(|| "-".to_owned())
    }

    /// Where it runs: `job cluster <key>`, `all-purpose <id>`, `serverless`, or `-`.
    #[must_use]
    pub fn cluster(&self) -> String {
        self.existing_cluster_id
            .as_ref()
            .map(|id| format!("all-purpose {id}"))
            .or_else(|| {
                self.job_cluster_key
                    .as_ref()
                    .map(|key| format!("job cluster {key}"))
            })
            .or_else(|| {
                self.environment_key
                    .as_ref()
                    .map(|_| "serverless".to_owned())
            })
            .unwrap_or_else(|| "-".to_owned())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct NotebookTask {
    #[serde(default)]
    pub notebook_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct SparkPythonTask {
    #[serde(default)]
    pub python_file: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PythonWheelTask {
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub entry_point: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PipelineTask {
    #[serde(default)]
    pub pipeline_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunJobTask {
    #[serde(default)]
    pub job_id: i64,
}

/// `GET /api/2.2/jobs/runs/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunsList {
    #[serde(default)]
    pub runs: Vec<Run>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    /// Only on `runs/get`; empty in list responses.
    #[serde(default, rename = "run_page_url")]
    pub page_url: String,
    /// Only on `runs/get`; empty in list responses.
    #[serde(default)]
    pub tasks: Vec<TaskRun>,
}

/// One task inside a run, from `runs/get`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TaskRun {
    /// The task's own run id, the one `runs/get-output` wants.
    #[serde(default)]
    pub run_id: i64,
    #[serde(default)]
    pub task_key: String,
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
            page_url: String::new(),
            tasks: Vec::new(),
        }
    }
}

/// `GET /api/2.2/jobs/runs/get-output` response, the parts that explain a failure. Notebook
/// output and logs are left out: the browser is the place to read a 1 MB stdout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunOutput {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_trace: Option<String>,
}

/// `POST /api/2.2/jobs/run-now` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunNowResponse {
    pub run_id: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunState {
    #[serde(default)]
    pub life_cycle_state: LifeCycleState,
    #[serde(default)]
    pub result_state: Option<ResultState>,
    #[serde(default)]
    pub state_message: String,
}

impl RunState {
    /// Finished with anything but success: failed, timed out, cancelled, skipped upstream.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self.result_state, Some(result) if !matches!(result, ResultState::Success))
    }
}

/// Closed set in practice, open in the API: anything new lands in `Unknown` instead of
/// breaking the parse.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

/// `GET /api/2.1/clusters/list` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ClustersList {
    #[serde(default)]
    pub clusters: Vec<Cluster>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// What a compute row is. Clusters come from `clusters/list`; SQL warehouses are folded into the
/// same shape so the panel, filter and menu have one type to deal with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum ComputeKind {
    #[default]
    Cluster,
    Warehouse,
}

/// One compute row: a cluster, all-purpose or job, or a SQL warehouse. Ids are strings like
/// `0901-071234-abcd1234`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Cluster {
    #[serde(default)]
    pub kind: ComputeKind,
    #[serde(rename = "cluster_id")]
    pub id: String,
    #[serde(default, rename = "cluster_name")]
    pub name: String,
    #[serde(default)]
    pub creator_user_name: String,
    /// `UI`, `JOB`, `API`, `PIPELINE`, ...
    #[serde(default, rename = "cluster_source")]
    pub source: String,
    #[serde(default)]
    pub state: ClusterState,
    #[serde(default)]
    pub state_message: String,
    #[serde(default)]
    pub spark_version: String,
    #[serde(default)]
    pub node_type_id: String,
    #[serde(default)]
    pub num_workers: Option<u32>,
    #[serde(default)]
    pub autoscale: Option<Autoscale>,
    #[serde(default, deserialize_with = "epoch_millis")]
    pub start_time: Option<Timestamp>,
}

impl Cluster {
    /// The state in the row's own vocabulary: a warehouse is `STOPPED`, not `TERMINATED`.
    #[must_use]
    pub const fn state_label(&self) -> &'static str {
        match (self.kind, self.state) {
            (ComputeKind::Warehouse, ClusterState::Terminated) => "STOPPED",
            (ComputeKind::Warehouse, ClusterState::Pending) => "STARTING",
            (ComputeKind::Warehouse, ClusterState::Terminating) => "STOPPING",
            _ => self.state.as_str(),
        }
    }

    /// `2`, `1-4` for autoscale, or `-`.
    #[must_use]
    pub fn workers(&self) -> String {
        self.autoscale.as_ref().map_or_else(
            || {
                self.num_workers
                    .map_or_else(|| "-".to_owned(), |n| n.to_string())
            },
            |scale| format!("{}-{}", scale.min_workers, scale.max_workers),
        )
    }
}

/// `GET /api/2.0/sql/warehouses` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WarehousesList {
    #[serde(default)]
    pub warehouses: Vec<Warehouse>,
}

/// A SQL warehouse, the one serverless thing with a visible warm or cold state.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Warehouse {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub creator_name: String,
    /// `STOPPED`, `STARTING`, `RUNNING`, `STOPPING`, `DELETING`, `DELETED`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub cluster_size: String,
    #[serde(default)]
    pub enable_serverless_compute: bool,
    #[serde(default)]
    pub auto_stop_mins: Option<u32>,
    #[serde(default)]
    pub num_clusters: Option<u32>,
}

impl From<Warehouse> for Cluster {
    /// Warehouse states map onto cluster states so one glyph function serves both.
    fn from(warehouse: Warehouse) -> Self {
        let state = match warehouse.state.as_str() {
            "RUNNING" => ClusterState::Running,
            "STARTING" => ClusterState::Pending,
            "STOPPING" | "DELETING" => ClusterState::Terminating,
            "STOPPED" | "DELETED" => ClusterState::Terminated,
            _ => ClusterState::Unknown,
        };
        let source = if warehouse.enable_serverless_compute {
            "SQL warehouse, serverless"
        } else {
            "SQL warehouse"
        };
        Self {
            kind: ComputeKind::Warehouse,
            id: warehouse.id,
            name: warehouse.name,
            creator_user_name: warehouse.creator_name,
            source: source.to_owned(),
            state,
            state_message: warehouse
                .auto_stop_mins
                .map_or_else(String::new, |mins| format!("auto-stop after {mins} min")),
            spark_version: String::new(),
            node_type_id: warehouse.cluster_size,
            num_workers: warehouse.num_clusters,
            autoscale: None,
            start_time: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Autoscale {
    #[serde(default)]
    pub min_workers: u32,
    #[serde(default)]
    pub max_workers: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClusterState {
    Pending,
    Running,
    Restarting,
    Resizing,
    Terminating,
    Terminated,
    Error,
    #[default]
    #[serde(other)]
    Unknown,
}

impl ClusterState {
    /// Up, or on its way up or down: something is billing.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Pending | Self::Running | Self::Restarting | Self::Resizing | Self::Terminating
        )
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Restarting => "RESTARTING",
            Self::Resizing => "RESIZING",
            Self::Terminating => "TERMINATING",
            Self::Terminated => "TERMINATED",
            Self::Error => "ERROR",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// `GET /api/2.0/pipelines` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PipelinesList {
    #[serde(default)]
    pub statuses: Vec<Pipeline>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// One Lakeflow / Delta Live Tables pipeline. Ids are UUID strings here, not integers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PipelineUpdate {
    #[serde(rename = "update_id")]
    pub id: String,
    #[serde(default)]
    pub state: UpdateState,
    /// RFC 3339 here, unlike the epoch millis on runs. jiff's serde handles it.
    #[serde(default)]
    pub creation_time: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
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
    /// An update is in progress, so the pipeline can be stopped.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Deploying
                | Self::Starting
                | Self::Running
                | Self::Stopping
                | Self::Recovering
                | Self::Resetting
        )
    }

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
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

/// `POST /api/2.0/pipelines/{id}/updates` response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UpdateStartResponse {
    pub update_id: String,
}

/// `GET /api/2.0/preview/scim/v2/Me`: who the token belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    const RUN_GET: &str = include_str!("../../tests/fixtures/run_get.json");
    const RUN_OUTPUT: &str = include_str!("../../tests/fixtures/run_output.json");
    const JOB_GET: &str = include_str!("../../tests/fixtures/job_get.json");
    const CLUSTERS_LIST: &str = include_str!("../../tests/fixtures/clusters_list.json");
    const WAREHOUSES_LIST: &str = include_str!("../../tests/fixtures/warehouses_list.json");

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
    fn parses_run_get_fixture_with_tasks() {
        let run: Run = serde_json::from_str(RUN_GET).unwrap();
        assert_eq!(run.id, 50_851_892_761_073);
        assert_eq!(run.tasks.len(), 2);
        assert_eq!(run.tasks[1].task_key, "endring_sluttdato_fakta");
        assert_eq!(run.tasks[1].run_id, 11_831_981_220_627);
        assert_eq!(run.tasks[1].state.result_state, Some(ResultState::Success));
        assert!(run.page_url.starts_with("https://adb-1"));
        let listed: Run = serde_json::from_str(r#"{"run_id":1}"#).unwrap();
        assert!(listed.tasks.is_empty(), "list responses carry no tasks");
    }

    #[test]
    fn parses_job_get_fixture_with_tasks_and_deployment() {
        let job: Job = serde_json::from_str(JOB_GET).unwrap();
        let settings = &job.settings;
        assert_eq!(settings.edit_mode.as_deref(), Some("UI_LOCKED"));
        assert_eq!(settings.deployment.as_ref().unwrap().kind, "BUNDLE");
        assert_eq!(
            settings.schedule.as_ref().unwrap().quartz_cron_expression,
            "0 0 2 * * ?"
        );
        assert_eq!(settings.tasks.len(), 3);
        assert_eq!(settings.tasks[0].kind(), "python src/v1compat.py");
        assert_eq!(settings.tasks[0].cluster(), "job cluster small");
        assert_eq!(
            settings.tasks[1].kind(),
            "notebook /Repos/someone/okonomi/notebooks/endring"
        );
        assert_eq!(
            settings.tasks[1].cluster(),
            "all-purpose 0901-071234-abcd1234"
        );
        assert_eq!(settings.tasks[2].kind(), "wheel okonomi:publish");
        assert_eq!(settings.tasks[2].cluster(), "serverless");
        assert_eq!(TaskSettings::default().kind(), "-");
    }

    #[test]
    fn parses_clusters_list_fixture() {
        let page: ClustersList = serde_json::from_str(CLUSTERS_LIST).unwrap();
        assert_eq!(page.clusters.len(), 3);
        let interactive = &page.clusters[0];
        assert_eq!(interactive.id, "0901-071234-abcd1234");
        assert_eq!(interactive.state, ClusterState::Terminated);
        assert_eq!(interactive.workers(), "1-4");
        assert_eq!(interactive.source, "UI");
        let job = &page.clusters[1];
        assert_eq!(job.state, ClusterState::Running);
        assert_eq!(job.workers(), "2");
        assert!(page.clusters[2].state.is_active(), "pending bills");
        assert!(!ClusterState::Terminated.is_active());
    }

    #[test]
    fn warehouses_become_compute_rows() {
        let page: WarehousesList = serde_json::from_str(WAREHOUSES_LIST).unwrap();
        assert_eq!(page.warehouses.len(), 2);
        let stopped: Cluster = page.warehouses[0].clone().into();
        assert_eq!(stopped.kind, ComputeKind::Warehouse);
        assert_eq!(stopped.state, ClusterState::Terminated);
        assert_eq!(stopped.source, "SQL warehouse, serverless");
        assert_eq!(stopped.node_type_id, "2X-Small");
        assert_eq!(stopped.state_message, "auto-stop after 10 min");
        assert_eq!(stopped.state_label(), "STOPPED");
        let running: Cluster = page.warehouses[1].clone().into();
        assert_eq!(running.state, ClusterState::Running);
        assert_eq!(running.state_label(), "RUNNING");
        assert_eq!(running.workers(), "1");
        let listed: Cluster = serde_json::from_str(r#"{"cluster_id":"x"}"#).unwrap();
        assert_eq!(
            listed.kind,
            ComputeKind::Cluster,
            "clusters default the kind"
        );
    }

    #[test]
    fn parses_run_output_fixture() {
        let output: RunOutput = serde_json::from_str(RUN_OUTPUT).unwrap();
        assert!(output.error.unwrap().starts_with("AnalysisException"));
        assert!(output.error_trace.unwrap().contains("endring.py"));
        let quiet: RunOutput = serde_json::from_str(r#"{"metadata":{}}"#).unwrap();
        assert_eq!(quiet, RunOutput::default());
    }

    #[test]
    fn failure_is_any_result_but_success() {
        let state = |result| RunState {
            life_cycle_state: LifeCycleState::Terminated,
            result_state: result,
            state_message: String::new(),
        };
        assert!(state(Some(ResultState::Failed)).is_failure());
        assert!(state(Some(ResultState::Canceled)).is_failure());
        assert!(!state(Some(ResultState::Success)).is_failure());
        assert!(!state(None).is_failure(), "still running is not a failure");
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
