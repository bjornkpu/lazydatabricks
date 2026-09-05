//! Application state and the one place it changes.

mod custom;
mod filter;
mod focus;
mod keys;
mod list;
mod menu;
mod message;

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Duration;

pub use custom::{Context, CustomCommand, Output as CommandOutput, expand};
pub use filter::{Filter, Me, Status};
pub use focus::{ComputePanel, Panel, ScreenMode, SideLayout, Tab};
use jiff::tz::TimeZone;
pub use keys::{Action, Keymap};
pub use list::{Move, Selectable};
pub use menu::{FilterChoice, InputMode, MenuItem, parse_params};
pub use message::{ApiCall, Command, Key, Message};

use crate::api::models::{
    Cluster, ClusterState, ComputeKind, Job, LifeCycleState, Pipeline, PipelineState,
    PipelineUpdate, ResultState, Run, RunOutput, UpdateState,
};
use crate::config::{Loaded, Sort, Theme};
use crate::error::AppError;

/// Spinner frames, one per `Tick` while loading.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// Ticks the cursor must rest on a job before its runs are fetched. Holding `j` fires one
/// request, not one per row.
const RUNS_DEBOUNCE_TICKS: u8 = 3;
/// API log entries kept; older ones fall off.
const API_LOG_CAPACITY: usize = 200;
/// Refetch period for the runs table while a run on it is still going: 5 s at `TICK`.
const ACTIVE_POLL_TICKS: u64 = 50;
/// Heartbeat period. The input thread sends `Message::Tick` this often; ages and TTLs count
/// ticks, so tests drive time by sending ticks.
pub const TICK: Duration = Duration::from_millis(100);
/// Shown when an action is chosen without the opt-in.
const READ_ONLY: &str =
    "Read-only: start with --allow-actions or set allow_actions = true in config";
/// Ticks per second at `TICK` = 100ms. Turns config seconds into tick counts.
const TICKS_PER_SECOND: u64 = 10;

/// The workspace at a glance, for the Status panel: counted over every job and compute row,
/// not the filtered view, so it says what is going on, not what is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Jobs whose newest run is still going.
    pub running: usize,
    /// Jobs whose newest run failed.
    pub failed: usize,
    /// Clusters and warehouses that are up or on their way.
    pub compute_up: usize,
}

/// A fetched value and the tick it arrived on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cached<T> {
    at: u64,
    value: T,
}

/// A remote value and where its fetch stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Load<T> {
    /// Nothing to fetch (no job selected).
    #[default]
    Idle,
    Loading,
    Loaded(T),
    Failed(AppError),
}

/// All application state. Rendering is a pure function of this.
#[derive(Debug)]
pub struct App {
    pub profile: String,
    pub host: String,
    /// Zone for rendering timestamps. An input, so tests can pin UTC.
    pub tz: TimeZone,
    /// Wall-clock time from the last `Clock` message. `None` until the first one arrives.
    pub now: Option<jiff::Timestamp>,
    /// Newest known run per job, from the workspace-wide recent runs and from per-job fetches.
    pub latest_runs: HashMap<i64, Run>,
    pub keys: Keymap,
    /// Where config came from, for the Profile tab.
    pub config_note: String,
    /// The effective configuration as TOML, defaults filled in, for the Config tab.
    pub config_text: String,
    /// Config override for the `dev` tag that marks a job as mine.
    dev_tag: Option<String>,
    /// Config: other names that are me.
    me_aliases: Vec<String>,
    /// Upper bound on jobs fetched across pages.
    pub max_jobs: usize,
    /// Jobs older than this many ticks are refetched in the background.
    jobs_ttl_ticks: u64,
    /// Cached runs older than this many ticks are refetched when shown.
    runs_ttl_ticks: u64,
    /// Ticks since launch; the app's clock.
    pub ticks: u64,
    /// Who the token belongs to, once the SCIM call has answered.
    pub me: Option<Me>,
    pub me_error: Option<AppError>,
    /// Every job fetched. `jobs` is the filtered view of this.
    pub all_jobs: Vec<Job>,
    /// Jobs whose full settings (`jobs/get`) have replaced the list shape. Cleared on refresh.
    pub detailed: HashSet<i64>,
    /// Job whose `jobs/get` is in flight.
    job_inflight: Option<i64>,
    /// Every pipeline fetched. `pipelines` is the filtered view of this.
    pub all_pipelines: Vec<Pipeline>,
    pub pipelines: Selectable<Pipeline>,
    /// Tick the pipelines fetch started on, while one is in flight.
    pipelines_inflight: Option<u64>,
    pipelines_fetched_at: Option<u64>,
    pub pipelines_error: Option<AppError>,
    /// `pipelines/get` per pipeline, pretty-printed, for the JSON tab.
    pub pipeline_specs: HashMap<String, Load<String>>,
    /// Config: fetch and show `[4] Compute` at all.
    pub compute_panel: ComputePanel,
    /// Config: whether the side panel in context is the tall one.
    pub side_layout: SideLayout,
    /// Every cluster fetched. `compute` is the filtered view of this.
    pub all_compute: Vec<Cluster>,
    pub compute: Selectable<Cluster>,
    compute_inflight: Option<u64>,
    compute_fetched_at: Option<u64>,
    pub compute_error: Option<AppError>,
    /// The visible jobs, with the cursor.
    pub jobs: Selectable<Job>,
    pub filter: Filter,
    /// Where keys go: normal bindings, the filter, the `x` menu or its confirmation.
    pub input: InputMode,
    /// One-line feedback shown in place of the hint bar until the next key.
    pub notice: Option<String>,
    /// Run-now and cancel are allowed. Off by default: reading is safe, triggering is not.
    pub allow_actions: bool,
    /// Shell lines from config, offered in the `x` menu and on their own keys.
    pub custom: Vec<CustomCommand>,
    /// Every profile in `~/.databrickscfg`, for the `p` menu.
    pub profiles: Vec<String>,
    /// `(from, to)` pairs applied to list names, in config order. See `display_name`.
    pub replacements: Vec<(String, String)>,
    pub theme: Theme,
    /// `strftime` pattern for absolute times, validated at config load.
    pub date_format: String,
    /// Order of both lists. Applied in `apply_filter`.
    pub sort: Sort,
    /// A jobs fetch is in flight. True from launch until the first `JobsLoaded` or `JobsFailed`,
    /// then again during refreshes; the old list stays on screen meanwhile.
    pub loading: bool,
    /// Tick the last jobs fetch finished on, success or not. A failure counts, so the next
    /// attempt waits a whole TTL instead of firing on the next tick.
    jobs_fetched_at: Option<u64>,
    /// Index into `SPINNER`.
    pub spinner: usize,
    pub error: Option<AppError>,
    pub focus: Panel,
    /// The side panel whose selection the main panel shows. Always a side panel.
    pub context: Panel,
    /// Index into `context.tabs()`.
    pub tab: usize,
    /// Lines scrolled off the top of a text view in the main panel. Unbounded here; the draw
    /// clamps it and reports the limit back as `Message::ScrollLimit`.
    pub main_scroll: usize,
    pub mode: ScreenMode,
    /// Runs of the selected job, with the main panel's cursor.
    pub runs: Load<Selectable<Run>>,
    /// The run opened with Enter from the runs table, if any. Esc backs out.
    pub viewing_run: Option<i64>,
    pub run_detail: Load<Run>,
    /// Error output of the viewed run's failed tasks, by task run id. Cleared with the run.
    pub run_outputs: HashMap<i64, Load<RunOutput>>,
    /// The job `runs` belongs to, or is being fetched for.
    pub runs_job: Option<i64>,
    /// Runs fetch waiting for the cursor to rest: (job, ticks left).
    pending_runs: Option<(i64, u8)>,
    /// Job whose runs fetch is in flight.
    // ponytail: one in-flight id, not a set; a second fetch just overwrites it.
    runs_inflight: Option<i64>,
    /// Runs already fetched, by job. Revisits within `RUNS_TTL_TICKS` cost no call.
    runs_cache: HashMap<i64, Cached<Vec<Run>>>,
    pub api_log: VecDeque<ApiCall>,
    pub show_api_log: bool,
}

impl App {
    /// A freshly launched app: `main` has already kicked off the jobs and `Me` fetches.
    #[must_use]
    pub fn new(
        profile: &str,
        host: &str,
        tz: TimeZone,
        loaded: &Loaded,
        profiles: &[String],
    ) -> Self {
        let config = &loaded.config;
        let config_note = if loaded.found {
            loaded.path.display().to_string()
        } else {
            format!("{} (not found, defaults)", loaded.path.display())
        };
        Self {
            profile: profile.to_owned(),
            host: host.to_owned(),
            tz,
            now: None,
            latest_runs: HashMap::new(),
            // Conflicts were rejected at config load; a stray one falls back to the defaults.
            keys: Keymap::with_overrides(&config.keys).unwrap_or_default(),
            config_note,
            config_text: crate::config::render(config),
            dev_tag: config.dev_tag.clone(),
            me_aliases: config.me_aliases.clone(),
            max_jobs: config.max_jobs,
            jobs_ttl_ticks: config.jobs_ttl_secs.saturating_mul(TICKS_PER_SECOND),
            runs_ttl_ticks: config.runs_ttl_secs.saturating_mul(TICKS_PER_SECOND),
            ticks: 0,
            me: None,
            me_error: None,
            all_jobs: Vec::new(),
            detailed: HashSet::new(),
            job_inflight: None,
            all_pipelines: Vec::new(),
            pipelines: Selectable::default(),
            pipelines_inflight: Some(0),
            pipelines_fetched_at: None,
            pipelines_error: None,
            pipeline_specs: HashMap::new(),
            all_compute: Vec::new(),
            compute: Selectable::default(),
            compute_panel: ComputePanel::from_config(config.compute),
            side_layout: SideLayout::from_config(config.expand_focused),
            // The launch fetch is in flight, unless config turned the panel off.
            compute_inflight: config.compute.then_some(0),
            compute_fetched_at: None,
            compute_error: None,
            jobs: Selectable::default(),
            filter: Filter {
                text: config.filter.clone().unwrap_or_default(),
                mine_only: config.mine_only,
                status: config.status,
            },
            input: InputMode::Normal,
            notice: None,
            allow_actions: config.allow_actions,
            custom: config.commands.clone(),
            profiles: profiles.to_vec(),
            replacements: config
                .name_replacements
                .iter()
                .map(|(from, to)| (from.clone(), to.clone()))
                .collect(),
            theme: config.theme,
            date_format: config.date_format.clone(),
            sort: config.sort,
            loading: true,
            jobs_fetched_at: None,
            spinner: 0,
            error: None,
            focus: Panel::Jobs,
            context: Panel::Jobs,
            tab: 0,
            main_scroll: 0,
            mode: ScreenMode::Normal,
            runs: Load::Idle,
            viewing_run: None,
            run_detail: Load::Idle,
            run_outputs: HashMap::new(),
            runs_job: None,
            pending_runs: None,
            runs_inflight: None,
            runs_cache: HashMap::new(),
            api_log: VecDeque::new(),
            show_api_log: true,
        }
    }

    /// Folds one message into state and returns the side effects `main` should run. No IO
    /// happens here; quitting is `Command::Quit` so the terminal restore in `main` gets to run.
    pub fn update(&mut self, message: Message) -> Vec<Command> {
        let mut commands = Vec::new();
        match message {
            Message::Key(key) => self.on_key(key, &mut commands),
            Message::Tick => self.tick(&mut commands),
            Message::JobsLoaded(jobs) => self.on_jobs_loaded(jobs),
            Message::JobLoaded(job) => self.on_job_loaded(&job),
            Message::JobFailed { job_id, error } => self.on_job_failed(job_id, &error),
            Message::JobsFailed(error) => {
                self.loading = false;
                self.jobs_fetched_at = Some(self.ticks);
                // The border only has room for the kind; the full text shows once, here.
                if !self.all_jobs.is_empty() {
                    self.notice = Some(error.to_string());
                }
                self.error = Some(error);
            }
            Message::PipelinesLoaded(pipelines) => {
                self.on_pipelines_loaded(pipelines, &mut commands);
            }
            Message::ComputeLoaded(compute) => self.on_compute_loaded(compute),
            Message::ComputeFailed(error) => self.on_compute_failed(error),
            Message::ClusterStarted { cluster_id } => {
                self.on_cluster_action(&cluster_id, ClusterState::Pending, &mut commands);
            }
            Message::ClusterTerminated { cluster_id } => {
                self.on_cluster_action(&cluster_id, ClusterState::Terminating, &mut commands);
            }
            Message::PipelinesFailed(error) => self.on_pipelines_failed(error),
            Message::PipelineLoaded { pipeline_id, spec } => {
                self.on_pipeline_spec(pipeline_id, Ok(spec));
            }
            Message::PipelineFailed { pipeline_id, error } => {
                self.on_pipeline_spec(pipeline_id, Err(error));
            }
            Message::UpdateStarted {
                pipeline_id,
                update_id,
            } => self.on_update_started(&pipeline_id, &update_id, &mut commands),
            Message::PipelineStopped { pipeline_id } => {
                self.notice = Some("Stop requested".to_owned());
                self.patch_pipeline(&pipeline_id, |pipeline| {
                    pipeline.state = PipelineState::Stopping;
                });
                self.refresh_pipelines(&mut commands);
            }
            Message::Clock(now) => self.now = Some(now),
            Message::RecentRunsLoaded(runs) => self.on_recent_runs_loaded(runs, &mut commands),
            Message::RunsLoaded { job_id, runs } => {
                self.on_runs_loaded(job_id, runs, &mut commands);
            }
            Message::RunsFailed { job_id, error } => self.on_runs_failed(job_id, error),
            Message::RunDetailLoaded(run) => self.on_run_detail_loaded(run, &mut commands),
            Message::ScrollLimit(limit) => self.main_scroll = self.main_scroll.min(limit),
            Message::ShellFinished { name, output } => self.on_shell_finished(name, output),
            Message::ShellExited { name, detail } => {
                self.notice = Some(format!("{name}: {detail}"));
            }
            Message::RunOutputLoaded { run_id, output } => {
                self.set_run_output(run_id, Load::Loaded(output));
            }
            Message::RunOutputFailed { run_id, error } => {
                self.set_run_output(run_id, Load::Failed(error));
            }
            Message::RunDetailFailed { run_id, error } => self.on_run_detail_failed(run_id, error),
            Message::ApiCalled(call) => {
                if self.api_log.len() >= API_LOG_CAPACITY {
                    self.api_log.pop_front();
                }
                self.api_log.push_back(call);
            }
            Message::MeLoaded(email) => {
                let mut me = Me::from_email(&email);
                if let Some(tag) = &self.dev_tag {
                    tag.clone_into(&mut me.tag);
                }
                me.aliases.clone_from(&self.me_aliases);
                self.me = Some(me);
                self.me_error = None;
                if self.filter.mine_only {
                    self.apply_filter();
                }
            }
            Message::MeFailed(error) => self.me_error = Some(error),
            Message::RunStarted { job_id, run_id } => {
                self.on_run_started(job_id, run_id, &mut commands);
            }
            Message::RunCancelled { job_id, run_id } => {
                self.on_run_cancelled(job_id, run_id, &mut commands);
            }
            Message::SchedulePaused { job_id, paused } => self.on_schedule_paused(job_id, paused),
            Message::RunRepaired { job_id, run_id } => {
                self.on_run_repaired(job_id, run_id, &mut commands);
            }
            Message::RecentRunsFailed(error) | Message::ActionFailed(error) => {
                self.notice = Some(error.to_string());
            }
        }
        commands
    }

    /// Routes a key by input mode. Any key dismisses the last notice; the handler may set a new one.
    fn on_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        // Any key dismisses the last notice; the handler may set a new one.
        self.notice = None;
        match self.input {
            InputMode::Normal => self.key(key, commands),
            InputMode::Filter => self.filter_key(key, commands),
            InputMode::Menu { .. } => self.menu_key(key, commands),
            InputMode::Output { .. } => self.output_key(key, commands),
            InputMode::Confirm(_) => self.confirm_key(key, commands),
            InputMode::Params { .. } => self.params_key(key, commands),
            InputMode::Prompt { .. } => self.prompt_key(key, commands),
            InputMode::ConfirmActions => {
                self.input = InputMode::Normal;
                if key == Key::Char('y') {
                    self.allow_actions = true;
                    self.notice = Some("Actions enabled for this session".to_owned());
                }
            }
            InputMode::Help { scroll } => match key {
                Key::Esc | Key::Char('?' | 'q') => self.input = InputMode::Normal,
                Key::Char('j') | Key::Down => {
                    self.input = InputMode::Help {
                        scroll: scroll.saturating_add(1),
                    };
                }
                Key::Char('k') | Key::Up => {
                    self.input = InputMode::Help {
                        scroll: scroll.saturating_sub(1),
                    };
                }
                _ => {}
            },
        }
    }

    /// One heartbeat: the clock, the spinner, the debounced runs fetch and TTL-driven refreshes.
    fn tick(&mut self, commands: &mut Vec<Command>) {
        self.ticks = self.ticks.saturating_add(1);
        if self.loading || self.pipelines_loading() || self.compute_loading() || self.runs_busy() {
            self.spinner = self
                .spinner
                .wrapping_add(1)
                .checked_rem(SPINNER.len())
                .unwrap_or(0);
        }
        if let Some((job_id, ticks)) = self.pending_runs {
            let left = ticks.saturating_sub(1);
            if left == 0 {
                self.pending_runs = None;
                if self.runs_inflight != Some(job_id) {
                    self.runs_inflight = Some(job_id);
                    commands.push(Command::FetchRuns { job_id });
                }
            } else {
                self.pending_runs = Some((job_id, left));
            }
        }
        // The Detail and JSON tabs want the full job once the cursor has rested (no runs fetch
        // pending).
        if self.context == Panel::Jobs
            && matches!(self.active_tab(), Some(Tab::Detail | Tab::Json))
            && self.pending_runs.is_none()
            && let Some(job_id) = self.jobs.selected().map(|job| job.id)
            && !self.detailed.contains(&job_id)
            && self.job_inflight.is_none()
        {
            self.job_inflight = Some(job_id);
            commands.push(Command::FetchJob { job_id });
        }
        // The Output tab wants every task's output, not only the failed ones Detail asks for.
        if self.active_tab() == Some(Tab::Output)
            && let Load::Loaded(run) = &self.run_detail
        {
            let missing: Vec<i64> = run
                .tasks
                .iter()
                .map(|task| task.run_id)
                .filter(|run_id| matches!(self.run_outputs.get(run_id), None | Some(Load::Idle)))
                .collect();
            for run_id in missing {
                self.run_outputs.insert(run_id, Load::Loading);
                commands.push(Command::FetchRunOutput { run_id });
            }
        }
        // A pipeline's JSON tab fetches its spec the first time it is opened.
        if self.context == Panel::Pipelines
            && self.active_tab() == Some(Tab::Json)
            && let Some(pipeline_id) = self
                .pipelines
                .selected()
                .map(|pipeline| pipeline.id.clone())
            && !self.pipeline_specs.contains_key(&pipeline_id)
        {
            self.pipeline_specs
                .insert(pipeline_id.clone(), Load::Loading);
            commands.push(Command::FetchPipeline { pipeline_id });
        }
        // Background refresh of what is on screen, once it is older than its TTL.
        if self
            .jobs_fetched_at
            .is_some_and(|at| self.age_ticks(at) >= self.jobs_ttl_ticks)
        {
            self.refresh_jobs(commands);
        }
        if self
            .pipelines_fetched_at
            .is_some_and(|at| self.age_ticks(at) >= self.jobs_ttl_ticks)
        {
            self.refresh_pipelines(commands);
        }
        if self
            .compute_fetched_at
            .is_some_and(|at| self.age_ticks(at) >= self.jobs_ttl_ticks)
        {
            self.refresh_compute(commands);
        }
        // Active runs poll fast so the table moves on its own; settled ones wait the TTL.
        let runs_ttl = if self.runs_active() {
            self.runs_ttl_ticks.min(ACTIVE_POLL_TICKS)
        } else {
            self.runs_ttl_ticks
        };
        if let Some(job_id) = self.runs_job
            && self.pending_runs.is_none()
            && self
                .runs_cache
                .get(&job_id)
                .is_some_and(|cached| self.age_ticks(cached.at) >= runs_ttl)
        {
            self.refresh_runs(commands);
        }
    }

    /// The list landed: list shapes again, so details are refetched when a Detail tab asks.
    fn on_jobs_loaded(&mut self, jobs: Vec<Job>) {
        self.all_jobs = jobs;
        self.detailed.clear();
        self.jobs_fetched_at = Some(self.ticks);
        self.loading = false;
        self.error = None;
        self.apply_filter();
    }

    fn on_job_failed(&mut self, job_id: i64, error: &AppError) {
        if self.job_inflight == Some(job_id) {
            self.job_inflight = None;
        }
        self.notice = Some(error.to_string());
    }

    /// Full settings arrived: they replace the list shape in both lists, so the Detail tab and
    /// the JSON output see tasks and schedule.
    fn on_job_loaded(&mut self, job: &Job) {
        if self.job_inflight == Some(job.id) {
            self.job_inflight = None;
        }
        self.detailed.insert(job.id);
        for slot in self
            .all_jobs
            .iter_mut()
            .chain(self.jobs.items_mut().iter_mut())
            .filter(|slot| slot.id == job.id)
        {
            slot.settings = job.settings.clone();
        }
    }

    /// The workspace sweep landed. Newest first, so the first run seen for a job is its latest;
    /// jobs the sweep no longer covers keep the run they had, which is still their newest.
    fn on_recent_runs_loaded(&mut self, runs: Vec<Run>, commands: &mut Vec<Command>) {
        let mut latest = HashMap::new();
        for run in runs {
            latest.entry(run.job_id).or_insert(run);
        }
        self.alert(&self.run_failures(&latest), commands);
        self.latest_runs.extend(latest);
        // Activity order depends on these.
        self.apply_filter();
    }

    /// Jobs whose newest run just turned into a failure, by name. A job seen for the first time
    /// is not a transition, so the first load after start stays quiet.
    fn run_failures(&self, latest: &HashMap<i64, Run>) -> Vec<String> {
        let mut names: Vec<String> = latest
            .iter()
            .filter(|(job_id, run)| {
                run.state.is_failure()
                    && self
                        .latest_runs
                        .get(job_id)
                        .is_some_and(|old| old.id != run.id || !old.state.is_failure())
            })
            .filter_map(|(job_id, _)| {
                self.all_jobs
                    .iter()
                    .find(|job| job.id == *job_id)
                    .map(|job| job.settings.name.clone())
            })
            .collect();
        names.sort();
        names
    }

    /// Bell and a notice for every name in `failed`. Nothing when the list is empty.
    fn alert(&mut self, failed: &[String], commands: &mut Vec<Command>) {
        if failed.is_empty() {
            return;
        }
        self.notice = Some(format!("✗ {} failed", failed.join(", ")));
        commands.push(Command::Bell);
    }

    /// A job's runs arrived: cache them, show them if that job is selected, note its newest run.
    fn on_runs_loaded(&mut self, job_id: i64, runs: Vec<Run>, commands: &mut Vec<Command>) {
        if self.runs_inflight == Some(job_id) {
            self.runs_inflight = None;
        }
        if let Some(latest) = runs.first() {
            let fresh = HashMap::from([(job_id, latest.clone())]);
            self.alert(&self.run_failures(&fresh), commands);
            self.latest_runs.insert(job_id, latest.clone());
        }
        if self.runs_job == Some(job_id) {
            // Shown now and cached for the next visit: two owners, hence the clone.
            self.show_runs(Load::Loaded(Selectable::new(runs.clone())));
        }
        self.runs_cache.insert(
            job_id,
            Cached {
                at: self.ticks,
                value: runs,
            },
        );
        // Activity order may have changed for this job.
        self.apply_filter();
    }

    /// A runs fetch failed. Cached runs stay on screen and count as fresh again, so the retry
    /// waits a TTL; the error is only the whole view when there was nothing to show.
    fn on_runs_failed(&mut self, job_id: i64, error: AppError) {
        if self.runs_inflight == Some(job_id) {
            self.runs_inflight = None;
        }
        if let Some(cached) = self.runs_cache.get_mut(&job_id) {
            cached.at = self.ticks;
            if self.runs_job == Some(job_id) {
                self.notice = Some(error.to_string());
            }
        } else if self.runs_job == Some(job_id) {
            self.show_runs(Load::Failed(error));
        }
    }

    /// Only a slot opened by the viewed run takes the reply; late ones are dropped.
    fn set_run_output(&mut self, run_id: i64, output: Load<RunOutput>) {
        if let Some(slot) = self.run_outputs.get_mut(&run_id) {
            *slot = output;
        }
    }

    /// The viewed run arrived: show it and fetch the output of every failed task not seen yet.
    fn on_run_detail_loaded(&mut self, run: Run, commands: &mut Vec<Command>) {
        if self.viewing_run != Some(run.id) {
            return;
        }
        for task in run.tasks.iter().filter(|task| task.state.is_failure()) {
            let slot = self.run_outputs.entry(task.run_id).or_default();
            if matches!(slot, Load::Idle | Load::Failed(_)) {
                *slot = Load::Loading;
                commands.push(Command::FetchRunOutput {
                    run_id: task.run_id,
                });
            }
        }
        self.run_detail = Load::Loaded(run);
    }

    /// Only the run still being viewed gets its failure shown; a stale reply is dropped.
    fn on_run_detail_failed(&mut self, run_id: i64, error: AppError) {
        if self.viewing_run == Some(run_id) {
            self.run_detail = Load::Failed(error);
        }
    }

    /// `run-now` accepted. Optimistic: show the new run at once, then reconcile with a refetch.
    fn on_run_started(&mut self, job_id: i64, run_id: i64, commands: &mut Vec<Command>) {
        self.notice = Some(format!("Started run {run_id}"));
        if self.runs_job == Some(job_id) {
            if let Load::Loaded(runs) = &mut self.runs {
                runs.push_front(Run::placeholder(job_id, run_id));
            }
            self.refresh_runs(commands);
        }
    }

    /// An update was accepted. Optimistic: the pipeline shows it queued until the refetch.
    fn on_update_started(
        &mut self,
        pipeline_id: &str,
        update_id: &str,
        commands: &mut Vec<Command>,
    ) {
        let short: String = update_id.chars().take(8).collect();
        self.notice = Some(format!("Started update {short}"));
        let now = self.now;
        self.patch_pipeline(pipeline_id, |pipeline| {
            pipeline.state = PipelineState::Starting;
            pipeline.latest_updates.insert(
                0,
                PipelineUpdate {
                    id: update_id.to_owned(),
                    state: UpdateState::Queued,
                    creation_time: now,
                },
            );
        });
        self.refresh_pipelines(commands);
    }

    fn on_pipelines_loaded(&mut self, pipelines: Vec<Pipeline>, commands: &mut Vec<Command>) {
        self.alert(
            &pipeline_failures(&self.all_pipelines, &pipelines),
            commands,
        );
        self.all_pipelines = pipelines;
        self.pipelines_fetched_at = Some(self.ticks);
        self.pipelines_inflight = None;
        self.pipelines_error = None;
        self.apply_filter();
    }

    /// Same rule as jobs: the failure counts as a fetch, the old list stays, the kind shows in
    /// the border and the full text once in the hint bar.
    fn on_pipelines_failed(&mut self, error: AppError) {
        self.pipelines_inflight = None;
        self.pipelines_fetched_at = Some(self.ticks);
        if !self.all_pipelines.is_empty() {
            self.notice = Some(error.to_string());
        }
        self.pipelines_error = Some(error);
    }

    fn on_compute_loaded(&mut self, compute: Vec<Cluster>) {
        self.all_compute = compute;
        self.compute_fetched_at = Some(self.ticks);
        self.compute_inflight = None;
        self.compute_error = None;
        self.apply_filter();
    }

    fn on_compute_failed(&mut self, error: AppError) {
        self.compute_inflight = None;
        self.compute_fetched_at = Some(self.ticks);
        if !self.all_compute.is_empty() {
            self.notice = Some(error.to_string());
        }
        self.compute_error = Some(error);
    }

    /// A cluster action was accepted: show the transitional state until the refetch.
    fn on_cluster_action(
        &mut self,
        cluster_id: &str,
        state: ClusterState,
        commands: &mut Vec<Command>,
    ) {
        self.notice = Some(format!("{} requested", state.as_str().to_lowercase()));
        self.all_compute
            .iter_mut()
            .chain(self.compute.items_mut().iter_mut())
            .filter(|cluster| cluster.id == cluster_id)
            .for_each(|cluster| cluster.state = state);
        self.refresh_compute(commands);
    }

    /// One compute fetch at a time, same TTL as jobs.
    fn refresh_compute(&mut self, commands: &mut Vec<Command>) {
        if self.compute_panel.is_enabled() && self.compute_inflight.is_none() {
            self.compute_inflight = Some(self.ticks);
            commands.push(Command::FetchCompute { max: self.max_jobs });
        }
    }

    /// A compute fetch is in flight.
    #[must_use]
    pub const fn compute_loading(&self) -> bool {
        self.compute_inflight.is_some()
    }

    /// Applies `change` to a pipeline in both the full and the filtered list.
    fn patch_pipeline(&mut self, pipeline_id: &str, mut change: impl FnMut(&mut Pipeline)) {
        self.all_pipelines
            .iter_mut()
            .chain(self.pipelines.items_mut().iter_mut())
            .filter(|pipeline| pipeline.id == pipeline_id)
            .for_each(&mut change);
    }

    /// Cancel accepted. The run shows as terminating until the refetch says otherwise.
    fn on_run_cancelled(&mut self, job_id: i64, run_id: i64, commands: &mut Vec<Command>) {
        self.notice = Some(format!("Cancel requested for run {run_id}"));
        self.patch_run(job_id, run_id, |run| {
            run.state.life_cycle_state = LifeCycleState::Terminating;
        });
        if self.runs_job == Some(job_id) {
            self.refresh_runs(commands);
        }
    }

    /// The pause status changed. Patched into the job where its schedule is known; the list
    /// response carries no schedule, so an unopened job just gets the notice.
    fn on_schedule_paused(&mut self, job_id: i64, paused: bool) {
        let mut name = None;
        for job in self
            .all_jobs
            .iter_mut()
            .chain(self.jobs.items_mut().iter_mut())
            .filter(|job| job.id == job_id)
        {
            name = Some(job.settings.name.clone());
            if let Some(schedule) = &mut job.settings.schedule {
                schedule.pause_status = Some(if paused { "PAUSED" } else { "UNPAUSED" }.to_owned());
            }
        }
        let what = if paused { "paused" } else { "resumed" };
        self.notice = Some(name.map_or_else(
            || format!("Schedule {what}"),
            |name| format!("Schedule {what}: {name}"),
        ));
    }

    /// Repair accepted. Databricks reuses the run id; until the refetch, the run reads as pending.
    fn on_run_repaired(&mut self, job_id: i64, run_id: i64, commands: &mut Vec<Command>) {
        self.notice = Some(format!("Repair requested for run {run_id}"));
        self.patch_run(job_id, run_id, |run| {
            run.state.life_cycle_state = LifeCycleState::Pending;
            run.state.result_state = None;
        });
        if self.runs_job == Some(job_id) {
            self.refresh_runs(commands);
        }
    }

    /// Applies `change` to a run in the table, when that job's runs are the ones shown.
    fn patch_run(&mut self, job_id: i64, run_id: i64, change: impl FnOnce(&mut Run)) {
        if self.runs_job == Some(job_id)
            && let Load::Loaded(runs) = &mut self.runs
            && let Some(run) = runs.items_mut().iter_mut().find(|run| run.id == run_id)
        {
            change(run);
        }
    }

    /// The tab the main panel is showing, if the context panel has any.
    #[must_use]
    pub fn active_tab(&self) -> Option<Tab> {
        self.context.tabs().get(self.tab).copied()
    }

    #[must_use]
    pub fn spinner_glyph(&self) -> char {
        SPINNER.get(self.spinner).copied().unwrap_or(' ')
    }

    /// A run on screen is still going, in the table or opened in detail.
    fn runs_active(&self) -> bool {
        let table = match &self.runs {
            Load::Loaded(runs) => runs
                .items()
                .iter()
                .any(|run| run.state.life_cycle_state.is_active()),
            Load::Idle | Load::Loading | Load::Failed(_) => false,
        };
        let detail = match &self.run_detail {
            Load::Loaded(run) => run.state.life_cycle_state.is_active(),
            Load::Idle | Load::Loading | Load::Failed(_) => false,
        };
        table || detail
    }

    /// A runs fetch is scheduled or in flight for the shown job.
    #[must_use]
    pub const fn runs_busy(&self) -> bool {
        self.pending_runs.is_some() || self.runs_inflight.is_some()
    }

    /// How old the jobs list on screen is.
    #[must_use]
    pub fn jobs_age(&self) -> Option<Duration> {
        self.jobs_fetched_at
            .map(|at| TICK.saturating_mul(u32::try_from(self.age_ticks(at)).unwrap_or(u32::MAX)))
    }

    const fn age_ticks(&self, at: u64) -> u64 {
        self.ticks.saturating_sub(at)
    }

    /// One jobs fetch at a time; the list on screen stays until the new one lands.
    fn refresh_jobs(&mut self, commands: &mut Vec<Command>) {
        if !self.loading {
            self.loading = true;
            commands.push(Command::FetchJobs { max: self.max_jobs });
            commands.push(Command::FetchRecentRuns { max: self.max_jobs });
        }
    }

    /// One pipelines fetch at a time, same TTL as jobs.
    fn refresh_pipelines(&mut self, commands: &mut Vec<Command>) {
        if self.pipelines_inflight.is_none() {
            self.pipelines_inflight = Some(self.ticks);
            commands.push(Command::FetchPipelines { max: self.max_jobs });
        }
    }

    /// A pipelines fetch is in flight.
    #[must_use]
    pub const fn pipelines_loading(&self) -> bool {
        self.pipelines_inflight.is_some()
    }

    /// Refetches the shown job's runs now, skipping the debounce and the cache. The run being
    /// viewed, if any, is refetched too.
    fn refresh_runs(&mut self, commands: &mut Vec<Command>) {
        if let Some(job_id) = self.runs_job
            && self.runs_inflight.is_none()
        {
            self.pending_runs = None;
            self.runs_inflight = Some(job_id);
            commands.push(Command::FetchRuns { job_id });
        }
        if let Some(run_id) = self.viewing_run {
            self.run_detail = Load::Loading;
            commands.push(Command::FetchRunDetail { run_id });
        }
    }

    /// Replaces the runs on screen. A cursor already on the table keeps its row.
    fn show_runs(&mut self, runs: Load<Selectable<Run>>) {
        let keep = match &self.runs {
            Load::Loaded(current) => current.selected_index(),
            Load::Idle | Load::Loading | Load::Failed(_) => None,
        };
        self.runs = runs;
        if let (Load::Loaded(list), Some(index)) = (&mut self.runs, keep) {
            for _ in 0..index {
                list.apply(Move::Down);
            }
        }
    }

    /// The run under the cursor in the runs table, when the main panel shows it.
    fn selected_run(&self) -> Option<&Run> {
        if self.context != Panel::Jobs || self.active_tab() != Some(Tab::Runs) {
            return None;
        }
        match &self.runs {
            Load::Loaded(runs) => runs.selected(),
            Load::Idle | Load::Loading | Load::Failed(_) => None,
        }
    }

    fn leave_run_detail(&mut self) {
        self.viewing_run = None;
        self.main_scroll = 0;
        self.run_detail = Load::Idle;
        self.run_outputs.clear();
    }

    /// What the JSON tab shows for the selection: the job's settings once `jobs/get` has been
    /// seen, the pipeline's spec once fetched.
    /// The Output tab: every task of the viewed run with what `runs/get-output` returned for
    /// it. Plain strings, so the pager gets exactly what the panel shows; a `▸ ` prefix marks
    /// a task header.
    #[must_use]
    pub fn output_lines(&self) -> Vec<String> {
        let Some(run_id) = self.viewing_run else {
            return vec!["Open a run with Enter on the Runs tab to see its output.".to_owned()];
        };
        let run = match &self.run_detail {
            Load::Loaded(run) => run,
            Load::Failed(error) => return vec![error.to_string()],
            Load::Idle | Load::Loading => return vec![format!("Loading run {run_id}…")],
        };
        let mut lines = Vec::new();
        for task in &run.tasks {
            let result = task
                .state
                .result_state
                .map_or_else(|| task.state.life_cycle_state.as_str(), ResultState::as_str);
            lines.push(format!("▸ {}  {result}", task.task_key));
            match self.run_outputs.get(&task.run_id) {
                Some(Load::Loaded(output)) => lines.extend(output_body(output)),
                Some(Load::Failed(error)) => lines.push(error.to_string()),
                Some(Load::Loading | Load::Idle) | None => {
                    lines.push(format!("{} fetching output…", self.spinner_glyph()));
                }
            }
            lines.push(String::new());
        }
        lines.pop();
        lines
    }

    /// `pipelines/get` landed, or did not; either way the JSON tab has something to show.
    fn on_pipeline_spec(&mut self, pipeline_id: String, spec: Result<serde_json::Value, AppError>) {
        let load = match spec {
            Ok(spec) => Load::Loaded(pretty_json(&spec)),
            Err(error) => {
                self.notice = Some(error.to_string());
                Load::Failed(error)
            }
        };
        self.pipeline_specs.insert(pipeline_id, load);
    }

    #[must_use]
    pub fn json_view(&self) -> Load<String> {
        match self.context {
            Panel::Jobs => self.jobs.selected().map_or(Load::Idle, |job| {
                if self.detailed.contains(&job.id) {
                    Load::Loaded(pretty_json(&job.settings))
                } else {
                    Load::Loading
                }
            }),
            Panel::Pipelines => self.pipelines.selected().map_or(Load::Idle, |pipeline| {
                self.pipeline_specs
                    .get(&pipeline.id)
                    .cloned()
                    .unwrap_or(Load::Loading)
            }),
            Panel::Status | Panel::Compute | Panel::Main => Load::Idle,
        }
    }

    /// A name as the side lists show it: `name_replacements` applied in order. lazydocker's
    /// `replacements`; the full name stays what filters match and `y` copies.
    #[must_use]
    pub fn display_name(&self, name: &str) -> String {
        self.replacements
            .iter()
            .fold(name.to_owned(), |name, (from, to)| name.replace(from, to))
    }

    /// lazygit's status dashboard: what is running, what is red, what is billing.
    #[must_use]
    pub fn counts(&self) -> Counts {
        Counts {
            running: self
                .latest_runs
                .values()
                .filter(|run| run.state.life_cycle_state.is_active())
                .count(),
            failed: self
                .latest_runs
                .values()
                .filter(|run| run.state.is_failure())
                .count(),
            compute_up: self
                .all_compute
                .iter()
                .filter(|row| row.state.is_active())
                .count(),
        }
    }

    /// What the status panel says about the list: `mine only · /gold · 22 of 85`. Never a
    /// mystery why a list looks short.
    #[must_use]
    pub fn filter_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.filter.status != Status::All {
            parts.push(format!("{} only", self.filter.status.as_str()));
        }
        if self.filter.mine_only {
            parts.push("mine only".to_owned());
        }
        if !self.filter.text.is_empty() {
            parts.push(format!("/{}", self.filter.text));
        }
        // Counts follow the panel in context, so the status line explains the list you look at.
        let (visible, total, what) = match self.context {
            Panel::Pipelines => (
                self.pipelines.items().len(),
                self.all_pipelines.len(),
                "pipelines",
            ),
            Panel::Compute => (
                self.compute.items().len(),
                self.all_compute.len(),
                "compute",
            ),
            Panel::Status | Panel::Jobs | Panel::Main => {
                (self.jobs.items().len(), self.all_jobs.len(), "jobs")
            }
        };
        // The fetch stops at `max_jobs`; a full list means "at least this many", so say so.
        let plus = if total >= self.max_jobs { "+" } else { "" };
        parts.push(format!("{visible} of {total}{plus} {what}"));
        parts.join(" · ")
    }

    /// Keys outside filter editing: bound actions first, then the fixed digit keys.
    fn key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let Some(action) = self.keys.action(key) else {
            if let Key::Char(digit) = key
                && let Some(panel) = Panel::from_digit(digit)
            {
                self.set_focus(panel);
            } else {
                self.custom_key(key, commands);
            }
            return;
        };
        match action {
            Action::Quit => commands.push(Command::Quit),
            Action::ScreenMode => self.mode = self.mode.next(),
            Action::ToggleLog => self.show_api_log = !self.show_api_log,
            Action::Filter => {
                // The filter edits from whichever list is in context; jobs when neither is.
                let target = if self.context == Panel::Pipelines {
                    Panel::Pipelines
                } else {
                    Panel::Jobs
                };
                self.set_focus(target);
                self.input = InputMode::Filter;
            }
            Action::Menu => self.open_menu(),
            Action::Help => self.input = InputMode::Help { scroll: 0 },
            Action::ToggleActions => self.toggle_actions(),
            Action::SwitchProfile => self.open_profiles(),
            Action::Prompt => {
                self.input = InputMode::Prompt {
                    text: String::new(),
                }
            }
            Action::EditConfig => commands.push(Command::EditConfig),
            Action::FilterMenu => self.open_filter_menu(),
            Action::RangeSelect => self.toggle_range(),
            Action::Sort => {
                self.sort = self.sort.next();
                self.apply_filter();
                self.notice = Some(format!("Sorted by {}", self.sort.as_str()));
            }
            Action::Browse => {
                if let Some(url) = self.selected_url() {
                    self.notice = Some(format!("Opening {url}"));
                    commands.push(Command::OpenUrl(url));
                } else {
                    self.notice = Some("Nothing selected to open".to_owned());
                }
            }
            Action::Copy => self.open_copy_menu(),
            Action::CopyTable => {
                self.notice = Some("Copied the visible rows".to_owned());
                commands.push(Command::CopyVisible);
            }
            Action::MineOnly => {
                self.filter.mine_only = !self.filter.mine_only;
                self.apply_filter();
            }
            Action::StatusFilter => {
                self.filter.status = self.filter.status.next();
                self.apply_filter();
                self.notice = Some(match self.filter.status {
                    Status::All => "Showing all".to_owned(),
                    other => format!("Showing {} only", other.as_str()),
                });
            }
            Action::Refresh => self.refresh_focused(commands),
            Action::RefreshAll => self.refresh_all(commands),
            Action::NextPanel => self.set_focus(self.next_visible_side(self.focus)),
            Action::Open => self.open(commands),
            Action::Back => {
                if self.range_len().is_some() {
                    self.clear_range();
                } else if self.viewing_run.is_some() {
                    self.leave_run_detail();
                } else if self.focus == Panel::Main {
                    self.set_focus(self.context);
                }
            }
            Action::Down => self.move_cursor(Move::Down),
            Action::Up => self.move_cursor(Move::Up),
            Action::PageDown => self.move_cursor(Move::PageDown),
            Action::PageUp => self.move_cursor(Move::PageUp),
            Action::First => self.move_cursor(Move::First),
            Action::Last => self.move_cursor(Move::Last),
            Action::NextTab => self.next_tab(),
            Action::PrevTab => self.prev_tab(),
        }
    }

    /// `y`: what the selection can be copied as. URL first, so `y` Enter is still "copy the
    /// link"; then the id, the name and the JSON once fetched.
    fn open_copy_menu(&mut self) {
        let mut items = Vec::new();
        let mut copy = |label: &str, text: Option<String>| {
            if let Some(text) = text {
                items.push(MenuItem::CopyText {
                    label: label.to_owned(),
                    text,
                });
            }
        };
        copy("URL", self.selected_url());
        let viewed_run = self.viewing_run.or_else(|| {
            (self.focus == Panel::Main)
                .then(|| self.selected_run().map(|run| run.id))
                .flatten()
        });
        match self.context {
            Panel::Jobs => {
                copy("run ID", viewed_run.map(|id| id.to_string()));
                copy("job ID", self.jobs.selected().map(|job| job.id.to_string()));
                copy(
                    "name",
                    self.jobs.selected().map(|job| job.settings.name.clone()),
                );
            }
            Panel::Pipelines => {
                copy(
                    "pipeline ID",
                    self.pipelines
                        .selected()
                        .map(|pipeline| pipeline.id.clone()),
                );
                copy(
                    "name",
                    self.pipelines
                        .selected()
                        .map(|pipeline| pipeline.name.clone()),
                );
            }
            Panel::Compute => {
                copy("ID", self.compute.selected().map(|row| row.id.clone()));
                copy("name", self.compute.selected().map(|row| row.name.clone()));
            }
            Panel::Status | Panel::Main => {}
        }
        if let Load::Loaded(json) = self.json_view() {
            copy("JSON", Some(json));
        }
        // The API log exists to teach the API; the newest call as something you can run.
        if let Some(call) = self.api_log.back() {
            copy(
                "last request as databricks api",
                Some(format!(
                    "databricks api {} '{}' -p {}",
                    call.method.to_ascii_lowercase(),
                    call.path,
                    self.profile
                )),
            );
            let verb = if call.method == "GET" {
                String::new()
            } else {
                format!(" -X {}", call.method)
            };
            copy(
                "last request as curl",
                Some(format!(
                    "curl{verb} -H \"Authorization: Bearer $DATABRICKS_TOKEN\" '{}{}'",
                    self.host, call.path
                )),
            );
        }
        if items.is_empty() {
            self.notice = Some("Nothing selected to copy".to_owned());
            return;
        }
        self.input = InputMode::Menu { items, selected: 0 };
    }

    /// Enter: into `[0]`; on the runs table, into the run; on a text tab, into the pager.
    fn open(&mut self, commands: &mut Vec<Command>) {
        if self.focus == Panel::Main
            && matches!(
                self.active_tab(),
                Some(Tab::Json | Tab::Output | Tab::Config)
            )
        {
            self.page(commands);
        } else if self.focus == Panel::Main
            && self.viewing_run.is_none()
            && let Some(run_id) = self.selected_run().map(|run| run.id)
        {
            self.viewing_run = Some(run_id);
            self.run_detail = Load::Loading;
            self.main_scroll = 0;
            commands.push(Command::FetchRunDetail { run_id });
        } else {
            self.set_focus(Panel::Main);
        }
    }

    /// Enter on a text tab: the same text the tab shows, in `$PAGER`.
    fn page(&mut self, commands: &mut Vec<Command>) {
        let text = match self.active_tab() {
            Some(Tab::Json) => match self.json_view() {
                Load::Loaded(text) => Some(text),
                Load::Idle | Load::Loading | Load::Failed(_) => None,
            },
            Some(Tab::Output) if self.viewing_run.is_some() => Some(self.output_lines().join("\n")),
            Some(Tab::Config) => Some(self.config_text.clone()),
            _ => None,
        };
        match text {
            Some(text) => commands.push(Command::Page(text)),
            None => self.notice = Some("Nothing to page yet".to_owned()),
        }
    }

    /// `v` on a side list: anchor a range at the cursor, or end the one in progress.
    const fn toggle_range(&mut self) {
        match self.focus {
            Panel::Jobs => self.jobs.toggle_anchor(),
            Panel::Pipelines => self.pipelines.toggle_anchor(),
            Panel::Compute => self.compute.toggle_anchor(),
            Panel::Status | Panel::Main => {}
        }
    }

    const fn clear_range(&mut self) {
        self.jobs.clear_anchor();
        self.pipelines.clear_anchor();
        self.compute.clear_anchor();
    }

    /// How many rows the focused list's range covers, while one is in progress.
    #[must_use]
    pub fn range_len(&self) -> Option<usize> {
        let range = match self.focus {
            Panel::Jobs => self.jobs.range(),
            Panel::Pipelines => self.pipelines.range(),
            Panel::Compute => self.compute.range(),
            Panel::Status | Panel::Main => None,
        }?;
        Some(range.end().saturating_sub(*range.start()).saturating_add(1))
    }

    /// `F`: every filter as a menu, the cursor on the status in force. The same three settings
    /// `f`, `m` and `/` reach; this is the list of them.
    fn open_filter_menu(&mut self) {
        let mut items: Vec<MenuItem> = [Status::All, Status::Failed, Status::Active]
            .into_iter()
            .map(|status| MenuItem::Filter(FilterChoice::Status(status)))
            .collect();
        items.push(MenuItem::Filter(FilterChoice::MineOnly(
            !self.filter.mine_only,
        )));
        if !self.filter.text.is_empty() {
            items.push(MenuItem::Filter(FilterChoice::ClearText(
                self.filter.text.clone(),
            )));
        }
        let selected = items
            .iter()
            .position(|item| *item == MenuItem::Filter(FilterChoice::Status(self.filter.status)))
            .unwrap_or(0);
        self.input = InputMode::Menu { items, selected };
    }

    /// One `F` choice applied; the notice reads back the filter line the Status panel shows.
    fn apply_choice(&mut self, choice: &FilterChoice) {
        match choice {
            FilterChoice::Status(status) => self.filter.status = *status,
            FilterChoice::MineOnly(on) => self.filter.mine_only = *on,
            FilterChoice::ClearText(_) => self.filter.text.clear(),
        }
        self.apply_filter();
        self.notice = Some(self.filter_summary());
    }

    /// `p`: every profile in `~/.databrickscfg` as a menu, the current one under the cursor.
    fn open_profiles(&mut self) {
        if self.profiles.len() < 2 {
            self.notice = Some("Only one profile in ~/.databrickscfg".to_owned());
            return;
        }
        let selected = self
            .profiles
            .iter()
            .position(|name| *name == self.profile)
            .unwrap_or(0);
        let items = self
            .profiles
            .iter()
            .map(|name| MenuItem::SwitchProfile { name: name.clone() })
            .collect();
        self.input = InputMode::Menu { items, selected };
    }

    /// `A`: off at once, on only after a yes.
    fn toggle_actions(&mut self) {
        if self.allow_actions {
            self.allow_actions = false;
            self.notice = Some("Actions disabled".to_owned());
        } else {
            self.input = InputMode::ConfirmActions;
        }
    }

    /// `R`: every list, and the runs table.
    fn refresh_all(&mut self, commands: &mut Vec<Command>) {
        self.refresh_jobs(commands);
        self.refresh_pipelines(commands);
        self.refresh_compute(commands);
        self.refresh_runs(commands);
    }

    /// `r`: refetch what the focused panel shows.
    fn refresh_focused(&mut self, commands: &mut Vec<Command>) {
        match self.focus {
            Panel::Main => self.refresh_runs(commands),
            Panel::Jobs => self.refresh_jobs(commands),
            Panel::Pipelines => self.refresh_pipelines(commands),
            Panel::Compute => self.refresh_compute(commands),
            Panel::Status => {
                self.refresh_jobs(commands);
                self.refresh_pipelines(commands);
                self.refresh_compute(commands);
            }
        }
    }

    /// Keys while `/` filter editing is active. Letters go to the filter, not to bindings.
    fn filter_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        match key {
            Key::Ctrl('c') => commands.push(Command::Quit),
            Key::Enter => self.input = InputMode::Normal,
            Key::Esc => {
                self.input = InputMode::Normal;
                self.filter.text.clear();
                self.apply_filter();
            }
            Key::Backspace => {
                self.filter.text.pop();
                self.apply_filter();
            }
            Key::Char(c) => {
                self.filter.text.push(c);
                self.apply_filter();
            }
            Key::Down => self.move_cursor(Move::Down),
            Key::Up => self.move_cursor(Move::Up),
            Key::Tab | Key::Left | Key::Right | Key::Ctrl(_) => {}
        }
    }

    /// The workspace URL of the selected job or pipeline, by the panel in context.
    fn selected_url(&self) -> Option<String> {
        match self.context {
            Panel::Jobs => {
                let job = self.jobs.selected()?;
                let run_id = self.viewing_run.or_else(|| {
                    (self.focus == Panel::Main)
                        .then(|| self.selected_run().map(|run| run.id))
                        .flatten()
                });
                Some(run_id.map_or_else(
                    || format!("{}/jobs/{}", self.host, job.id),
                    |run_id| format!("{}/jobs/{}/runs/{run_id}", self.host, job.id),
                ))
            }
            Panel::Pipelines => {
                let pipeline = self.pipelines.selected()?;
                Some(format!("{}/pipelines/{}", self.host, pipeline.id))
            }
            Panel::Compute => {
                let row = self.compute.selected()?;
                Some(match row.kind {
                    ComputeKind::Cluster => format!("{}/compute/clusters/{}", self.host, row.id),
                    ComputeKind::Warehouse => format!("{}/sql/warehouses/{}", self.host, row.id),
                })
            }
            Panel::Status | Panel::Main => None,
        }
    }

    /// Opens the `x` menu over the selected item, or says why there is nothing to do.
    fn open_menu(&mut self) {
        let items = self.menu_items();
        if items.is_empty() {
            self.notice = Some("No actions for this selection".to_owned());
            return;
        }
        self.input = InputMode::Menu { items, selected: 0 };
    }

    /// The built-in actions for the selection, then the custom commands that apply to it. Over
    /// a `v` range of two or more rows, the bulk actions instead.
    fn menu_items(&self) -> Vec<MenuItem> {
        if let Some(bulk) = self.bulk_items() {
            return bulk;
        }
        let mut items = self.builtin_items();
        let vars = self.template_vars();
        items.extend(
            self.custom
                .iter()
                .filter(|custom| self.in_context(custom.context))
                .filter_map(|custom| Self::custom_item(custom, &vars).ok()),
        );
        items
    }

    /// A custom command on its own key. Silent when its context does not apply, like any
    /// unbound key; a placeholder with nothing to fill it says so.
    fn custom_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let Some(custom) = self
            .custom
            .iter()
            .find(|custom| custom.key == Some(key) && self.in_context(custom.context))
        else {
            return;
        };
        match Self::custom_item(custom, &self.template_vars()) {
            Ok(item) if matches!(item, MenuItem::Shell { confirm: true, .. }) => {
                self.input = InputMode::Confirm(item);
            }
            Ok(item) => self.fire(&item, commands),
            Err(error) => self.notice = Some(format!("{}: {error}", custom.name)),
        }
    }

    fn custom_item(
        custom: &CustomCommand,
        vars: &BTreeMap<&str, String>,
    ) -> Result<MenuItem, String> {
        Ok(MenuItem::Shell {
            name: custom.name.clone(),
            command: expand(&custom.command, vars)?,
            output: custom.output,
            confirm: custom.confirm,
        })
    }

    fn in_context(&self, context: Context) -> bool {
        match context {
            Context::Any => true,
            Context::Jobs => self.context == Panel::Jobs,
            Context::Runs => self.selected_run().is_some(),
            Context::Pipelines => self.context == Panel::Pipelines,
            Context::Compute => self.context == Panel::Compute,
        }
    }

    /// What `{{...}}` can name in a custom command: the workspace, then whatever is selected.
    fn template_vars(&self) -> BTreeMap<&'static str, String> {
        let mut vars = BTreeMap::from([
            ("host", self.host.clone()),
            ("profile", self.profile.clone()),
        ]);
        if let Some(url) = self.selected_url() {
            vars.insert("url", url);
        }
        match self.context {
            Panel::Jobs => {
                if let Some(job) = self.jobs.selected() {
                    vars.insert("job_id", job.id.to_string());
                    vars.insert("name", job.settings.name.clone());
                }
                if let Some(run) = self.selected_run() {
                    vars.insert("run_id", run.id.to_string());
                }
            }
            Panel::Pipelines => {
                if let Some(pipeline) = self.pipelines.selected() {
                    vars.insert("pipeline_id", pipeline.id.clone());
                    vars.insert("name", pipeline.name.clone());
                }
            }
            Panel::Compute => {
                if let Some(cluster) = self.compute.selected() {
                    vars.insert("cluster_id", cluster.id.clone());
                    vars.insert("name", cluster.name.clone());
                }
            }
            Panel::Status | Panel::Main => {}
        }
        vars
    }

    /// Sends an action and says so; the reply, or its failure, comes back as a message. A bulk
    /// action also ends the range, so a second Enter cannot repeat it by accident.
    fn fire(&mut self, item: &MenuItem, commands: &mut Vec<Command>) {
        self.notice = Some(format!("{}…", item.label()));
        commands.extend(item.commands());
        if matches!(item, MenuItem::Bulk { .. }) {
            self.clear_range();
        }
    }

    /// lazydocker's bulk commands: the same actions as the single-row menu, once per selected
    /// row, each named with its count. `None` unless a range of two or more is in progress.
    fn bulk_items(&self) -> Option<Vec<MenuItem>> {
        if self.range_len()? < 2 {
            return None;
        }
        Some(match self.focus {
            Panel::Jobs => self.bulk_jobs(),
            Panel::Pipelines => self.bulk_pipelines(),
            Panel::Compute => self.bulk_compute(),
            Panel::Status | Panel::Main => Vec::new(),
        })
    }

    fn bulk_jobs(&self) -> Vec<MenuItem> {
        let mut items = Vec::new();
        let jobs = self.jobs.selected_items();
        let n = plural(jobs.len(), "job");
        items.push(bulk(
            format!("Run now: {n}"),
            format!("Start a run of {n} now?"),
            jobs.iter()
                .map(|job| Command::RunNow {
                    job_id: job.id,
                    params: BTreeMap::new(),
                })
                .collect(),
        ));
        let cancels: Vec<Command> = jobs
            .iter()
            .filter_map(|job| self.latest_runs.get(&job.id))
            .filter(|run| run.state.life_cycle_state.is_active())
            .map(|run| Command::CancelRun {
                job_id: run.job_id,
                run_id: run.id,
            })
            .collect();
        if !cancels.is_empty() {
            let m = plural(cancels.len(), "active run");
            items.push(bulk(format!("Cancel {m}"), format!("Cancel {m}?"), cancels));
        }
        for (paused, verb) in [(true, "Pause"), (false, "Resume")] {
            items.push(bulk(
                format!("{verb} schedule: {n}"),
                format!("{verb} the schedule of {n}?"),
                jobs.iter()
                    .map(|job| Command::SetSchedulePaused {
                        job_id: job.id,
                        paused,
                    })
                    .collect(),
            ));
        }
        items
    }

    fn bulk_pipelines(&self) -> Vec<MenuItem> {
        let mut items = Vec::new();
        let pipelines = self.pipelines.selected_items();
        let n = plural(pipelines.len(), "pipeline");
        items.push(bulk(
            format!("Start update: {n}"),
            format!("Start an update of {n} now?"),
            pipelines
                .iter()
                .map(|pipeline| Command::StartUpdate {
                    pipeline_id: pipeline.id.clone(),
                })
                .collect(),
        ));
        let stops: Vec<Command> = pipelines
            .iter()
            .filter(|pipeline| pipeline.state.is_active())
            .map(|pipeline| Command::StopPipeline {
                pipeline_id: pipeline.id.clone(),
            })
            .collect();
        if !stops.is_empty() {
            let m = plural(stops.len(), "running pipeline");
            items.push(bulk(format!("Stop {m}"), format!("Stop {m}?"), stops));
        }
        items
    }

    fn bulk_compute(&self) -> Vec<MenuItem> {
        let mut items = Vec::new();
        let rows = self.compute.selected_items();
        let starts: Vec<Command> = rows
            .iter()
            .filter(|row| !row.state.is_active())
            .map(|row| match row.kind {
                ComputeKind::Cluster => Command::StartCluster {
                    cluster_id: row.id.clone(),
                },
                ComputeKind::Warehouse => Command::StartWarehouse {
                    warehouse_id: row.id.clone(),
                },
            })
            .collect();
        if !starts.is_empty() {
            let m = plural(starts.len(), "stopped compute");
            items.push(bulk(format!("Start {m}"), format!("Start {m}?"), starts));
        }
        let stops: Vec<Command> = rows
            .iter()
            .filter(|row| row.state.is_active())
            .map(|row| match row.kind {
                ComputeKind::Cluster => Command::TerminateCluster {
                    cluster_id: row.id.clone(),
                },
                ComputeKind::Warehouse => Command::StopWarehouse {
                    warehouse_id: row.id.clone(),
                },
            })
            .collect();
        if !stops.is_empty() {
            let m = plural(stops.len(), "running compute");
            items.push(bulk(format!("Stop {m}"), format!("Stop {m}?"), stops));
        }
        items
    }

    /// A popup command's output goes into the overlay; nothing, or a failure, into the notice.
    fn on_shell_finished(&mut self, name: String, output: Result<String, AppError>) {
        match output {
            Ok(text) if text.trim().is_empty() => {
                self.notice = Some(format!("{name}: no output"));
            }
            Ok(text) => {
                self.input = InputMode::Output {
                    title: name,
                    lines: text.lines().map(str::to_owned).collect(),
                    scroll: 0,
                };
            }
            Err(error) => self.notice = Some(format!("{name}: {error}")),
        }
    }

    /// Actions valid for what is selected: run the job, cancel any of its active runs.
    fn builtin_items(&self) -> Vec<MenuItem> {
        if self.context == Panel::Pipelines {
            return self.pipeline_menu_items();
        }
        if self.context == Panel::Compute {
            return self.cluster_menu_items();
        }
        if self.context != Panel::Jobs {
            return Vec::new();
        }
        let Some(job) = self.jobs.selected() else {
            return Vec::new();
        };
        let mut items = vec![
            MenuItem::RunNow {
                job_id: job.id,
                name: job.settings.name.clone(),
            },
            MenuItem::RunWith {
                job_id: job.id,
                name: job.settings.name.clone(),
            },
        ];
        let cancel = |run: &Run| MenuItem::CancelRun {
            job_id: job.id,
            run_id: run.id,
        };
        let repair = |run: &Run| MenuItem::RepairRun {
            job_id: job.id,
            run_id: run.id,
        };
        if self.focus == Panel::Main {
            // The table has a cursor: offer repair or cancel for that row only.
            if let Some(run) = self.selected_run() {
                if run.state.is_failure() {
                    items.push(repair(run));
                } else if run.state.life_cycle_state.is_active() {
                    items.push(cancel(run));
                }
            }
        } else if let Load::Loaded(runs) = &self.runs {
            // From the side panel: repair the newest run if it failed, cancel every active one.
            if let Some(newest) = runs.items().first()
                && newest.state.is_failure()
            {
                items.push(repair(newest));
            }
            items.extend(
                runs.items()
                    .iter()
                    .filter(|run| run.state.life_cycle_state.is_active())
                    .map(cancel),
            );
        }
        // The list response carries no schedule: until `jobs/get` has been seen, offer both
        // and let Databricks say if there is nothing to pause.
        let pause = MenuItem::PauseSchedule {
            job_id: job.id,
            name: job.settings.name.clone(),
        };
        let resume = MenuItem::ResumeSchedule {
            job_id: job.id,
            name: job.settings.name.clone(),
        };
        match &job.settings.schedule {
            Some(schedule) if schedule.is_paused() => items.push(resume),
            Some(_) => items.push(pause),
            None if self.detailed.contains(&job.id) => {}
            None => items.extend([pause, resume]),
        }
        items
    }

    /// Start a cluster that is down; terminate one that is up.
    fn cluster_menu_items(&self) -> Vec<MenuItem> {
        let Some(cluster) = self.compute.selected() else {
            return Vec::new();
        };
        let (id, name) = (cluster.id.clone(), cluster.name.clone());
        let item = match (cluster.kind, cluster.state.is_active()) {
            (ComputeKind::Cluster, true) => MenuItem::TerminateCluster {
                cluster_id: id,
                name,
            },
            (ComputeKind::Cluster, false) => MenuItem::StartCluster {
                cluster_id: id,
                name,
            },
            (ComputeKind::Warehouse, true) => MenuItem::StopWarehouse {
                warehouse_id: id,
                name,
            },
            (ComputeKind::Warehouse, false) => MenuItem::StartWarehouse {
                warehouse_id: id,
                name,
            },
        };
        vec![item]
    }

    /// Start an update; stop it too while one is in progress.
    fn pipeline_menu_items(&self) -> Vec<MenuItem> {
        let Some(pipeline) = self.pipelines.selected() else {
            return Vec::new();
        };
        let mut items = vec![MenuItem::StartUpdate {
            pipeline_id: pipeline.id.clone(),
            name: pipeline.name.clone(),
        }];
        if pipeline.state.is_active() {
            items.push(MenuItem::StopPipeline {
                pipeline_id: pipeline.id.clone(),
                name: pipeline.name.clone(),
            });
        }
        items
    }

    /// Keys while the menu is open: move, choose, or close. Custom commands skip the actions
    /// opt-in: the person who wrote them into config already opted in.
    fn menu_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let InputMode::Menu { items, selected } = std::mem::take(&mut self.input) else {
            return;
        };
        let last = items.len().saturating_sub(1);
        match key {
            Key::Esc | Key::Char('x') => {}
            Key::Enter => {
                let Some(item) = items.into_iter().nth(selected) else {
                    return;
                };
                match item {
                    MenuItem::Filter(choice) => self.apply_choice(&choice),
                    MenuItem::CopyText { label, text } => {
                        self.notice = Some(format!("Copied {label}"));
                        commands.push(Command::Copy(text));
                    }
                    MenuItem::Shell { confirm: false, .. } | MenuItem::SwitchProfile { .. } => {
                        self.fire(&item, commands);
                    }
                    MenuItem::Shell { .. } => self.input = InputMode::Confirm(item),
                    _ if !self.allow_actions => self.notice = Some(READ_ONLY.to_owned()),
                    MenuItem::RunWith { job_id, name } => {
                        self.input = InputMode::Params {
                            job_id,
                            name,
                            text: String::new(),
                        };
                    }
                    item => self.input = InputMode::Confirm(item),
                }
            }
            Key::Char('j') | Key::Down => {
                self.input = InputMode::Menu {
                    items,
                    selected: selected.saturating_add(1).min(last),
                };
            }
            Key::Char('k') | Key::Up => {
                self.input = InputMode::Menu {
                    items,
                    selected: selected.saturating_sub(1),
                };
            }
            _ => self.input = InputMode::Menu { items, selected },
        }
    }

    /// `y` sends the confirmed action; any other key backs out without a word.
    fn confirm_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let InputMode::Confirm(item) = std::mem::take(&mut self.input) else {
            return;
        };
        if key == Key::Char('y') {
            self.fire(&item, commands);
        }
    }

    /// Keys in the output overlay: scroll, copy, close.
    fn output_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let InputMode::Output {
            title,
            lines,
            scroll,
        } = std::mem::take(&mut self.input)
        else {
            return;
        };
        let last = lines.len().saturating_sub(1);
        let scroll = match key {
            Key::Esc | Key::Char('q') => return,
            Key::Char('j') | Key::Down => scroll.saturating_add(1).min(last),
            Key::Char('k') | Key::Up => scroll.saturating_sub(1),
            Key::Ctrl('d') => scroll.saturating_add(list::PAGE).min(last),
            Key::Ctrl('u') => scroll.saturating_sub(list::PAGE),
            Key::Char('g') => 0,
            Key::Char('G') => last,
            Key::Char('y') => {
                self.notice = Some("Copied the output".to_owned());
                commands.push(Command::Copy(lines.join("\n")));
                scroll
            }
            _ => scroll,
        };
        self.input = InputMode::Output {
            title,
            lines,
            scroll,
        };
    }

    /// Keys in the parameter prompt. `Enter` parses and sends; a bad pair keeps the prompt open
    /// with the complaint in the hint bar.
    fn params_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let InputMode::Params {
            job_id,
            name,
            mut text,
        } = std::mem::take(&mut self.input)
        else {
            return;
        };
        match key {
            Key::Ctrl('c') => commands.push(Command::Quit),
            Key::Esc => {}
            Key::Enter => match parse_params(&text) {
                Ok(params) => {
                    self.notice = Some(format!("Run now: {name}…"));
                    commands.push(Command::RunNow { job_id, params });
                }
                Err(detail) => {
                    self.notice = Some(detail);
                    self.input = InputMode::Params { job_id, name, text };
                }
            },
            Key::Backspace => {
                text.pop();
                self.input = InputMode::Params { job_id, name, text };
            }
            Key::Char(c) => {
                text.push(c);
                self.input = InputMode::Params { job_id, name, text };
            }
            Key::Tab | Key::Up | Key::Down | Key::Left | Key::Right | Key::Ctrl(_) => {
                self.input = InputMode::Params { job_id, name, text };
            }
        }
    }

    /// Keys in the `:` prompt. Enter runs `databricks <text> -p <profile>` with the same
    /// placeholders custom commands take, output in a popup; a placeholder with nothing to fill
    /// it keeps the prompt open and says so.
    fn prompt_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let InputMode::Prompt { mut text } = std::mem::take(&mut self.input) else {
            return;
        };
        match key {
            Key::Ctrl('c') => commands.push(Command::Quit),
            Key::Esc => {}
            Key::Enter if text.trim().is_empty() => {}
            Key::Enter => {
                let line = format!("databricks {} -p {{{{profile}}}}", text.trim());
                match expand(&line, &self.template_vars()) {
                    Ok(command) => {
                        self.notice = Some(format!("{command}…"));
                        commands.push(Command::Shell {
                            name: command.clone(),
                            command,
                            output: CommandOutput::Popup,
                        });
                    }
                    Err(error) => {
                        self.notice = Some(error);
                        self.input = InputMode::Prompt { text };
                    }
                }
            }
            Key::Backspace => {
                text.pop();
                self.input = InputMode::Prompt { text };
            }
            Key::Char(c) => {
                text.push(c);
                self.input = InputMode::Prompt { text };
            }
            Key::Tab | Key::Up | Key::Down | Key::Left | Key::Right | Key::Ctrl(_) => {
                self.input = InputMode::Prompt { text };
            }
        }
    }

    /// Rebuilds the visible lists from `all_jobs` and `all_pipelines`, keeping each cursor on the
    /// same item when it survives the filter.
    fn apply_filter(&mut self) {
        let keep_pipeline = self
            .pipelines
            .selected()
            .map(|pipeline| pipeline.id.clone());
        let mut visible: Vec<Pipeline> = self
            .all_pipelines
            .iter()
            .filter(|pipeline| self.filter.matches_pipeline(pipeline, self.me.as_ref()))
            .cloned()
            .collect();
        self.sort_pipelines(&mut visible);
        self.pipelines.set_items(visible);
        if let Some(id) = keep_pipeline {
            self.pipelines.select_where(|pipeline| pipeline.id == id);
        }

        let keep_cluster = self.compute.selected().map(|cluster| cluster.id.clone());
        let mut visible: Vec<Cluster> = self
            .all_compute
            .iter()
            .filter(|cluster| self.filter.matches_cluster(cluster, self.me.as_ref()))
            .cloned()
            .collect();
        // Clusters have no run to sort by; name order is the one that stays put.
        visible.sort_by_cached_key(|cluster| cluster.name.to_lowercase());
        self.compute.set_items(visible);
        if let Some(id) = keep_cluster {
            self.compute.select_where(|cluster| cluster.id == id);
        }

        let keep = self.jobs.selected().map(|job| job.id);
        // The visible list is a copy of the matching jobs: a few hundred small structs per
        // keystroke, and `Selectable` stays a plain list with a cursor.
        let mut visible: Vec<Job> = self
            .all_jobs
            .iter()
            .filter(|job| {
                self.filter
                    .matches(job, self.latest_runs.get(&job.id), self.me.as_ref())
            })
            .cloned()
            .collect();
        self.sort_jobs(&mut visible);
        self.jobs.set_items(visible);
        if let Some(id) = keep {
            self.jobs.select_where(|job| job.id == id);
        }
        self.select_runs();
    }

    /// Orders jobs by `self.sort`; ties and unknown timestamps fall back to the name.
    fn sort_jobs(&self, jobs: &mut [Job]) {
        let name = |job: &Job| job.settings.name.to_lowercase();
        match self.sort {
            Sort::Activity => jobs.sort_by_cached_key(|job| {
                let latest = self.latest_runs.get(&job.id).and_then(|run| run.start_time);
                (Reverse(latest), name(job))
            }),
            Sort::Name => jobs.sort_by_cached_key(name),
            Sort::Created => {
                jobs.sort_by_cached_key(|job| (Reverse(job.created_time), name(job)));
            }
        }
    }

    /// Orders pipelines by `self.sort`. `Created` is name order here: the list response has no
    /// creation time.
    fn sort_pipelines(&self, pipelines: &mut [Pipeline]) {
        let name = |pipeline: &Pipeline| pipeline.name.to_lowercase();
        match self.sort {
            Sort::Activity => pipelines.sort_by_cached_key(|pipeline| {
                let latest = pipeline
                    .latest_updates
                    .first()
                    .and_then(|update| update.creation_time);
                (Reverse(latest), name(pipeline))
            }),
            Sort::Name | Sort::Created => pipelines.sort_by_cached_key(name),
        }
    }

    /// `[4]` is worth a panel only while there is, or may still be, something to show. A
    /// serverless workspace with no warehouses loads an empty list and the panel folds away.
    #[must_use]
    pub const fn show_compute(&self) -> bool {
        self.compute_panel.is_enabled()
            && (self.compute_fetched_at.is_none()
                || !self.all_compute.is_empty()
                || self.compute_error.is_some())
    }

    /// The side panel after `panel`, skipping a hidden compute panel.
    fn next_visible_side(&self, panel: Panel) -> Panel {
        let next = panel.next_side();
        if next == Panel::Compute && !self.show_compute() {
            next.next_side()
        } else {
            next
        }
    }

    fn set_focus(&mut self, panel: Panel) {
        if panel == Panel::Compute && !self.show_compute() {
            return;
        }
        self.focus = panel;
        if panel.is_side() && self.context != panel {
            self.context = panel;
            self.tab = 0;
            self.main_scroll = 0;
        }
    }

    /// Cursor keys act on the focused panel's list, or scroll the main panel's text views.
    fn move_cursor(&mut self, movement: Move) {
        match self.focus {
            Panel::Jobs => {
                self.jobs.apply(movement);
                self.main_scroll = 0;
                self.select_runs();
            }
            Panel::Pipelines => {
                self.pipelines.apply(movement);
                self.main_scroll = 0;
            }
            Panel::Compute => {
                self.compute.apply(movement);
                self.main_scroll = 0;
            }
            Panel::Main => {
                if self.viewing_run.is_none()
                    && self.context == Panel::Jobs
                    && self.active_tab() == Some(Tab::Runs)
                    && let Load::Loaded(runs) = &mut self.runs
                {
                    runs.apply(movement);
                } else {
                    self.main_scroll = scrolled(self.main_scroll, movement);
                }
            }
            Panel::Status => {}
        }
    }

    /// Points the runs view at the selected job and schedules a fetch for when the cursor rests.
    fn select_runs(&mut self) {
        let selected = self.jobs.selected().map(|job| job.id);
        if selected == self.runs_job {
            return;
        }
        self.runs_job = selected;
        self.leave_run_detail();
        let Some(job_id) = selected else {
            self.runs = Load::Idle;
            self.pending_runs = None;
            return;
        };
        let Some(cached) = self.runs_cache.get(&job_id) else {
            self.runs = Load::Loading;
            self.pending_runs = Some((job_id, RUNS_DEBOUNCE_TICKS));
            return;
        };
        // Show what we have at once; refetch behind it only if it has gone stale.
        self.runs = Load::Loaded(Selectable::new(cached.value.clone()));
        let stale = self.age_ticks(cached.at) >= self.runs_ttl_ticks;
        self.pending_runs = stale.then_some((job_id, RUNS_DEBOUNCE_TICKS));
    }

    fn next_tab(&mut self) {
        let len = self.context.tabs().len();
        if len == 0 {
            return;
        }
        self.tab = self.tab.saturating_add(1).checked_rem(len).unwrap_or(0);
        self.main_scroll = 0;
    }

    fn prev_tab(&mut self) {
        let len = self.context.tabs().len();
        if len == 0 {
            return;
        }
        self.tab = self
            .tab
            .checked_sub(1)
            .unwrap_or_else(|| len.saturating_sub(1));
        self.main_scroll = 0;
    }
}

/// One `x` entry over a range: named with its count, carrying one command per row.
const fn bulk(label: String, confirmation: String, commands: Vec<Command>) -> MenuItem {
    MenuItem::Bulk {
        label,
        confirmation,
        commands,
    }
}

/// `3 jobs`, `1 active run`: a count with its noun.
fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// What one task's output reads as: the notebook result, then logs, then the error and its
/// trace, each only when present. Truncation is said, not hidden.
fn output_body(output: &RunOutput) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(notebook) = &output.notebook_output
        && let Some(result) = &notebook.result
    {
        lines.extend(result.lines().map(str::to_owned));
        if notebook.truncated {
            lines.push("[result truncated]".to_owned());
        }
    }
    if let Some(logs) = &output.logs {
        lines.extend(logs.lines().map(str::to_owned));
        if output.logs_truncated {
            lines.push("[logs truncated to the last 5 MB]".to_owned());
        }
    }
    lines.extend(
        output
            .error
            .iter()
            .flat_map(|s| s.lines())
            .map(str::to_owned),
    );
    lines.extend(
        output
            .error_trace
            .iter()
            .flat_map(|s| s.lines())
            .map(str::to_owned),
    );
    if lines.is_empty() {
        lines.push("no output".to_owned());
    }
    lines
}

/// Pretty JSON with `null` members left out: the model fills absent fields with `None`, and a
/// page of `"schedule": null` says nothing.
fn pretty_json(value: &impl serde::Serialize) -> String {
    fn strip(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .filter(|(_, member)| !member.is_null())
                    .map(|(key, member)| (key, strip(member)))
                    .collect(),
            ),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.into_iter().map(strip).collect())
            }
            other => other,
        }
    }
    serde_json::to_value(value)
        .map(strip)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|error| error.to_string())
}

/// A text view's scroll after one cursor movement. `Last` overshoots on purpose: the draw
/// clamps it to the real end and reports back.
const fn scrolled(scroll: usize, movement: Move) -> usize {
    match movement {
        Move::Down => scroll.saturating_add(1),
        Move::Up => scroll.saturating_sub(1),
        Move::PageDown => scroll.saturating_add(list::PAGE),
        Move::PageUp => scroll.saturating_sub(list::PAGE),
        Move::First => 0,
        Move::Last => usize::MAX,
    }
}

/// Pipelines whose latest update just turned into a failure, by name.
fn pipeline_failures(old: &[Pipeline], new: &[Pipeline]) -> Vec<String> {
    let failed = |pipeline: &Pipeline| {
        matches!(
            pipeline.latest_updates.first().map(|update| update.state),
            Some(UpdateState::Failed | UpdateState::Canceled)
        )
    };
    let mut names: Vec<String> = new
        .iter()
        .filter(|pipeline| failed(pipeline))
        .filter(|pipeline| {
            old.iter().any(|before| {
                before.id == pipeline.id
                    && (!failed(before)
                        || before.latest_updates.first().map(|update| &update.id)
                            != pipeline.latest_updates.first().map(|update| &update.id))
            })
        })
        .map(|pipeline| pipeline.name.clone())
        .collect();
    names.sort();
    names
}

#[cfg(test)]
pub mod tests {
    use std::time::Duration;

    use super::*;
    use crate::api::models::{
        CronSchedule, JobSettings, LifeCycleState, PipelineState, PipelineUpdate, ResultState,
        RunState, TaskRun, UpdateState,
    };
    use crate::config::Config;

    pub fn job(id: i64, name: &str) -> Job {
        Job {
            id,
            creator_user_name: "someone@example.com".to_owned(),
            run_as_user_name: "someone@example.com".to_owned(),
            created_time: None,
            settings: JobSettings {
                name: name.to_owned(),
                timeout_seconds: Some(7200),
                max_concurrent_runs: Some(4),
                tags: [("dev", "someone"), ("domain", "okonomi")]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
                format: Some("MULTI_TASK".to_owned()),
                ..JobSettings::default()
            },
        }
    }

    /// A job owned by somebody else: different creator, run-as and `dev` tag.
    pub fn theirs(id: i64, name: &str) -> Job {
        let mut job = job(id, name);
        job.creator_user_name = "other@example.com".to_owned();
        job.run_as_user_name = "other@example.com".to_owned();
        job.settings
            .tags
            .insert("dev".to_owned(), "other".to_owned());
        job
    }

    pub fn run(run_id: i64, start_ms: i64, end_ms: i64, result: Option<ResultState>) -> Run {
        let life_cycle_state = if end_ms == 0 {
            LifeCycleState::Running
        } else {
            LifeCycleState::Terminated
        };
        let ts = |ms: i64| (ms > 0).then(|| jiff::Timestamp::from_millisecond(ms).unwrap());
        Run {
            id: run_id,
            job_id: 1,
            state: RunState {
                life_cycle_state,
                result_state: result,
                state_message: String::new(),
            },
            start_time: ts(start_ms),
            end_time: ts(end_ms),
            page_url: String::new(),
            tasks: Vec::new(),
        }
    }

    /// One task of a run, with its own run id.
    pub fn task(run_id: i64, key: &str, result: Option<ResultState>) -> TaskRun {
        TaskRun {
            run_id,
            task_key: key.to_owned(),
            state: RunState {
                life_cycle_state: LifeCycleState::Terminated,
                result_state: result,
                state_message: if result == Some(ResultState::Failed) {
                    "Workload failed, see run output for details".to_owned()
                } else {
                    String::new()
                },
            },
            start_time: jiff::Timestamp::from_millisecond(1_788_170_938_500).ok(),
            end_time: jiff::Timestamp::from_millisecond(1_788_170_965_431).ok(),
        }
    }

    pub fn pipeline(id: &str, name: &str, creator: &str) -> Pipeline {
        Pipeline {
            id: id.to_owned(),
            name: name.to_owned(),
            state: PipelineState::Idle,
            creator_user_name: creator.to_owned(),
            latest_updates: vec![PipelineUpdate {
                id: "ac18068b-9637-4dcd-a149-03f08eb24e12".to_owned(),
                state: UpdateState::Completed,
                creation_time: jiff::Timestamp::from_millisecond(1_787_784_000_000).ok(),
            }],
        }
    }

    pub fn cluster(id: &str, name: &str, state: ClusterState) -> Cluster {
        Cluster {
            kind: ComputeKind::Cluster,
            id: id.to_owned(),
            name: name.to_owned(),
            creator_user_name: "someone@example.com".to_owned(),
            source: "UI".to_owned(),
            state,
            state_message: String::new(),
            spark_version: "15.4.x-scala2.12".to_owned(),
            node_type_id: "Standard_DS3_v2".to_owned(),
            num_workers: Some(2),
            autoscale: None,
            start_time: None,
        }
    }

    pub fn api_call(path: &str, status: Option<u16>, ms: u64) -> ApiCall {
        ApiCall {
            method: "GET".to_owned(),
            path: path.to_owned(),
            status,
            duration: Duration::from_millis(ms),
        }
    }

    pub fn defaults() -> Loaded {
        Loaded {
            config: Config::default(),
            path: "config.toml".into(),
            found: false,
        }
    }

    fn boom() -> AppError {
        AppError::Internal("boom".to_owned())
    }

    fn nope() -> AppError {
        AppError::Internal("nope".to_owned())
    }

    pub fn app() -> App {
        App::new(
            "dev",
            "https://adb-1.azuredatabricks.net",
            TimeZone::UTC,
            &defaults(),
            &[],
        )
    }

    fn key(key: Key) -> Message {
        Message::Key(key)
    }

    /// `y` opens the copy menu with the URL first; Enter takes it.
    fn copy_url(app: &mut App) -> Vec<Command> {
        app.update(key(Key::Char('y')));
        app.update(key(Key::Enter))
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            app.update(key(Key::Char(c)));
        }
    }

    fn loaded() -> App {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "a"),
            job(2, "b"),
            job(3, "c"),
        ]));
        app
    }

    fn names(app: &App) -> Vec<&str> {
        app.jobs
            .items()
            .iter()
            .map(|job| job.settings.name.as_str())
            .collect()
    }

    #[test]
    fn q_and_ctrl_c_quit_even_while_loading() {
        let mut app = app();
        assert!(app.loading);
        assert_eq!(app.update(key(Key::Char('q'))), vec![Command::Quit]);
        assert_eq!(app.update(key(Key::Ctrl('c'))), vec![Command::Quit]);
        assert_eq!(app.update(key(Key::Ctrl('d'))), vec![], "^D is not quit");
    }

    #[test]
    fn main_panel_text_views_scroll_and_reset() {
        let mut app = loaded();
        press(&mut app, "0]jj");
        assert_eq!(app.active_tab(), Some(Tab::Detail));
        assert_eq!(app.main_scroll, 2);
        app.update(key(Key::Ctrl('d')));
        assert_eq!(app.main_scroll, 12);
        press(&mut app, "k");
        assert_eq!(app.main_scroll, 11);
        press(&mut app, "g");
        assert_eq!(app.main_scroll, 0);
        press(&mut app, "G");
        app.update(Message::ScrollLimit(5));
        assert_eq!(app.main_scroll, 5, "the draw clamps G to the last line");
        press(&mut app, "k");
        assert_eq!(app.main_scroll, 4);
        press(&mut app, "[");
        assert_eq!(app.main_scroll, 0, "a tab change starts at the top");
        press(&mut app, "]jj2j");
        assert_eq!(
            app.main_scroll, 0,
            "moving the side cursor starts at the top"
        );
    }

    #[test]
    fn runs_table_keeps_its_cursor_instead_of_scrolling() {
        let mut app = loaded();
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![run(1, 1, 2, None), run(2, 3, 4, None)],
        });
        press(&mut app, "0j");
        assert_eq!(app.main_scroll, 0);
        assert_eq!(run_index(&app), Some(1));
    }

    fn echo(context: Context, command: &str, confirm: bool) -> CustomCommand {
        CustomCommand {
            name: "Echo".to_owned(),
            key: Some(Key::Char('E')),
            context,
            command: command.to_owned(),
            output: CommandOutput::Popup,
            confirm,
        }
    }

    fn shell(command: &str) -> Command {
        Command::Shell {
            name: "Echo".to_owned(),
            command: command.to_owned(),
            output: CommandOutput::Popup,
        }
    }

    #[test]
    fn custom_command_in_the_menu_and_on_its_key() {
        let mut app = loaded();
        app.custom = vec![echo(
            Context::Jobs,
            "echo {{job_id}} {{name}} -p {{profile}}",
            false,
        )];
        press(&mut app, "jx");
        let InputMode::Menu { items, .. } = &app.input else {
            panic!("{:?}", app.input);
        };
        assert_eq!(
            items.last(),
            Some(&MenuItem::Shell {
                name: "Echo".to_owned(),
                command: "echo 2 b -p dev".to_owned(),
                output: CommandOutput::Popup,
                confirm: false,
            })
        );
        press(&mut app, "jjjj");
        let commands = app.update(key(Key::Enter));
        assert_eq!(
            commands,
            vec![shell("echo 2 b -p dev")],
            "no actions opt-in needed"
        );
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(
            app.update(key(Key::Char('E'))),
            vec![shell("echo 2 b -p dev")]
        );
        press(&mut app, "3");
        assert_eq!(
            app.update(key(Key::Char('E'))),
            vec![],
            "wrong context: nothing"
        );
    }

    #[test]
    fn custom_command_needs_its_placeholders() {
        let mut app = loaded();
        app.custom = vec![echo(Context::Runs, "echo {{run_id}}", false)];
        press(&mut app, "x");
        let InputMode::Menu { items, .. } = &app.input else {
            panic!("{:?}", app.input);
        };
        assert!(
            !items
                .iter()
                .any(|item| matches!(item, MenuItem::Shell { .. })),
            "no run selected, so no runs command"
        );
        app.update(key(Key::Esc));
        let mut app = with_active_run();
        app.custom = vec![echo(Context::Runs, "echo {{run_id}} {{nope}}", false)];
        press(&mut app, "0");
        assert_eq!(app.update(key(Key::Char('E'))), vec![]);
        assert_eq!(app.notice.as_deref(), Some("Echo: no {{nope}} here"));
        app.custom = vec![echo(Context::Runs, "echo {{run_id}}", false)];
        assert_eq!(app.update(key(Key::Char('E'))), vec![shell("echo 10")]);
    }

    #[test]
    fn custom_command_can_ask_first() {
        let mut app = loaded();
        app.custom = vec![echo(Context::Any, "databricks bundle deploy", true)];
        assert_eq!(app.update(key(Key::Char('E'))), vec![]);
        assert!(matches!(
            app.input,
            InputMode::Confirm(MenuItem::Shell { .. })
        ));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![shell("databricks bundle deploy")]
        );
        assert_eq!(app.notice.as_deref(), Some("Echo…"));
    }

    #[test]
    fn shell_output_opens_a_scrolling_popup() {
        let mut app = loaded();
        app.update(Message::ShellFinished {
            name: "Echo".to_owned(),
            output: Ok("a\nb\nc\n".to_owned()),
        });
        let lines = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(
            app.input,
            InputMode::Output {
                title: "Echo".to_owned(),
                lines,
                scroll: 0
            }
        );
        press(&mut app, "jjjj");
        assert!(matches!(app.input, InputMode::Output { scroll: 2, .. }));
        press(&mut app, "g");
        assert!(matches!(app.input, InputMode::Output { scroll: 0, .. }));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::Copy("a\nb\nc".to_owned())]
        );
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal);
        app.update(Message::ShellFinished {
            name: "Echo".to_owned(),
            output: Ok("  \n".to_owned()),
        });
        assert_eq!(app.notice.as_deref(), Some("Echo: no output"));
        app.update(Message::ShellFinished {
            name: "Echo".to_owned(),
            output: Err(boom()),
        });
        assert_eq!(app.notice.as_deref(), Some("Echo: internal error: boom"));
        app.update(Message::ShellExited {
            name: "Deploy".to_owned(),
            detail: "exit status: 0".to_owned(),
        });
        assert_eq!(app.notice.as_deref(), Some("Deploy: exit status: 0"));
    }

    #[test]
    fn menu_offers_pause_or_resume_by_schedule_state() {
        let mut app = loaded();
        let labels = |app: &App| -> Vec<String> {
            let InputMode::Menu { items, .. } = &app.input else {
                panic!("{:?}", app.input);
            };
            items.iter().map(MenuItem::label).collect()
        };
        press(&mut app, "x");
        assert_eq!(
            labels(&app),
            [
                "Run now: a",
                "Run with parameters: a",
                "Pause schedule: a",
                "Resume schedule: a"
            ],
            "schedule unknown: both"
        );
        app.update(key(Key::Esc));
        let mut detailed = job(1, "a");
        detailed.settings.schedule = Some(CronSchedule {
            quartz_cron_expression: "0 0 4 * * ?".to_owned(),
            timezone_id: "Europe/Oslo".to_owned(),
            pause_status: Some("UNPAUSED".to_owned()),
        });
        app.update(Message::JobLoaded(detailed.clone()));
        press(&mut app, "x");
        assert!(labels(&app).contains(&"Pause schedule: a".to_owned()));
        assert!(!labels(&app).contains(&"Resume schedule: a".to_owned()));
        press(&mut app, "jj");
        app.allow_actions = true;
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::SetSchedulePaused {
                job_id: 1,
                paused: true
            }]
        );
        app.update(Message::SchedulePaused {
            job_id: 1,
            paused: true,
        });
        assert_eq!(app.notice.as_deref(), Some("Schedule paused: a"));
        assert!(
            app.jobs
                .selected()
                .unwrap()
                .settings
                .schedule
                .as_ref()
                .unwrap()
                .is_paused()
        );
        press(&mut app, "x");
        assert!(labels(&app).contains(&"Resume schedule: a".to_owned()));
        assert!(!labels(&app).contains(&"Pause schedule: a".to_owned()));
        app.update(key(Key::Esc));
        detailed.settings.schedule = None;
        app.update(Message::JobLoaded(detailed));
        press(&mut app, "x");
        assert!(
            !labels(&app).iter().any(|label| label.contains("schedule")),
            "known to have no schedule: neither"
        );
    }

    #[test]
    fn counts_cover_the_whole_workspace_not_the_filter() {
        let mut app = loaded();
        assert_eq!(app.counts(), Counts::default());
        let mut failed = run(20, 1000, 2000, Some(ResultState::Failed));
        failed.job_id = 2;
        let mut fine = run(30, 1000, 2000, Some(ResultState::Success));
        fine.job_id = 3;
        app.update(Message::RecentRunsLoaded(vec![
            run(10, 1000, 0, None),
            failed,
            fine,
        ]));
        app.update(Message::ComputeLoaded(vec![
            cluster("c1", "up", ClusterState::Running),
            cluster("c2", "down", ClusterState::Terminated),
        ]));
        press(&mut app, "m");
        assert_eq!(
            app.counts(),
            Counts {
                running: 1,
                failed: 1,
                compute_up: 1
            }
        );
    }

    #[test]
    fn json_tab_shows_settings_once_fetched() {
        let mut app = loaded();
        press(&mut app, "ll");
        assert_eq!(app.active_tab(), Some(Tab::Json));
        assert_eq!(app.json_view(), Load::Loading);
        assert_eq!(
            ticks(&mut app, 3),
            vec![
                Command::FetchRuns { job_id: 1 },
                Command::FetchJob { job_id: 1 }
            ]
        );
        let mut full = job(1, "a");
        full.settings.schedule = Some(CronSchedule {
            quartz_cron_expression: "0 0 4 * * ?".to_owned(),
            timezone_id: "Europe/Oslo".to_owned(),
            pause_status: None,
        });
        app.update(Message::JobLoaded(full));
        let Load::Loaded(text) = app.json_view() else {
            panic!("{:?}", app.json_view());
        };
        assert!(
            text.contains("\"quartz_cron_expression\": \"0 0 4 * * ?\""),
            "{text}"
        );
        assert!(!text.contains("null"), "absent fields are left out: {text}");
        assert!(!text.contains("pause_status"), "{text}");
    }

    #[test]
    fn pipeline_json_tab_fetches_the_spec_once() {
        let mut app = with_pipelines();
        ticks(&mut app, 3);
        press(&mut app, "3ll");
        assert_eq!(app.active_tab(), Some(Tab::Json));
        let commands = ticks(&mut app, 1);
        assert_eq!(commands.len(), 1, "{commands:?}");
        let Command::FetchPipeline { pipeline_id } = &commands[0] else {
            panic!("{commands:?}");
        };
        assert_eq!(app.json_view(), Load::Loading);
        assert_eq!(ticks(&mut app, 2), vec![], "one fetch per pipeline");
        app.update(Message::PipelineLoaded {
            pipeline_id: pipeline_id.clone(),
            spec: serde_json::json!({ "name": "felles", "serverless": true, "catalog": null }),
        });
        assert_eq!(
            app.json_view(),
            Load::Loaded("{\n  \"name\": \"felles\",\n  \"serverless\": true\n}".to_owned())
        );
        app.update(Message::PipelineFailed {
            pipeline_id: "nope".to_owned(),
            error: boom(),
        });
        assert_eq!(app.notice.as_deref(), Some("internal error: boom"));
    }

    #[test]
    fn output_tab_fetches_every_task_and_lists_them() {
        let mut app = with_active_run();
        assert_eq!(
            app.output_lines(),
            vec!["Open a run with Enter on the Runs tab to see its output."]
        );
        press(&mut app, "0j");
        app.update(key(Key::Enter));
        let mut detail = run(9, 1000, 2000, Some(ResultState::Failed));
        detail.tasks = vec![
            task(91, "extract", Some(ResultState::Success)),
            task(92, "load", Some(ResultState::Failed)),
        ];
        assert_eq!(
            app.update(Message::RunDetailLoaded(detail)),
            vec![Command::FetchRunOutput { run_id: 92 }],
            "Detail asks for the failed task only"
        );
        press(&mut app, "]]]");
        assert_eq!(app.active_tab(), Some(Tab::Output));
        assert_eq!(
            ticks(&mut app, 1),
            vec![Command::FetchRunOutput { run_id: 91 }],
            "Output asks for the rest"
        );
        assert_eq!(ticks(&mut app, 1), vec![], "once");
        app.update(Message::RunOutputLoaded {
            run_id: 91,
            output: RunOutput {
                notebook_output: Some(crate::api::models::NotebookOutput {
                    result: Some("rows=42".to_owned()),
                    truncated: true,
                }),
                logs: Some("line 1\nline 2".to_owned()),
                logs_truncated: true,
                ..RunOutput::default()
            },
        });
        let lines = app.output_lines();
        assert_eq!(lines[0], "▸ extract  SUCCESS");
        assert_eq!(lines[1], "rows=42");
        assert_eq!(lines[2], "[result truncated]");
        assert_eq!(lines[3], "line 1");
        assert_eq!(lines[5], "[logs truncated to the last 5 MB]");
        assert_eq!(lines[6], "");
        assert_eq!(lines[7], "▸ load  FAILED");
        assert!(lines[8].contains("fetching output"), "{lines:?}");
        assert_eq!(lines.len(), 9, "no trailing blank: {lines:?}");
        app.update(Message::RunOutputFailed {
            run_id: 92,
            error: boom(),
        });
        assert_eq!(app.output_lines()[8], "internal error: boom");
    }

    #[test]
    fn enter_pages_the_json_and_output_tabs() {
        let mut app = loaded();
        press(&mut app, "0ll");
        assert_eq!(app.active_tab(), Some(Tab::Json));
        assert_eq!(app.update(key(Key::Enter)), vec![]);
        assert_eq!(app.notice.as_deref(), Some("Nothing to page yet"));
        app.update(Message::JobLoaded(job(1, "a")));
        let commands = app.update(key(Key::Enter));
        assert!(
            matches!(&commands[..], [Command::Page(text)] if text.starts_with('{')),
            "{commands:?}"
        );
        press(&mut app, "l");
        assert_eq!(app.active_tab(), Some(Tab::Output));
        assert_eq!(app.update(key(Key::Enter)), vec![], "no run viewed");
        press(&mut app, "hhh");
        assert_eq!(app.active_tab(), Some(Tab::Runs));
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![],
            "no runs loaded, so Enter has nothing to open"
        );
    }

    #[test]
    fn y_offers_url_id_name_and_json() {
        let mut app = loaded();
        let labels = |app: &App| -> Vec<String> {
            let InputMode::Menu { items, .. } = &app.input else {
                panic!("{:?}", app.input);
            };
            items.iter().map(MenuItem::label).collect()
        };
        press(&mut app, "y");
        assert_eq!(labels(&app), ["Copy URL", "Copy job ID", "Copy name"]);
        press(&mut app, "j");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::Copy("1".to_owned())],
            "no actions opt-in needed"
        );
        assert_eq!(app.notice.as_deref(), Some("Copied job ID"));
        app.update(Message::JobLoaded(job(1, "a")));
        press(&mut app, "y");
        assert_eq!(labels(&app)[3], "Copy JSON");
        app.update(key(Key::Esc));
        press(&mut app, "1y");
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(app.notice.as_deref(), Some("Nothing selected to copy"));
        app.update(Message::ApiCalled(api_call(
            "/api/2.2/jobs/list?limit=25",
            Some(200),
            84,
        )));
        press(&mut app, "y");
        assert_eq!(
            labels(&app),
            [
                "Copy last request as databricks api",
                "Copy last request as curl"
            ],
            "Status has no selection, but the log has a call"
        );
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::Copy(
                "databricks api get '/api/2.2/jobs/list?limit=25' -p dev".to_owned()
            )]
        );
        press(&mut app, "yj");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::Copy(
                "curl -H \"Authorization: Bearer $DATABRICKS_TOKEN\" 'https://adb-1.azuredatabricks.net/api/2.2/jobs/list?limit=25'".to_owned()
            )]
        );
    }

    #[test]
    fn colon_runs_a_databricks_line_with_the_selection_filled_in() {
        let mut app = loaded();
        press(&mut app, ":");
        assert_eq!(
            app.input,
            InputMode::Prompt {
                text: String::new()
            }
        );
        press(&mut app, "jobs get {{job_id}}");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::Shell {
                name: "databricks jobs get 1 -p dev".to_owned(),
                command: "databricks jobs get 1 -p dev".to_owned(),
                output: CommandOutput::Popup,
            }]
        );
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, ":runs get {{run_id}}");
        assert_eq!(app.update(key(Key::Enter)), vec![]);
        assert_eq!(app.notice.as_deref(), Some("no {{run_id}} here"));
        assert!(
            matches!(&app.input, InputMode::Prompt { text } if text == "runs get {{run_id}}"),
            "the prompt keeps the text: {:?}",
            app.input
        );
        app.update(key(Key::Backspace));
        assert!(matches!(&app.input, InputMode::Prompt { text } if text == "runs get {{run_id}"));
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, ":  ");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![],
            "blank line: nothing runs"
        );
        assert_eq!(app.input, InputMode::Normal);
    }

    #[test]
    fn e_opens_the_config_in_the_editor() {
        let mut app = loaded();
        assert_eq!(app.update(key(Key::Char('e'))), vec![Command::EditConfig]);
        app.update(Message::ShellExited {
            name: "Config".to_owned(),
            detail: "changes apply after a restart".to_owned(),
        });
        assert_eq!(
            app.notice.as_deref(),
            Some("Config: changes apply after a restart")
        );
    }

    #[test]
    fn config_tab_shows_the_effective_toml() {
        let mut app = loaded();
        press(&mut app, "1l");
        assert_eq!(app.active_tab(), Some(Tab::Config));
        assert!(
            app.config_text.contains("expand_focused = true"),
            "{}",
            app.config_text
        );
        assert!(
            app.config_text.contains("max_jobs = 200"),
            "{}",
            app.config_text
        );
        press(&mut app, "0");
        let commands = app.update(key(Key::Enter));
        assert_eq!(commands, vec![Command::Page(app.config_text.clone())]);
    }

    #[test]
    fn display_names_apply_replacements_in_order() {
        let mut app = loaded();
        assert_eq!(
            app.display_name("[dev bk] nightly_ingest"),
            "[dev bk] nightly_ingest"
        );
        app.replacements = vec![
            ("[dev bk] ".to_owned(), String::new()),
            ("_ingest".to_owned(), " ↓".to_owned()),
        ];
        assert_eq!(app.display_name("[dev bk] nightly_ingest"), "nightly ↓");
        press(&mut app, "/dev");
        assert_eq!(
            app.jobs.items().len(),
            0,
            "the filter sees the full name, which lacks it"
        );
    }

    #[test]
    fn capital_f_lists_the_filters_and_applies_one() {
        let mut app = loaded();
        press(&mut app, "/a");
        app.update(key(Key::Enter));
        press(&mut app, "F");
        let InputMode::Menu { items, selected } = &app.input else {
            panic!("{:?}", app.input);
        };
        assert_eq!(*selected, 0, "status all is in force");
        let labels: Vec<String> = items.iter().map(MenuItem::label).collect();
        assert_eq!(
            labels,
            [
                "Show all",
                "Show failed only",
                "Show active only",
                "Mine only: on",
                "Clear text filter /a"
            ]
        );
        press(&mut app, "j");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![],
            "no command: state only"
        );
        assert_eq!(app.filter.status, Status::Failed);
        assert_eq!(app.notice.as_deref(), Some(app.filter_summary().as_str()));
        press(&mut app, "F");
        assert!(matches!(app.input, InputMode::Menu { selected: 1, .. }));
        press(&mut app, "jj");
        app.update(key(Key::Enter));
        assert!(app.filter.mine_only);
        press(&mut app, "F");
        press(&mut app, "jjj");
        app.update(key(Key::Enter));
        assert_eq!(app.filter.text, "");
        press(&mut app, "F");
        let InputMode::Menu { items, .. } = &app.input else {
            panic!("{:?}", app.input);
        };
        assert_eq!(items.len(), 4, "no text, no clear entry");
        assert_eq!(items[3].label(), "Mine only: off");
    }

    #[test]
    fn v_anchors_a_range_that_esc_clears() {
        let mut app = loaded();
        assert_eq!(app.range_len(), None);
        press(&mut app, "vjj");
        assert_eq!(app.range_len(), Some(3));
        assert_eq!(app.jobs.selected_items().len(), 3);
        press(&mut app, "k");
        assert_eq!(app.range_len(), Some(2));
        press(&mut app, "0");
        assert_eq!(app.range_len(), None, "the main panel has no range");
        press(&mut app, "2");
        assert_eq!(app.range_len(), Some(2), "the list kept it");
        app.update(key(Key::Esc));
        assert_eq!(app.range_len(), None);
        assert_eq!(app.focus, Panel::Jobs, "Esc took the range, not the focus");
        press(&mut app, "vj");
        app.update(Message::JobsLoaded(vec![job(1, "a"), job(2, "b")]));
        assert_eq!(app.range_len(), None, "a refetch ends the range");
    }

    #[test]
    fn x_on_a_range_acts_on_every_row() {
        let mut app = loaded();
        let mut active = run(20, 1000, 0, None);
        active.job_id = 2;
        app.update(Message::RecentRunsLoaded(vec![active]));
        // The active run sorted job 2 first and the cursor followed job 1; start from the top.
        press(&mut app, "gvjjx");
        let InputMode::Menu { items, .. } = &app.input else {
            panic!("{:?}", app.input);
        };
        let labels: Vec<String> = items.iter().map(MenuItem::label).collect();
        assert_eq!(
            labels,
            [
                "Run now: 3 jobs",
                "Cancel 1 active run",
                "Pause schedule: 3 jobs",
                "Resume schedule: 3 jobs"
            ]
        );
        assert_eq!(items[1].confirmation(), "Cancel 1 active run?");
        app.allow_actions = true;
        app.update(key(Key::Enter));
        assert!(matches!(
            app.input,
            InputMode::Confirm(MenuItem::Bulk { .. })
        ));
        let commands = app.update(key(Key::Char('y')));
        assert_eq!(commands.len(), 3, "{commands:?}");
        assert!(
            commands
                .iter()
                .all(|command| matches!(command, Command::RunNow { .. }))
        );
        assert_eq!(app.notice.as_deref(), Some("Run now: 3 jobs…"));
        assert_eq!(app.range_len(), None, "the range ends with the action");
        press(&mut app, "vx");
        let InputMode::Menu { items, .. } = &app.input else {
            panic!("{:?}", app.input);
        };
        assert!(
            matches!(items.first(), Some(MenuItem::RunNow { .. })),
            "one row anchored: the single-row menu"
        );
    }

    #[test]
    fn other_keys_do_nothing() {
        assert_eq!(app().update(key(Key::Char('z'))), vec![]);
    }

    #[test]
    fn jobs_loaded_replaces_list_and_stops_loading() {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![job(1, "old")]));
        app.update(Message::JobsLoaded(vec![job(2, "a"), job(3, "b")]));
        assert_eq!(app.jobs.items(), [job(2, "a"), job(3, "b")]);
        assert_eq!(app.jobs.selected(), Some(&job(2, "a")));
        assert!(!app.loading);
    }

    #[test]
    fn tick_advances_spinner_only_while_something_loads() {
        let mut app = app();
        app.update(Message::Tick);
        assert_eq!(app.spinner, 1);
        app.update(Message::JobsLoaded(vec![]));
        app.update(Message::Tick);
        assert_eq!(app.spinner, 2, "pipelines still loading");
        app.update(Message::PipelinesLoaded(vec![]));
        app.update(Message::Tick);
        assert_eq!(app.spinner, 3, "compute still loading");
        app.update(Message::ComputeLoaded(vec![]));
        app.update(Message::Tick);
        assert_eq!(app.spinner, 3);
    }

    #[test]
    fn spinner_wraps() {
        let mut app = app();
        for _ in 0..SPINNER.len() {
            app.update(Message::Tick);
        }
        assert_eq!(app.spinner, 0);
    }

    #[test]
    fn failure_stops_loading_and_keeps_message() {
        let mut app = app();
        app.update(Message::JobsFailed(boom()));
        assert!(!app.loading);
        assert_eq!(app.error, Some(boom()));
    }

    #[test]
    fn failed_refresh_keeps_the_list_and_waits_a_ttl() {
        let mut app = loaded();
        let ttl = app.jobs_ttl_ticks;
        ticks(&mut app, 3); // the debounced runs fetch
        assert_eq!(
            ticks(&mut app, ttl - 3),
            vec![
                Command::FetchJobs { max: 200 },
                Command::FetchRecentRuns { max: 200 }
            ]
        );
        app.update(Message::JobsFailed(boom()));
        assert_eq!(names(&app).len(), 3, "the old list stays");
        assert_eq!(app.error, Some(boom()));
        assert_eq!(app.notice, Some("internal error: boom".to_owned()));
        assert_eq!(
            ticks(&mut app, ttl - 1),
            vec![],
            "no retry storm: nothing until a whole TTL has passed"
        );
        assert_eq!(
            ticks(&mut app, 1).first(),
            Some(&Command::FetchJobs { max: 200 })
        );

        app.update(Message::PipelinesFailed(boom()));
        assert_eq!(ticks(&mut app, ttl - 1), vec![]);
        assert_eq!(
            ticks(&mut app, 1),
            vec![Command::FetchPipelines { max: 200 }]
        );
    }

    #[test]
    fn a_job_turning_red_rings_the_bell_once() {
        let mut app = loaded();
        let mut ok = run(20, 1000, 2000, Some(ResultState::Success));
        ok.job_id = 2;
        let mut first_seen_failed = run(30, 1000, 2000, Some(ResultState::Failed));
        first_seen_failed.job_id = 3;
        assert_eq!(
            app.update(Message::RecentRunsLoaded(vec![ok, first_seen_failed])),
            vec![],
            "first sight is not a transition"
        );
        assert_eq!(app.notice, None);
        let mut failed = run(21, 3000, 4000, Some(ResultState::Timedout));
        failed.job_id = 2;
        assert_eq!(
            app.update(Message::RecentRunsLoaded(vec![failed.clone()])),
            vec![Command::Bell]
        );
        assert_eq!(app.notice, Some("✗ b failed".to_owned()));
        assert!(
            app.latest_runs.contains_key(&3),
            "unswept jobs keep their run"
        );
        app.update(key(Key::Char('k')));
        assert_eq!(
            app.update(Message::RecentRunsLoaded(vec![failed])),
            vec![],
            "same failed run again is not news"
        );
        // Per-job fetches count too: run 21 running again then failing.
        let mut again = run(22, 5000, 6000, Some(ResultState::Failed));
        again.job_id = 2;
        ticks(&mut app, 3);
        let commands = app.update(Message::RunsLoaded {
            job_id: 2,
            runs: vec![again],
        });
        assert!(commands.contains(&Command::Bell));
    }

    #[test]
    fn a_pipeline_update_failing_rings_the_bell() {
        let mut app = with_pipelines();
        let mut pipelines = app.all_pipelines.clone();
        pipelines[0].latest_updates[0].state = UpdateState::Failed;
        assert_eq!(
            app.update(Message::PipelinesLoaded(pipelines.clone())),
            vec![Command::Bell]
        );
        assert_eq!(app.notice, Some("✗ felles_gold failed".to_owned()));
        app.update(key(Key::Char('k')));
        assert_eq!(
            app.update(Message::PipelinesLoaded(pipelines)),
            vec![],
            "still the same failed update"
        );
    }

    #[test]
    fn active_runs_poll_every_five_seconds() {
        let mut app = with_active_run();
        assert_eq!(ticks(&mut app, ACTIVE_POLL_TICKS - 1), vec![]);
        assert_eq!(
            ticks(&mut app, 1),
            vec![Command::FetchRuns { job_id: 1 }],
            "a running run on screen polls fast"
        );
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![run(10, 1000, 2000, Some(ResultState::Success))],
        });
        assert_eq!(
            ticks(&mut app, ACTIVE_POLL_TICKS),
            vec![],
            "settled: back to the TTL"
        );
    }

    #[test]
    fn failed_runs_refresh_keeps_cached_runs() {
        let mut app = with_active_run();
        let shown = app.runs.clone();
        let runs_ttl = app.runs_ttl_ticks;
        ticks(&mut app, runs_ttl);
        assert_eq!(
            app.update(Message::Tick),
            vec![],
            "refetch already in flight"
        );
        app.update(Message::RunsFailed {
            job_id: 1,
            error: boom(),
        });
        assert_eq!(app.runs, shown, "stale beats blank");
        assert_eq!(app.notice, Some("internal error: boom".to_owned()));
        assert_eq!(
            ticks(&mut app, 5),
            vec![],
            "the cache counts as fresh again"
        );
        let mut app = loaded();
        ticks(&mut app, 3);
        app.update(Message::RunsFailed {
            job_id: 1,
            error: boom(),
        });
        assert_eq!(
            app.runs,
            Load::Failed(boom()),
            "nothing cached: the error is the view"
        );
    }

    #[test]
    fn detail_tab_fetches_the_full_job_once_the_cursor_rests() {
        let mut app = loaded();
        press(&mut app, "l");
        assert_eq!(app.active_tab(), Some(Tab::Detail));
        assert_eq!(
            ticks(&mut app, 3),
            vec![
                Command::FetchRuns { job_id: 1 },
                Command::FetchJob { job_id: 1 }
            ],
            "runs debounce first, then the job"
        );
        assert_eq!(ticks(&mut app, 2), vec![], "one in flight");
        let mut full = job(1, "a");
        full.settings.edit_mode = Some("UI_LOCKED".to_owned());
        app.update(Message::JobLoaded(full));
        assert_eq!(
            app.jobs.items()[0].settings.edit_mode.as_deref(),
            Some("UI_LOCKED"),
            "the visible list carries the full settings"
        );
        assert_eq!(
            ticks(&mut app, 2),
            vec![],
            "detailed jobs are not refetched"
        );
        press(&mut app, "j");
        assert_eq!(
            ticks(&mut app, 3),
            vec![
                Command::FetchRuns { job_id: 2 },
                Command::FetchJob { job_id: 2 }
            ]
        );
        app.update(Message::JobFailed {
            job_id: 2,
            error: boom(),
        });
        assert_eq!(app.notice, Some("internal error: boom".to_owned()));
        press(&mut app, "h");
        assert_eq!(ticks(&mut app, 3), vec![], "runs tab asks for nothing");
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        assert!(app.detailed.is_empty(), "a refresh brings list shapes back");
    }

    #[test]
    fn p_lists_every_profile_and_switches() {
        let mut app = loaded();
        assert_eq!(app.update(key(Key::Char('p'))), vec![]);
        assert_eq!(
            app.notice.as_deref(),
            Some("Only one profile in ~/.databrickscfg")
        );
        app.profiles = ["DEFAULT", "dev", "prod"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        press(&mut app, "p");
        assert!(
            matches!(&app.input, InputMode::Menu { items, selected: 1 } if items.len() == 3),
            "the current profile starts selected: {:?}",
            app.input
        );
        press(&mut app, "j");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::SwitchProfile("prod".to_owned())],
            "no actions opt-in needed"
        );
        assert_eq!(app.notice.as_deref(), Some("Switch to prod…"));
    }

    #[test]
    fn a_full_list_says_plus() {
        let mut app = loaded();
        assert_eq!(app.filter_summary(), "3 of 3 jobs");
        app.max_jobs = 3;
        assert_eq!(app.filter_summary(), "3 of 3+ jobs");
    }

    #[test]
    fn f_cycles_the_status_filter_over_the_newest_runs() {
        let mut app = loaded();
        let mut failed = run(20, 1000, 2000, Some(ResultState::Failed));
        failed.job_id = 2;
        let mut running = run(30, 1000, 0, None);
        running.job_id = 3;
        app.update(Message::RecentRunsLoaded(vec![failed, running]));
        press(&mut app, "f");
        assert_eq!(names(&app), ["b"]);
        assert_eq!(app.filter_summary(), "failed only · 1 of 3 jobs");
        assert_eq!(app.notice, Some("Showing failed only".to_owned()));
        press(&mut app, "f");
        assert_eq!(names(&app), ["c"]);
        assert_eq!(app.filter_summary(), "active only · 1 of 3 jobs");
        press(&mut app, "f");
        assert_eq!(names(&app).len(), 3);
        assert_eq!(app.notice, Some("Showing all".to_owned()));
        // Late-arriving runs re-apply the filter, so a job whose run just failed appears.
        press(&mut app, "f");
        let mut late = run(40, 3000, 4000, Some(ResultState::Timedout));
        late.job_id = 1;
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![late],
        });
        assert_eq!(names(&app), ["a", "b"]);
    }

    #[test]
    fn success_after_failure_clears_error() {
        let mut app = app();
        app.update(Message::JobsFailed(boom()));
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        assert_eq!(app.error, None);
    }

    #[test]
    fn digits_tab_enter_and_esc_move_focus() {
        let mut app = app();
        assert_eq!(app.focus, Panel::Jobs);
        app.update(key(Key::Char('1')));
        assert_eq!(app.focus, Panel::Status);
        app.update(key(Key::Tab));
        assert_eq!(app.focus, Panel::Jobs);
        app.update(key(Key::Char('0')));
        assert_eq!(app.focus, Panel::Main);
        app.update(key(Key::Esc));
        assert_eq!(
            app.focus,
            Panel::Jobs,
            "Esc backs out of main to its context"
        );
        app.update(key(Key::Char('3')));
        app.update(key(Key::Enter));
        assert_eq!(app.focus, Panel::Main);
        app.update(key(Key::Char('7')));
        assert_eq!(app.focus, Panel::Main);
    }

    #[test]
    fn plus_cycles_screen_mode() {
        let mut app = app();
        app.update(key(Key::Char('+')));
        assert_eq!(app.mode, ScreenMode::Half);
        app.update(key(Key::Char('+')));
        app.update(key(Key::Char('+')));
        assert_eq!(app.mode, ScreenMode::Normal);
    }

    #[test]
    fn cursor_keys_move_jobs_only_when_jobs_focused() {
        let mut app = loaded();
        app.update(key(Key::Char('j')));
        app.update(key(Key::Down));
        assert_eq!(app.jobs.selected_index(), Some(2));
        app.update(key(Key::Char('k')));
        assert_eq!(app.jobs.selected_index(), Some(1));
        app.update(key(Key::Char('G')));
        assert_eq!(app.jobs.selected_index(), Some(2));
        app.update(key(Key::Char('g')));
        assert_eq!(app.jobs.selected_index(), Some(0));

        app.update(key(Key::Char('3')));
        app.update(key(Key::Char('j')));
        assert_eq!(app.jobs.selected_index(), Some(0));
    }

    #[test]
    fn runs_fetch_fires_once_the_cursor_rests() {
        let mut app = loaded();
        app.update(key(Key::Char('j')));
        app.update(key(Key::Char('j')));
        assert!(app.runs_busy());
        assert_eq!(app.update(Message::Tick), vec![]);
        assert_eq!(app.update(Message::Tick), vec![]);
        assert_eq!(
            app.update(Message::Tick),
            vec![Command::FetchRuns { job_id: 3 }]
        );
        assert_eq!(app.update(Message::Tick), vec![], "fires once");
    }

    #[test]
    fn moving_back_to_the_same_job_does_not_reschedule() {
        let mut app = loaded();
        for _ in 0..RUNS_DEBOUNCE_TICKS {
            app.update(Message::Tick);
        }
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![run(10, 1000, 2000, Some(ResultState::Success))],
        });
        app.update(key(Key::Char('k')));
        assert_eq!(
            app.runs,
            Load::Loaded(Selectable::new(vec![run(
                10,
                1000,
                2000,
                Some(ResultState::Success)
            )]))
        );
    }

    #[test]
    fn replies_for_other_jobs_are_not_shown() {
        let mut app = loaded();
        app.update(key(Key::Char('j')));
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![run(10, 1000, 2000, Some(ResultState::Success))],
        });
        assert_eq!(app.runs, Load::Loading);
        app.update(Message::RunsLoaded {
            job_id: 2,
            runs: vec![run(11, 1000, 2000, Some(ResultState::Failed))],
        });
        assert_eq!(
            app.runs,
            Load::Loaded(Selectable::new(vec![run(
                11,
                1000,
                2000,
                Some(ResultState::Failed)
            )]))
        );
    }

    #[test]
    fn runs_failure_is_kept_for_the_selected_job_only() {
        let mut app = loaded();
        app.update(Message::RunsFailed {
            job_id: 2,
            error: nope(),
        });
        assert_eq!(app.runs, Load::Loading);
        app.update(Message::RunsFailed {
            job_id: 1,
            error: nope(),
        });
        assert_eq!(app.runs, Load::Failed(nope()));
    }

    #[test]
    fn tabs_follow_context_and_wrap() {
        let mut app = loaded();
        assert_eq!(app.active_tab(), Some(Tab::Runs));
        app.update(key(Key::Char('l')));
        assert_eq!(app.active_tab(), Some(Tab::Detail));
        app.update(key(Key::Char(']')));
        assert_eq!(app.active_tab(), Some(Tab::Json));
        app.update(key(Key::Char(']')));
        assert_eq!(app.active_tab(), Some(Tab::Output));
        app.update(key(Key::Char(']')));
        assert_eq!(app.active_tab(), Some(Tab::Runs), "wraps");
        app.update(key(Key::Char('h')));
        assert_eq!(app.active_tab(), Some(Tab::Output));
        press(&mut app, "hh");
        assert_eq!(app.active_tab(), Some(Tab::Detail));
        app.update(key(Key::Char('0')));
        app.update(key(Key::Left));
        assert_eq!(
            app.active_tab(),
            Some(Tab::Runs),
            "main focus keeps jobs context"
        );
        app.update(key(Key::Char('1')));
        assert_eq!(app.context, Panel::Status);
        assert_eq!(app.active_tab(), Some(Tab::Profile));
        app.update(key(Key::Char('3')));
        assert_eq!(app.active_tab(), Some(Tab::Updates));
        app.update(key(Key::Char('l')));
        assert_eq!(app.active_tab(), Some(Tab::Detail));
    }

    #[test]
    fn at_toggles_api_log_and_log_is_capped() {
        let mut app = app();
        assert!(app.show_api_log);
        app.update(key(Key::Char('@')));
        assert!(!app.show_api_log);
        for i in 0..250 {
            app.update(Message::ApiCalled(api_call(&format!("/{i}"), Some(200), 5)));
        }
        assert_eq!(app.api_log.len(), API_LOG_CAPACITY);
        assert_eq!(app.api_log.back().unwrap().path, "/249");
    }

    #[test]
    fn slash_types_a_filter_and_letters_stop_being_bindings() {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "okonomi_gold"),
            job(2, "ems_gold"),
            job(3, "aktorer_ingest"),
        ]));
        press(&mut app, "3/");
        assert_eq!(app.focus, Panel::Pipelines, "/ filters the list in context");
        assert_eq!(app.input, InputMode::Filter);
        press(&mut app, "q");
        assert_eq!(app.filter.text, "q", "q is a letter now, not quit");
        app.update(key(Key::Backspace));
        press(&mut app, "GOLD");
        assert_eq!(names(&app), ["ems_gold", "okonomi_gold"]);
        assert_eq!(app.filter_summary(), "/GOLD · 0 of 0 pipelines");
        app.update(key(Key::Enter));
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(
            names(&app),
            ["ems_gold", "okonomi_gold"],
            "Enter keeps the filter"
        );
        assert_eq!(app.update(key(Key::Char('q'))), vec![Command::Quit]);
    }

    #[test]
    fn esc_clears_the_filter() {
        let mut app = loaded();
        press(&mut app, "/zzz");
        assert!(names(&app).is_empty());
        assert_eq!(app.jobs.selected_index(), None);
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(names(&app), ["a", "b", "c"]);
        assert_eq!(app.filter_summary(), "3 of 3 jobs");
    }

    #[test]
    fn filter_keeps_the_cursor_on_the_same_job() {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "aa"),
            job(2, "ab"),
            job(3, "bb"),
        ]));
        press(&mut app, "G/b");
        assert_eq!(names(&app), ["ab", "bb"]);
        assert_eq!(app.jobs.selected(), Some(&job(3, "bb")));
        assert_eq!(app.runs_job, Some(3), "runs view stays on the same job");
    }

    #[test]
    fn mine_only_needs_me_and_reapplies_when_me_arrives() {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "mine"),
            theirs(2, "theirs"),
        ]));
        press(&mut app, "m");
        assert!(app.filter.mine_only);
        assert!(names(&app).is_empty(), "who am I? nothing matches yet");
        assert_eq!(app.filter_summary(), "mine only · 0 of 2 jobs");
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        assert_eq!(names(&app), ["mine"]);
        press(&mut app, "m");
        assert_eq!(names(&app), ["mine", "theirs"]);
    }

    fn ticks(app: &mut App, n: u64) -> Vec<Command> {
        (0..n).flat_map(|_| app.update(Message::Tick)).collect()
    }

    #[test]
    fn revisiting_a_job_within_ttl_uses_the_cache() {
        let mut app = loaded();
        assert_eq!(ticks(&mut app, 3), vec![Command::FetchRuns { job_id: 1 }]);
        let runs = vec![run(10, 1000, 2000, Some(ResultState::Success))];
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: runs.clone(),
        });
        press(&mut app, "j");
        assert_eq!(ticks(&mut app, 3), vec![Command::FetchRuns { job_id: 2 }]);
        app.update(Message::RunsLoaded {
            job_id: 2,
            runs: vec![],
        });
        press(&mut app, "k");
        assert_eq!(app.runs, Load::Loaded(Selectable::new(runs)));
        assert!(!app.runs_busy());
        assert_eq!(ticks(&mut app, 600), vec![], "a minute of ticks, no calls");
    }

    #[test]
    fn stale_cache_is_shown_then_refreshed() {
        let mut app = loaded();
        ticks(&mut app, 3);
        let runs = vec![run(10, 1000, 2000, None)];
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: runs.clone(),
        });
        press(&mut app, "j");
        let ttl = app.runs_ttl_ticks;
        ticks(&mut app, ttl);
        press(&mut app, "k");
        assert_eq!(
            app.runs,
            Load::Loaded(Selectable::new(runs)),
            "stale data shown at once"
        );
        assert!(app.runs_busy());
        assert_eq!(ticks(&mut app, 3), vec![Command::FetchRuns { job_id: 1 }]);
    }

    #[test]
    fn background_refresh_after_ttl() {
        let mut app = loaded();
        ticks(&mut app, 3);
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![],
        });
        let runs_ttl = app.runs_ttl_ticks;
        assert_eq!(
            ticks(&mut app, runs_ttl),
            vec![Command::FetchRuns { job_id: 1 }]
        );
        assert_eq!(ticks(&mut app, 50), vec![], "one refresh while in flight");
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![],
        });
        let jobs_ttl = app.jobs_ttl_ticks;
        let commands = ticks(&mut app, jobs_ttl);
        assert!(commands.contains(&Command::FetchJobs { max: 200 }));
        assert!(app.loading);
        assert_eq!(
            ticks(&mut app, jobs_ttl),
            vec![],
            "no second jobs fetch while loading"
        );
    }

    #[test]
    fn r_refreshes_the_focused_panel_and_shift_r_everything() {
        let mut app = loaded();
        ticks(&mut app, 3);
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![],
        });
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![
                Command::FetchJobs { max: 200 },
                Command::FetchRecentRuns { max: 200 }
            ]
        );
        assert!(app.loading);
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![],
            "one jobs fetch at a time"
        );
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        press(&mut app, "0");
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![Command::FetchRuns { job_id: 1 }]
        );
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![],
        });
        assert_eq!(
            app.update(key(Key::Char('R'))),
            vec![
                Command::FetchJobs { max: 200 },
                Command::FetchRecentRuns { max: 200 },
                Command::FetchRuns { job_id: 1 }
            ]
        );
    }

    #[test]
    fn jobs_age_counts_ticks() {
        let mut app = app();
        assert_eq!(app.jobs_age(), None);
        app.update(Message::JobsLoaded(vec![]));
        ticks(&mut app, 25);
        assert_eq!(app.jobs_age(), Some(Duration::from_millis(2500)));
    }

    #[test]
    fn replies_for_other_jobs_are_cached_for_later() {
        let mut app = loaded();
        let runs = vec![run(30, 1000, 2000, None)];
        app.update(Message::RunsLoaded {
            job_id: 3,
            runs: runs.clone(),
        });
        assert_eq!(app.runs, Load::Loading);
        // Job 3 now has the newest run, so activity order puts it first.
        press(&mut app, "g");
        assert_eq!(app.runs, Load::Loaded(Selectable::new(runs)));
        assert_eq!(ticks(&mut app, 3), vec![], "no fetch for a cached job");
    }

    #[test]
    fn config_sets_filter_tag_ttls_and_keys() {
        let mut loaded = defaults();
        loaded.found = true;
        loaded.config.mine_only = true;
        loaded.config.dev_tag = Some("bk".to_owned());
        loaded.config.max_jobs = 50;
        loaded.config.runs_ttl_secs = 1;
        loaded
            .config
            .keys
            .insert(Action::NextTab, vec![Key::Char('ø')]);
        let mut app = App::new("dev", "https://h", TimeZone::UTC, &loaded, &[]);
        assert_eq!(app.config_note, "config.toml");
        assert!(app.filter.mine_only);
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        assert_eq!(app.me.as_ref().map(|me| me.tag.as_str()), Some("bk"));
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        press(&mut app, "ø");
        assert_eq!(app.active_tab(), Some(Tab::Detail));
        press(&mut app, "l");
        assert_eq!(
            app.active_tab(),
            Some(Tab::Detail),
            "default binding replaced"
        );
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![
                Command::FetchJobs { max: 50 },
                Command::FetchRecentRuns { max: 50 }
            ]
        );
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        ticks(&mut app, 3);
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![],
        });
        assert_eq!(
            ticks(&mut app, 10),
            vec![Command::FetchRuns { job_id: 1 }],
            "runs ttl of one second"
        );
    }

    /// Jobs loaded, first job's runs loaded with one active run.
    fn with_active_run() -> App {
        let mut app = loaded();
        ticks(&mut app, 3);
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![
                run(10, 1000, 0, None),
                run(9, 1000, 2000, Some(ResultState::Success)),
            ],
        });
        app
    }

    #[test]
    fn x_opens_a_menu_naming_the_job_and_its_active_runs() {
        let mut app = with_active_run();
        press(&mut app, "x");
        assert_eq!(
            app.input,
            InputMode::Menu {
                items: vec![
                    MenuItem::RunNow {
                        job_id: 1,
                        name: "a".to_owned()
                    },
                    MenuItem::RunWith {
                        job_id: 1,
                        name: "a".to_owned()
                    },
                    MenuItem::CancelRun {
                        job_id: 1,
                        run_id: 10
                    },
                    MenuItem::PauseSchedule {
                        job_id: 1,
                        name: "a".to_owned()
                    },
                    MenuItem::ResumeSchedule {
                        job_id: 1,
                        name: "a".to_owned()
                    },
                ],
                selected: 0,
            }
        );
        press(&mut app, "jjjjjj");
        assert!(
            matches!(app.input, InputMode::Menu { selected: 4, .. }),
            "clamped at the end"
        );
        press(&mut app, "kkkk");
        assert!(matches!(app.input, InputMode::Menu { selected: 0, .. }));
        press(&mut app, "q");
        assert!(
            matches!(app.input, InputMode::Menu { .. }),
            "q is not quit in the menu"
        );
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal);
    }

    #[test]
    fn x_elsewhere_explains_itself() {
        let mut app = loaded();
        press(&mut app, "3x");
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(app.notice.as_deref(), Some("No actions for this selection"));
        press(&mut app, "2");
        assert_eq!(app.notice, None, "next key clears the notice");
    }

    #[test]
    fn read_only_refuses_at_enter() {
        let mut app = with_active_run();
        press(&mut app, "x");
        assert_eq!(app.update(key(Key::Enter)), vec![]);
        assert_eq!(app.input, InputMode::Normal);
        assert_eq!(app.notice.as_deref(), Some(READ_ONLY));
    }

    #[test]
    fn actions_take_x_enter_y_and_show_the_name_first() {
        let mut app = with_active_run();
        app.allow_actions = true;
        press(&mut app, "x");
        assert_eq!(app.update(key(Key::Enter)), vec![]);
        assert_eq!(
            app.input,
            InputMode::Confirm(MenuItem::RunNow {
                job_id: 1,
                name: "a".to_owned()
            })
        );
        assert_eq!(
            app.update(key(Key::Char('n'))),
            vec![],
            "anything but y backs out"
        );
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, "xjj");
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::CancelRun {
                job_id: 1,
                run_id: 10
            }]
        );
        assert_eq!(app.notice.as_deref(), Some("Cancel run 10…"));
    }

    #[test]
    fn run_started_shows_a_placeholder_then_refetches() {
        let mut app = with_active_run();
        let commands = app.update(Message::RunStarted {
            job_id: 1,
            run_id: 11,
        });
        assert_eq!(commands, vec![Command::FetchRuns { job_id: 1 }]);
        assert_eq!(app.notice.as_deref(), Some("Started run 11"));
        let Load::Loaded(runs) = &app.runs else {
            panic!("runs should stay loaded");
        };
        assert_eq!(runs.items()[0].id, 11);
        assert_eq!(
            runs.items()[0].state.life_cycle_state,
            LifeCycleState::Pending
        );
        assert_eq!(runs.items().len(), 3);
    }

    #[test]
    fn run_cancelled_marks_it_terminating_then_refetches() {
        let mut app = with_active_run();
        let commands = app.update(Message::RunCancelled {
            job_id: 1,
            run_id: 10,
        });
        assert_eq!(commands, vec![Command::FetchRuns { job_id: 1 }]);
        let Load::Loaded(runs) = &app.runs else {
            panic!("runs should stay loaded");
        };
        assert_eq!(
            runs.items()[0].state.life_cycle_state,
            LifeCycleState::Terminating
        );
    }

    #[test]
    fn action_failure_is_a_notice() {
        let mut app = loaded();
        app.update(Message::ActionFailed(boom()));
        assert_eq!(app.notice.as_deref(), Some("internal error: boom"));
    }

    #[test]
    fn question_mark_opens_help_and_esc_closes_it() {
        let mut app = loaded();
        press(&mut app, "?");
        assert_eq!(app.input, InputMode::Help { scroll: 0 });
        assert_eq!(
            app.update(key(Key::Char('q'))),
            vec![],
            "q closes help, not the app"
        );
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, "?jjk");
        assert_eq!(
            app.input,
            InputMode::Help { scroll: 1 },
            "j and k scroll the list"
        );
        press(&mut app, "kkx");
        assert_eq!(
            app.input,
            InputMode::Help { scroll: 0 },
            "clamped at the top; other keys are ignored"
        );
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal);
    }

    #[test]
    fn o_and_y_use_the_job_url() {
        let mut app = loaded();
        press(&mut app, "j");
        let url = "https://adb-1.azuredatabricks.net/jobs/2".to_owned();
        assert_eq!(
            app.update(key(Key::Char('o'))),
            vec![Command::OpenUrl(url.clone())]
        );
        assert_eq!(
            app.notice.as_deref(),
            Some("Opening https://adb-1.azuredatabricks.net/jobs/2")
        );
        press(&mut app, "0");
        assert_eq!(
            copy_url(&mut app),
            vec![Command::Copy(url)],
            "main keeps jobs context"
        );
        press(&mut app, "3");
        assert_eq!(app.update(key(Key::Char('y'))), vec![]);
        assert_eq!(app.notice.as_deref(), Some("Nothing selected to copy"));
    }

    fn with_pipelines() -> App {
        let mut app = loaded();
        app.update(Message::PipelinesLoaded(vec![
            // Same update time on all three, so activity order falls back to name: p1, p2, p3.
            pipeline("p1", "felles_gold", "someone@example.com"),
            pipeline("p2", "kodeverk_ingest", "other@example.com"),
            pipeline("p3", "okonomi_gold", "someone@example.com"),
        ]));
        app
    }

    #[test]
    fn clusters_list_start_and_terminate() {
        let mut app = loaded();
        assert!(app.compute_loading(), "fetched from launch");
        app.update(Message::ComputeLoaded(vec![
            cluster("c2", "shared-analytics", ClusterState::Running),
            cluster("c1", "someone's interactive", ClusterState::Terminated),
        ]));
        assert!(!app.compute_loading());
        press(&mut app, "4");
        assert_eq!(app.filter_summary(), "2 of 2 compute");
        assert_eq!(app.compute.selected().map(|c| c.id.as_str()), Some("c2"));
        press(&mut app, "j");
        assert_eq!(app.compute.selected().map(|c| c.id.as_str()), Some("c1"));
        assert_eq!(
            copy_url(&mut app),
            vec![Command::Copy(
                "https://adb-1.azuredatabricks.net/compute/clusters/c1".to_owned()
            )]
        );
        app.allow_actions = true;
        press(&mut app, "x");
        assert!(matches!(&app.input, InputMode::Menu { items, .. }
            if items == &[MenuItem::StartCluster { cluster_id: "c1".to_owned(), name: "someone's interactive".to_owned() }]));
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::StartCluster {
                cluster_id: "c1".to_owned()
            }]
        );
        assert_eq!(
            app.update(Message::ClusterStarted {
                cluster_id: "c1".to_owned()
            }),
            vec![Command::FetchCompute { max: 200 }]
        );
        assert_eq!(
            app.compute.selected().map(|c| c.state),
            Some(ClusterState::Pending),
            "optimistic until the refetch"
        );
        press(&mut app, "kx");
        assert!(matches!(&app.input, InputMode::Menu { items, .. }
            if matches!(items.first(), Some(MenuItem::TerminateCluster { .. }))));
        app.update(key(Key::Esc));
        press(&mut app, "f");
        assert_eq!(app.filter_summary(), "failed only · 0 of 2 compute");
        press(&mut app, "f");
        assert_eq!(app.filter_summary(), "active only · 2 of 2 compute");
    }

    #[test]
    fn compute_false_in_config_never_shows_or_fetches() {
        let mut loaded = defaults();
        loaded.config.compute = false;
        let mut app = App::new(
            "dev",
            "https://adb-1.azuredatabricks.net",
            TimeZone::UTC,
            &loaded,
            &[],
        );
        assert!(!app.show_compute());
        assert!(!app.compute_loading(), "no launch fetch, so no spinner");
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        press(&mut app, "4");
        assert_eq!(app.focus, Panel::Jobs);
        let commands = app.update(key(Key::Char('R')));
        assert!(commands.contains(&Command::FetchJobs { max: 200 }));
        assert!(
            !commands.contains(&Command::FetchCompute { max: 200 }),
            "refresh everything leaves compute alone"
        );
    }

    #[test]
    fn empty_compute_hides_the_panel_and_skips_it() {
        let mut app = loaded();
        assert!(app.show_compute(), "still loading: shown with a spinner");
        app.update(Message::ComputeLoaded(vec![]));
        assert!(!app.show_compute(), "nothing to show");
        press(&mut app, "4");
        assert_eq!(app.focus, Panel::Jobs, "4 does nothing");
        press(&mut app, "3");
        app.update(key(Key::Tab));
        assert_eq!(app.focus, Panel::Status, "Tab skips the hidden panel");
        app.update(Message::ComputeLoaded(vec![cluster(
            "w1",
            "Serverless Starter Warehouse",
            ClusterState::Terminated,
        )]));
        assert!(app.show_compute());
        press(&mut app, "3");
        app.update(key(Key::Tab));
        assert_eq!(app.focus, Panel::Compute);
        app.update(Message::ComputeFailed(boom()));
        assert!(app.show_compute(), "an error is worth showing");
    }

    #[test]
    fn warehouses_get_start_and_stop() {
        let mut app = loaded();
        let mut warehouse = cluster("w1", "shared-bi", ClusterState::Running);
        warehouse.kind = ComputeKind::Warehouse;
        app.update(Message::ComputeLoaded(vec![warehouse]));
        app.allow_actions = true;
        press(&mut app, "4x");
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::StopWarehouse {
                warehouse_id: "w1".to_owned()
            }]
        );
        app.update(Message::ClusterTerminated {
            cluster_id: "w1".to_owned(),
        });
        assert_eq!(
            app.compute.selected().map(|row| row.state),
            Some(ClusterState::Terminating)
        );
        app.update(Message::ComputeLoaded(vec![{
            let mut stopped = cluster("w1", "shared-bi", ClusterState::Terminated);
            stopped.kind = ComputeKind::Warehouse;
            stopped
        }]));
        press(&mut app, "x");
        assert!(matches!(&app.input, InputMode::Menu { items, .. }
            if matches!(items.first(), Some(MenuItem::StartWarehouse { .. }))));
    }

    #[test]
    fn pipelines_load_filter_and_move_like_jobs() {
        let mut app = with_pipelines();
        assert!(!app.pipelines_loading());
        assert_eq!(app.pipelines.counter(), "1 of 3");
        press(&mut app, "3jj");
        assert_eq!(app.pipelines.selected().map(|p| p.id.as_str()), Some("p3"));
        assert_eq!(app.active_tab(), Some(Tab::Updates));
        assert_eq!(app.filter_summary(), "3 of 3 pipelines");
        press(&mut app, "/gold");
        assert_eq!(
            app.focus,
            Panel::Pipelines,
            "filter stays on the pipelines panel"
        );
        assert_eq!(app.pipelines.counter(), "2 of 2");
        assert_eq!(
            app.pipelines.selected().map(|p| p.id.as_str()),
            Some("p3"),
            "cursor kept"
        );
        assert_eq!(
            app.jobs.counter(),
            "0 of 0",
            "the same filter applies to jobs"
        );
        app.update(key(Key::Esc));
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        press(&mut app, "m");
        assert_eq!(app.pipelines.counter(), "2 of 2");
        assert_eq!(app.filter_summary(), "mine only · 2 of 3 pipelines");
    }

    #[test]
    fn pipelines_refresh_and_url() {
        let mut app = with_pipelines();
        press(&mut app, "3");
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![Command::FetchPipelines { max: 200 }]
        );
        assert!(app.pipelines_loading());
        assert_eq!(app.update(key(Key::Char('r'))), vec![], "one at a time");
        app.update(Message::PipelinesFailed(boom()));
        assert_eq!(app.pipelines_error, Some(boom()));
        assert!(!app.pipelines_loading());
        assert_eq!(
            copy_url(&mut app),
            vec![Command::Copy(
                "https://adb-1.azuredatabricks.net/pipelines/p1".to_owned()
            )]
        );
        press(&mut app, "1");
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![
                Command::FetchJobs { max: 200 },
                Command::FetchRecentRuns { max: 200 },
                Command::FetchPipelines { max: 200 }
            ],
            "status refreshes both lists"
        );
    }

    #[test]
    fn latest_run_per_job_comes_from_recent_runs_and_per_job_fetches() {
        let mut app = loaded();
        let mut newest = run(30, 3000, 4000, Some(ResultState::Failed));
        newest.job_id = 2;
        let mut older = run(20, 1000, 2000, Some(ResultState::Success));
        older.job_id = 2;
        let mut other = run(40, 500, 600, None);
        other.job_id = 3;
        app.update(Message::RecentRunsLoaded(vec![
            newest.clone(),
            older,
            other,
        ]));
        assert_eq!(app.latest_runs[&2], newest);
        assert_eq!(app.latest_runs[&3].id, 40);
        assert!(!app.latest_runs.contains_key(&1));
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![run(11, 5000, 0, None), run(10, 1000, 2000, None)],
        });
        assert_eq!(
            app.latest_runs[&1].id, 11,
            "per-job fetch fills in the newest"
        );
        app.update(Message::Clock(
            jiff::Timestamp::from_millisecond(9000).unwrap(),
        ));
        assert_eq!(app.now.unwrap().as_millisecond(), 9000);
    }

    fn run_index(app: &App) -> Option<usize> {
        match &app.runs {
            Load::Loaded(runs) => runs.selected_index(),
            _ => None,
        }
    }

    #[test]
    fn runs_cursor_moves_only_on_the_runs_tab_in_main() {
        let mut app = with_active_run();
        assert_eq!(run_index(&app), Some(0));
        press(&mut app, "j");
        assert_eq!(
            run_index(&app),
            None,
            "jobs focus moves jobs; job 2's runs are loading"
        );
        press(&mut app, "k0j");
        assert_eq!(run_index(&app), Some(1));
        press(&mut app, "j");
        assert_eq!(run_index(&app), Some(1), "clamped");
        press(&mut app, "lj");
        assert_eq!(run_index(&app), Some(1), "detail tab has no cursor");
        press(&mut app, "hk");
        assert_eq!(run_index(&app), Some(0));
    }

    #[test]
    fn enter_opens_run_detail_and_esc_backs_out_one_level_at_a_time() {
        let mut app = with_active_run();
        press(&mut app, "0");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::FetchRunDetail { run_id: 10 }]
        );
        assert_eq!(app.viewing_run, Some(10));
        assert_eq!(app.run_detail, Load::Loading);
        let mut stale = run(9, 1000, 2000, Some(ResultState::Success));
        stale.tasks.clear();
        app.update(Message::RunDetailLoaded(stale));
        assert_eq!(
            app.run_detail,
            Load::Loading,
            "another run's detail is ignored"
        );
        let detail = run(10, 1000, 0, None);
        app.update(Message::RunDetailLoaded(detail.clone()));
        assert_eq!(app.run_detail, Load::Loaded(detail));
        press(&mut app, "j");
        assert_eq!(
            run_index(&app),
            Some(0),
            "no cursor moves while viewing a run"
        );
        assert_eq!(
            app.update(key(Key::Char('r'))),
            vec![
                Command::FetchRuns { job_id: 1 },
                Command::FetchRunDetail { run_id: 10 }
            ]
        );
        app.update(key(Key::Esc));
        assert_eq!(app.viewing_run, None);
        assert_eq!(app.focus, Panel::Main);
        app.update(key(Key::Esc));
        assert_eq!(app.focus, Panel::Jobs);
    }

    #[test]
    fn failed_tasks_fetch_their_output_once() {
        let mut app = with_active_run();
        press(&mut app, "0");
        app.update(key(Key::Enter));
        let mut detail = run(10, 1000, 2000, Some(ResultState::Failed));
        detail.tasks = vec![
            task(71, "ok", Some(ResultState::Success)),
            task(72, "boom", Some(ResultState::Failed)),
        ];
        assert_eq!(
            app.update(Message::RunDetailLoaded(detail.clone())),
            vec![Command::FetchRunOutput { run_id: 72 }],
            "only the failed task's output is fetched"
        );
        assert_eq!(app.run_outputs.get(&72), Some(&Load::Loading));
        let output = RunOutput {
            error: Some("ValueError: nope".to_owned()),
            ..RunOutput::default()
        };
        app.update(Message::RunOutputLoaded {
            run_id: 72,
            output: output.clone(),
        });
        assert_eq!(
            app.run_outputs.get(&72),
            Some(&Load::Loaded(output.clone()))
        );
        assert_eq!(
            app.update(Message::RunDetailLoaded(detail)),
            vec![],
            "a refetched run keeps the output it already has"
        );
        app.update(Message::RunOutputFailed {
            run_id: 99,
            error: boom(),
        });
        assert_eq!(app.run_outputs.len(), 1, "unknown task ids are ignored");
        app.update(key(Key::Esc));
        assert!(
            app.run_outputs.is_empty(),
            "leaving the run drops its outputs"
        );
        app.update(Message::RunOutputLoaded { run_id: 72, output });
        assert!(app.run_outputs.is_empty(), "late replies are dropped");
    }

    #[test]
    fn leaving_the_job_leaves_its_run_detail() {
        let mut app = with_active_run();
        press(&mut app, "0");
        app.update(key(Key::Enter));
        press(&mut app, "2j");
        assert_eq!(app.viewing_run, None);
    }

    #[test]
    fn menu_in_main_cancels_the_selected_run_only() {
        let mut app = with_active_run();
        press(&mut app, "0jx");
        assert!(
            matches!(&app.input, InputMode::Menu { items, .. } if !items.iter().any(|item| matches!(item, MenuItem::CancelRun { .. }))),
            "run 9 succeeded, so nothing to cancel"
        );
        app.update(key(Key::Esc));
        press(&mut app, "kx");
        assert!(matches!(&app.input, InputMode::Menu { items, .. }
            if items[2] == MenuItem::CancelRun { job_id: 1, run_id: 10 }));
    }

    #[test]
    fn repair_is_offered_for_failed_runs() {
        let mut app = loaded();
        app.allow_actions = true;
        ticks(&mut app, 3);
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![
                run(10, 1000, 2000, Some(ResultState::Failed)),
                run(9, 1000, 2000, Some(ResultState::Success)),
            ],
        });
        press(&mut app, "x");
        assert!(
            matches!(&app.input, InputMode::Menu { items, .. }
            if items[2] == MenuItem::RepairRun { job_id: 1, run_id: 10 }),
            "side panel: the newest run failed"
        );
        press(&mut app, "jj");
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::RepairRun {
                job_id: 1,
                run_id: 10
            }]
        );
        assert_eq!(
            app.update(Message::RunRepaired {
                job_id: 1,
                run_id: 10
            }),
            vec![Command::FetchRuns { job_id: 1 }]
        );
        let Load::Loaded(runs) = &app.runs else {
            panic!("runs are loaded");
        };
        assert_eq!(
            runs.items()[0].state.life_cycle_state,
            LifeCycleState::Pending
        );
        assert_eq!(runs.items()[0].state.result_state, None);
        assert_eq!(app.notice, Some("Repair requested for run 10".to_owned()));
        // From the table, only the row under the cursor; run 9 succeeded.
        press(&mut app, "0jx");
        assert!(
            matches!(&app.input, InputMode::Menu { items, .. } if !items.iter().any(|item| matches!(item, MenuItem::RepairRun { .. })))
        );
    }

    #[test]
    fn capital_a_enables_actions_after_a_yes() {
        let mut app = with_active_run();
        press(&mut app, "A");
        assert_eq!(app.input, InputMode::ConfirmActions);
        press(&mut app, "n");
        assert!(!app.allow_actions, "anything but y leaves it off");
        press(&mut app, "Ay");
        assert!(app.allow_actions);
        assert_eq!(
            app.notice,
            Some("Actions enabled for this session".to_owned())
        );
        press(&mut app, "x");
        app.update(key(Key::Enter));
        assert!(
            matches!(app.input, InputMode::Confirm(_)),
            "the menu now confirms"
        );
        app.update(key(Key::Esc));
        press(&mut app, "A");
        assert!(!app.allow_actions, "off again without a question");
        assert_eq!(app.input, InputMode::Normal);
    }

    #[test]
    fn run_with_parameters_prompts_then_sends() {
        let mut app = with_active_run();
        press(&mut app, "xj");
        app.update(key(Key::Enter));
        assert_eq!(app.notice.as_deref(), Some(READ_ONLY), "read-only refuses");
        app.allow_actions = true;
        press(&mut app, "xj");
        app.update(key(Key::Enter));
        assert_eq!(
            app.input,
            InputMode::Params {
                job_id: 1,
                name: "a".to_owned(),
                text: String::new(),
            }
        );
        press(&mut app, "datex=1");
        app.update(key(Key::Backspace));
        app.update(key(Key::Backspace));
        app.update(key(Key::Backspace));
        press(&mut app, "=2026-09-01 mode");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![],
            "a bare word is refused"
        );
        assert!(matches!(&app.input, InputMode::Params { text, .. } if text.ends_with("mode")));
        assert!(app.notice.as_deref().unwrap_or("").contains("mode"));
        press(&mut app, "=full");
        assert_eq!(
            app.update(key(Key::Enter)),
            vec![Command::RunNow {
                job_id: 1,
                params: [("date", "2026-09-01"), ("mode", "full")]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect(),
            }]
        );
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, "xj");
        app.update(key(Key::Enter));
        press(&mut app, "x=1");
        app.update(key(Key::Esc));
        assert_eq!(app.input, InputMode::Normal, "Esc cancels");
    }

    #[test]
    fn run_urls_follow_the_cursor() {
        let mut app = with_active_run();
        assert_eq!(
            copy_url(&mut app),
            vec![Command::Copy(
                "https://adb-1.azuredatabricks.net/jobs/1".to_owned()
            )]
        );
        press(&mut app, "0j");
        assert_eq!(
            copy_url(&mut app),
            vec![Command::Copy(
                "https://adb-1.azuredatabricks.net/jobs/1/runs/9".to_owned()
            )]
        );
    }

    #[test]
    fn pipeline_menu_starts_and_stops() {
        let mut app = with_pipelines();
        app.allow_actions = true;
        press(&mut app, "3x");
        assert_eq!(
            app.input,
            InputMode::Menu {
                items: vec![MenuItem::StartUpdate {
                    pipeline_id: "p1".to_owned(),
                    name: "felles_gold".to_owned(),
                }],
                selected: 0,
            },
            "idle pipeline: start only"
        );
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::StartUpdate {
                pipeline_id: "p1".to_owned()
            }]
        );
        let commands = app.update(Message::UpdateStarted {
            pipeline_id: "p1".to_owned(),
            update_id: "abcdef12-0000".to_owned(),
        });
        assert_eq!(commands, vec![Command::FetchPipelines { max: 200 }]);
        assert_eq!(app.notice.as_deref(), Some("Started update abcdef12"));
        let shown = app.pipelines.selected().unwrap();
        assert_eq!(shown.state, PipelineState::Starting);
        assert_eq!(shown.latest_updates[0].id, "abcdef12-0000");
        assert_eq!(
            app.all_pipelines[0].state,
            PipelineState::Starting,
            "both lists patched"
        );
        app.update(Message::PipelinesLoaded(app.all_pipelines.clone()));
        press(&mut app, "x");
        assert!(
            matches!(&app.input, InputMode::Menu { items, .. } if items.len() == 2),
            "an active pipeline can be stopped"
        );
        press(&mut app, "j");
        app.update(key(Key::Enter));
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::StopPipeline {
                pipeline_id: "p1".to_owned()
            }]
        );
        app.update(Message::PipelineStopped {
            pipeline_id: "p1".to_owned(),
        });
        assert_eq!(
            app.pipelines.selected().unwrap().state,
            PipelineState::Stopping
        );
    }

    #[test]
    fn sort_cycles_activity_name_created_with_name_tiebreak() {
        let ts = |ms: i64| jiff::Timestamp::from_millisecond(ms).ok();
        let mut app = app();
        let mut b = job(1, "b");
        b.created_time = ts(1000);
        let mut a = job(2, "a");
        a.created_time = ts(3000);
        let mut c = job(3, "c");
        c.created_time = ts(2000);
        app.update(Message::JobsLoaded(vec![b, a, c]));
        assert_eq!(
            names(&app),
            ["a", "b", "c"],
            "no activity known: name order"
        );
        let mut run_b = run(10, 5000, 6000, Some(ResultState::Success));
        run_b.job_id = 1;
        let mut run_c = run(11, 9000, 9500, Some(ResultState::Failed));
        run_c.job_id = 3;
        app.update(Message::RecentRunsLoaded(vec![run_c, run_b]));
        assert_eq!(
            names(&app),
            ["c", "b", "a"],
            "newest run first, never-run last"
        );
        press(&mut app, "s");
        assert_eq!(app.sort, Sort::Name);
        assert_eq!(names(&app), ["a", "b", "c"]);
        assert_eq!(app.notice.as_deref(), Some("Sorted by name"));
        press(&mut app, "s");
        assert_eq!(names(&app), ["a", "c", "b"], "newest created first");
        press(&mut app, "s");
        assert_eq!(app.sort, Sort::Activity);
        assert_eq!(names(&app), ["c", "b", "a"]);
    }

    #[test]
    fn pipelines_sort_by_latest_update() {
        let mut app = app();
        let mut old = pipeline("p1", "b_old", "x@example.com");
        old.latest_updates[0].creation_time = jiff::Timestamp::from_millisecond(1000).ok();
        let mut new = pipeline("p2", "c_new", "x@example.com");
        new.latest_updates[0].creation_time = jiff::Timestamp::from_millisecond(2000).ok();
        let mut never = pipeline("p3", "a_never", "x@example.com");
        never.latest_updates.clear();
        app.update(Message::PipelinesLoaded(vec![old, never, new]));
        let names: Vec<&str> = app
            .pipelines
            .items()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["c_new", "b_old", "a_never"]);
        press(&mut app, "s");
        let names: Vec<&str> = app
            .pipelines
            .items()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["a_never", "b_old", "c_new"]);
    }

    #[test]
    fn me_failure_is_kept() {
        let mut app = app();
        app.update(Message::MeFailed(boom()));
        assert_eq!(app.me_error, Some(boom()));
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        assert_eq!(app.me_error, None);
        assert_eq!(app.me.as_ref().map(|me| me.tag.as_str()), Some("someone"));
    }
}
