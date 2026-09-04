//! Application state and the one place it changes.

mod filter;
mod focus;
mod keys;
mod list;
mod menu;
mod message;

use std::cmp::Reverse;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

pub use filter::{Filter, Me};
pub use focus::{Panel, ScreenMode, Tab};
use jiff::tz::TimeZone;
pub use keys::{Action, Keymap};
pub use list::{Move, Selectable};
pub use menu::{InputMode, MenuItem};
pub use message::{ApiCall, Command, Key, Message};

use crate::api::models::{
    Job, LifeCycleState, Pipeline, PipelineState, PipelineUpdate, Run, UpdateState,
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
/// Heartbeat period. The input thread sends `Message::Tick` this often; ages and TTLs count
/// ticks, so tests drive time by sending ticks.
pub const TICK: Duration = Duration::from_millis(100);
/// Shown when an action is chosen without the opt-in.
const READ_ONLY: &str =
    "Read-only: start with --allow-actions or set allow_actions = true in config";
/// Ticks per second at `TICK` = 100ms. Turns config seconds into tick counts.
const TICKS_PER_SECOND: u64 = 10;

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
    /// Config override for the `dev` tag that marks a job as mine.
    dev_tag: Option<String>,
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
    /// Every pipeline fetched. `pipelines` is the filtered view of this.
    pub all_pipelines: Vec<Pipeline>,
    pub pipelines: Selectable<Pipeline>,
    /// Tick the pipelines fetch started on, while one is in flight.
    pipelines_inflight: Option<u64>,
    pipelines_fetched_at: Option<u64>,
    pub pipelines_error: Option<AppError>,
    /// The visible jobs, with the cursor.
    pub jobs: Selectable<Job>,
    pub filter: Filter,
    /// Where keys go: normal bindings, the filter, the `x` menu or its confirmation.
    pub input: InputMode,
    /// One-line feedback shown in place of the hint bar until the next key.
    pub notice: Option<String>,
    /// Run-now and cancel are allowed. Off by default: reading is safe, triggering is not.
    pub allow_actions: bool,
    pub theme: Theme,
    /// Order of both lists. Applied in `apply_filter`.
    pub sort: Sort,
    /// A jobs fetch is in flight. True from launch until the first `JobsLoaded` or `JobsFailed`,
    /// then again during refreshes; the old list stays on screen meanwhile.
    pub loading: bool,
    /// Tick the current `all_jobs` arrived on.
    jobs_fetched_at: Option<u64>,
    /// Index into `SPINNER`.
    pub spinner: usize,
    pub error: Option<AppError>,
    pub focus: Panel,
    /// The side panel whose selection the main panel shows. Always a side panel.
    pub context: Panel,
    /// Index into `context.tabs()`.
    pub tab: usize,
    pub mode: ScreenMode,
    /// Runs of the selected job, with the main panel's cursor.
    pub runs: Load<Selectable<Run>>,
    /// The run opened with Enter from the runs table, if any. Esc backs out.
    pub viewing_run: Option<i64>,
    pub run_detail: Load<Run>,
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
    pub fn new(profile: &str, host: &str, tz: TimeZone, loaded: &Loaded) -> Self {
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
            keys: Keymap::with_overrides(&config.keys),
            config_note,
            dev_tag: config.dev_tag.clone(),
            max_jobs: config.max_jobs,
            jobs_ttl_ticks: config.jobs_ttl_secs.saturating_mul(TICKS_PER_SECOND),
            runs_ttl_ticks: config.runs_ttl_secs.saturating_mul(TICKS_PER_SECOND),
            ticks: 0,
            me: None,
            me_error: None,
            all_jobs: Vec::new(),
            all_pipelines: Vec::new(),
            pipelines: Selectable::default(),
            pipelines_inflight: Some(0),
            pipelines_fetched_at: None,
            pipelines_error: None,
            jobs: Selectable::default(),
            filter: Filter {
                text: config.filter.clone().unwrap_or_default(),
                mine_only: config.mine_only,
            },
            input: InputMode::Normal,
            notice: None,
            allow_actions: config.allow_actions,
            theme: config.theme,
            sort: config.sort,
            loading: true,
            jobs_fetched_at: None,
            spinner: 0,
            error: None,
            focus: Panel::Jobs,
            context: Panel::Jobs,
            tab: 0,
            mode: ScreenMode::Normal,
            runs: Load::Idle,
            viewing_run: None,
            run_detail: Load::Idle,
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
            Message::JobsLoaded(jobs) => {
                self.all_jobs = jobs;
                self.jobs_fetched_at = Some(self.ticks);
                self.loading = false;
                self.error = None;
                self.apply_filter();
            }
            Message::JobsFailed(error) => {
                self.loading = false;
                self.error = Some(error);
            }
            Message::PipelinesLoaded(pipelines) => {
                self.all_pipelines = pipelines;
                self.pipelines_fetched_at = Some(self.ticks);
                self.pipelines_inflight = None;
                self.pipelines_error = None;
                self.apply_filter();
            }
            Message::PipelinesFailed(error) => {
                self.pipelines_inflight = None;
                self.pipelines_error = Some(error);
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
            Message::RecentRunsLoaded(runs) => {
                // Newest first, so the first run seen for a job is its latest.
                for run in runs {
                    self.latest_runs.entry(run.job_id).or_insert(run);
                }
                // Activity order depends on these.
                self.apply_filter();
            }
            Message::RunsLoaded { job_id, runs } => self.on_runs_loaded(job_id, runs),
            Message::RunsFailed { job_id, error } => {
                if self.runs_inflight == Some(job_id) {
                    self.runs_inflight = None;
                }
                if self.runs_job == Some(job_id) {
                    self.show_runs(Load::Failed(error));
                }
            }
            Message::RunDetailLoaded(run) => {
                if self.viewing_run == Some(run.id) {
                    self.run_detail = Load::Loaded(run);
                }
            }
            Message::RunDetailFailed { run_id, error } => {
                if self.viewing_run == Some(run_id) {
                    self.run_detail = Load::Failed(error);
                }
            }
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
            InputMode::Menu { .. } => self.menu_key(key),
            InputMode::Confirm(_) => self.confirm_key(key, commands),
            InputMode::Help => {
                if matches!(key, Key::Esc | Key::Char('?' | 'q')) {
                    self.input = InputMode::Normal;
                }
            }
        }
    }

    /// One heartbeat: the clock, the spinner, the debounced runs fetch and TTL-driven refreshes.
    fn tick(&mut self, commands: &mut Vec<Command>) {
        self.ticks = self.ticks.saturating_add(1);
        if self.loading || self.pipelines_loading() || self.runs_busy() {
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
        if let Some(job_id) = self.runs_job
            && self.pending_runs.is_none()
            && self
                .runs_cache
                .get(&job_id)
                .is_some_and(|cached| self.age_ticks(cached.at) >= self.runs_ttl_ticks)
        {
            self.refresh_runs(commands);
        }
    }

    /// A job's runs arrived: cache them, show them if that job is selected, note its newest run.
    fn on_runs_loaded(&mut self, job_id: i64, runs: Vec<Run>) {
        if self.runs_inflight == Some(job_id) {
            self.runs_inflight = None;
        }
        if let Some(latest) = runs.first() {
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
        if self.runs_job == Some(job_id) {
            if let Load::Loaded(runs) = &mut self.runs
                && let Some(run) = runs.items_mut().iter_mut().find(|run| run.id == run_id)
            {
                run.state.life_cycle_state = LifeCycleState::Terminating;
            }
            self.refresh_runs(commands);
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
            // Rebuilt from scratch so jobs whose runs were deleted do not keep a stale age.
            self.latest_runs.clear();
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
        self.run_detail = Load::Idle;
    }

    /// What the status panel says about the list: `mine only · /gold · 22 of 85`. Never a
    /// mystery why a list looks short.
    #[must_use]
    pub fn filter_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.filter.mine_only {
            parts.push("mine only".to_owned());
        }
        if !self.filter.text.is_empty() {
            parts.push(format!("/{}", self.filter.text));
        }
        // Counts follow the panel in context, so the status line explains the list you look at.
        let (visible, total, what) = if self.context == Panel::Pipelines {
            (
                self.pipelines.items().len(),
                self.all_pipelines.len(),
                "pipelines",
            )
        } else {
            (self.jobs.items().len(), self.all_jobs.len(), "jobs")
        };
        parts.push(format!("{visible} of {total} {what}"));
        parts.join(" · ")
    }

    /// Keys outside filter editing: bound actions first, then the fixed digit keys.
    fn key(&mut self, key: Key, commands: &mut Vec<Command>) {
        let Some(action) = self.keys.action(key) else {
            if let Key::Char(digit) = key
                && let Some(panel) = Panel::from_digit(digit)
            {
                self.set_focus(panel);
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
            Action::Help => self.input = InputMode::Help,
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
            Action::Copy => {
                if let Some(url) = self.selected_url() {
                    self.notice = Some(format!("Copied {url}"));
                    commands.push(Command::Copy(url));
                } else {
                    self.notice = Some("Nothing selected to copy".to_owned());
                }
            }
            Action::MineOnly => {
                self.filter.mine_only = !self.filter.mine_only;
                self.apply_filter();
            }
            Action::Refresh => match self.focus {
                Panel::Main => self.refresh_runs(commands),
                Panel::Jobs => self.refresh_jobs(commands),
                Panel::Pipelines => self.refresh_pipelines(commands),
                Panel::Status => {
                    self.refresh_jobs(commands);
                    self.refresh_pipelines(commands);
                }
            },
            Action::RefreshAll => {
                self.refresh_jobs(commands);
                self.refresh_pipelines(commands);
                self.refresh_runs(commands);
            }
            Action::NextPanel => self.set_focus(self.focus.next_side()),
            Action::Open => {
                if self.focus == Panel::Main
                    && self.viewing_run.is_none()
                    && let Some(run_id) = self.selected_run().map(|run| run.id)
                {
                    self.viewing_run = Some(run_id);
                    self.run_detail = Load::Loading;
                    commands.push(Command::FetchRunDetail { run_id });
                } else {
                    self.set_focus(Panel::Main);
                }
            }
            Action::Back => {
                if self.viewing_run.is_some() {
                    self.leave_run_detail();
                } else if self.focus == Panel::Main {
                    self.set_focus(self.context);
                }
            }
            Action::Down => self.move_cursor(Move::Down),
            Action::Up => self.move_cursor(Move::Up),
            Action::First => self.move_cursor(Move::First),
            Action::Last => self.move_cursor(Move::Last),
            Action::NextTab => self.next_tab(),
            Action::PrevTab => self.prev_tab(),
        }
    }

    /// Keys while `/` filter editing is active. Letters go to the filter, not to bindings.
    fn filter_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        match key {
            Key::CtrlC => commands.push(Command::Quit),
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
            Key::Tab | Key::Left | Key::Right => {}
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

    /// Actions valid for what is selected: run the job, cancel any of its active runs.
    fn menu_items(&self) -> Vec<MenuItem> {
        if self.context == Panel::Pipelines {
            return self.pipeline_menu_items();
        }
        if self.context != Panel::Jobs {
            return Vec::new();
        }
        let Some(job) = self.jobs.selected() else {
            return Vec::new();
        };
        let mut items = vec![MenuItem::RunNow {
            job_id: job.id,
            name: job.settings.name.clone(),
        }];
        let cancel = |run: &Run| MenuItem::CancelRun {
            job_id: job.id,
            run_id: run.id,
        };
        if self.focus == Panel::Main {
            // The table has a cursor: offer cancel for that row only.
            if let Some(run) = self.selected_run()
                && run.state.life_cycle_state.is_active()
            {
                items.push(cancel(run));
            }
        } else if let Load::Loaded(runs) = &self.runs {
            items.extend(
                runs.items()
                    .iter()
                    .filter(|run| run.state.life_cycle_state.is_active())
                    .map(cancel),
            );
        }
        items
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

    /// Keys while the menu is open: move, choose, or close.
    fn menu_key(&mut self, key: Key) {
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
                if self.allow_actions {
                    self.input = InputMode::Confirm(item);
                } else {
                    self.notice = Some(READ_ONLY.to_owned());
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
            self.notice = Some(format!("{}…", item.label()));
            commands.push(item.command());
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

        let keep = self.jobs.selected().map(|job| job.id);
        // The visible list is a copy of the matching jobs: a few hundred small structs per
        // keystroke, and `Selectable` stays a plain list with a cursor.
        let mut visible: Vec<Job> = self
            .all_jobs
            .iter()
            .filter(|job| self.filter.matches(job, self.me.as_ref()))
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

    fn set_focus(&mut self, panel: Panel) {
        self.focus = panel;
        if panel.is_side() && self.context != panel {
            self.context = panel;
            self.tab = 0;
        }
    }

    /// Cursor keys act on the focused panel's list.
    fn move_cursor(&mut self, movement: Move) {
        match self.focus {
            Panel::Jobs => {
                self.jobs.apply(movement);
                self.select_runs();
            }
            Panel::Pipelines => self.pipelines.apply(movement),
            Panel::Main => {
                if self.viewing_run.is_none()
                    && self.context == Panel::Jobs
                    && self.active_tab() == Some(Tab::Runs)
                    && let Load::Loaded(runs) = &mut self.runs
                {
                    runs.apply(movement);
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
    }
}

#[cfg(test)]
pub mod tests {
    use std::time::Duration;

    use super::*;
    use crate::api::models::{
        JobSettings, LifeCycleState, PipelineState, PipelineUpdate, ResultState, RunState,
        UpdateState,
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
            },
        }
    }

    /// A job owned by somebody else: different creator and `dev` tag.
    pub fn theirs(id: i64, name: &str) -> Job {
        let mut job = job(id, name);
        job.creator_user_name = "other@example.com".to_owned();
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
        )
    }

    fn key(key: Key) -> Message {
        Message::Key(key)
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
        assert_eq!(app.update(key(Key::CtrlC)), vec![Command::Quit]);
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
        assert_eq!(app.spinner, 2);
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
        assert_eq!(app.active_tab(), Some(Tab::Runs));
        app.update(key(Key::Char('h')));
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
        let mut app = App::new("dev", "https://h", TimeZone::UTC, &loaded);
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
                    MenuItem::CancelRun {
                        job_id: 1,
                        run_id: 10
                    },
                ],
                selected: 0,
            }
        );
        press(&mut app, "jjj");
        assert!(
            matches!(app.input, InputMode::Menu { selected: 1, .. }),
            "clamped at the end"
        );
        press(&mut app, "k");
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
        press(&mut app, "xj");
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
        assert_eq!(app.input, InputMode::Help);
        assert_eq!(
            app.update(key(Key::Char('q'))),
            vec![],
            "q closes help, not the app"
        );
        assert_eq!(app.input, InputMode::Normal);
        press(&mut app, "?");
        app.update(key(Key::Char('j')));
        assert_eq!(
            app.input,
            InputMode::Help,
            "other keys are ignored while help is up"
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
            app.update(key(Key::Char('y'))),
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
            app.update(key(Key::Char('y'))),
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
            matches!(&app.input, InputMode::Menu { items, .. } if items.len() == 1),
            "run 9 is finished, so only run-now"
        );
        app.update(key(Key::Esc));
        press(&mut app, "kx");
        assert!(matches!(&app.input, InputMode::Menu { items, .. }
            if items[1] == MenuItem::CancelRun { job_id: 1, run_id: 10 }));
    }

    #[test]
    fn run_urls_follow_the_cursor() {
        let mut app = with_active_run();
        assert_eq!(
            app.update(key(Key::Char('y'))),
            vec![Command::Copy(
                "https://adb-1.azuredatabricks.net/jobs/1".to_owned()
            )]
        );
        press(&mut app, "0j");
        assert_eq!(
            app.update(key(Key::Char('y'))),
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
