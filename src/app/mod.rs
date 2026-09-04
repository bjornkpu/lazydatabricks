//! Application state and the one place it changes.

mod filter;
mod focus;
mod list;
mod message;

use std::collections::VecDeque;

pub use filter::{Filter, Me};
pub use focus::{Panel, ScreenMode, Tab};
use jiff::tz::TimeZone;
pub use list::{Move, Selectable};
pub use message::{ApiCall, Command, Key, Message};

use crate::api::models::{Job, Run};

/// Spinner frames, one per `Tick` while loading.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// Ticks the cursor must rest on a job before its runs are fetched. Holding `j` fires one
/// request, not one per row.
const RUNS_DEBOUNCE_TICKS: u8 = 3;
/// API log entries kept; older ones fall off.
const API_LOG_CAPACITY: usize = 200;

/// A remote value and where its fetch stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Load<T> {
    /// Nothing to fetch (no job selected).
    #[default]
    Idle,
    Loading,
    Loaded(T),
    /// The full error chain, ready to display.
    Failed(String),
}

impl<T> Load<T> {
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// All application state. Rendering is a pure function of this.
#[derive(Debug)]
pub struct App {
    pub profile: String,
    pub host: String,
    /// Zone for rendering timestamps. An input, so tests can pin UTC.
    pub tz: TimeZone,
    /// Who the token belongs to, once the SCIM call has answered.
    pub me: Option<Me>,
    pub me_error: Option<String>,
    /// Every job fetched. `jobs` is the filtered view of this.
    pub all_jobs: Vec<Job>,
    /// The visible jobs, with the cursor.
    pub jobs: Selectable<Job>,
    pub filter: Filter,
    /// `/` was pressed: keys edit `filter.text` until Enter or Esc.
    pub filtering: bool,
    /// A jobs fetch is in flight. True from launch until the first `JobsLoaded` or `JobsFailed`.
    pub loading: bool,
    /// Index into `SPINNER`.
    pub spinner: usize,
    pub error: Option<String>,
    pub focus: Panel,
    /// The side panel whose selection the main panel shows. Always a side panel.
    pub context: Panel,
    /// Index into `context.tabs()`.
    pub tab: usize,
    pub mode: ScreenMode,
    /// Runs of the selected job.
    pub runs: Load<Vec<Run>>,
    /// The job `runs` belongs to, or is being fetched for.
    pub runs_job: Option<i64>,
    /// Runs fetch waiting for the cursor to rest: (job, ticks left).
    pending_runs: Option<(i64, u8)>,
    pub api_log: VecDeque<ApiCall>,
    pub show_api_log: bool,
}

impl App {
    /// A freshly launched app: `main` has already kicked off the jobs and `Me` fetches.
    #[must_use]
    pub fn new(profile: &str, host: &str, tz: TimeZone) -> Self {
        Self {
            profile: profile.to_owned(),
            host: host.to_owned(),
            tz,
            me: None,
            me_error: None,
            all_jobs: Vec::new(),
            jobs: Selectable::default(),
            filter: Filter::default(),
            filtering: false,
            loading: true,
            spinner: 0,
            error: None,
            focus: Panel::Jobs,
            context: Panel::Jobs,
            tab: 0,
            mode: ScreenMode::Normal,
            runs: Load::Idle,
            runs_job: None,
            pending_runs: None,
            api_log: VecDeque::new(),
            show_api_log: true,
        }
    }

    /// Folds one message into state and returns the side effects `main` should run. No IO
    /// happens here; quitting is `Command::Quit` so the terminal restore in `main` gets to run.
    pub fn update(&mut self, message: Message) -> Vec<Command> {
        let mut commands = Vec::new();
        match message {
            Message::Key(key) if self.filtering => self.filter_key(key, &mut commands),
            Message::Key(key) => self.key(key, &mut commands),
            Message::Tick => {
                if self.loading || self.runs.is_loading() {
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
                        commands.push(Command::FetchRuns { job_id });
                    } else {
                        self.pending_runs = Some((job_id, left));
                    }
                }
            }
            Message::JobsLoaded(jobs) => {
                self.all_jobs = jobs;
                self.loading = false;
                self.error = None;
                self.apply_filter();
            }
            Message::JobsFailed(error) => {
                self.loading = false;
                self.error = Some(error);
            }
            Message::RunsLoaded { job_id, runs } => {
                // A reply for a job the cursor has since left is stale; drop it.
                if self.runs_job == Some(job_id) {
                    self.runs = Load::Loaded(runs);
                }
            }
            Message::RunsFailed { job_id, error } => {
                if self.runs_job == Some(job_id) {
                    self.runs = Load::Failed(error);
                }
            }
            Message::ApiCalled(call) => {
                if self.api_log.len() >= API_LOG_CAPACITY {
                    self.api_log.pop_front();
                }
                self.api_log.push_back(call);
            }
            Message::MeLoaded(email) => {
                self.me = Some(Me::from_email(&email));
                self.me_error = None;
                if self.filter.mine_only {
                    self.apply_filter();
                }
            }
            Message::MeFailed(error) => self.me_error = Some(error),
        }
        commands
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
        parts.push(format!(
            "{} of {}",
            self.jobs.items().len(),
            self.all_jobs.len()
        ));
        parts.join(" · ")
    }

    /// Keys outside filter editing.
    fn key(&mut self, key: Key, commands: &mut Vec<Command>) {
        match key {
            Key::Char('q') | Key::CtrlC => commands.push(Command::Quit),
            Key::Char('+') => self.mode = self.mode.next(),
            Key::Char('@') => self.show_api_log = !self.show_api_log,
            Key::Char('/') => {
                self.set_focus(Panel::Jobs);
                self.filtering = true;
            }
            Key::Char('m') => {
                self.filter.mine_only = !self.filter.mine_only;
                self.apply_filter();
            }
            Key::Tab => self.set_focus(self.focus.next_side()),
            Key::Enter => self.set_focus(Panel::Main),
            Key::Esc => {
                if self.focus == Panel::Main {
                    self.set_focus(self.context);
                }
            }
            Key::Char('j') | Key::Down => self.move_cursor(Move::Down),
            Key::Char('k') | Key::Up => self.move_cursor(Move::Up),
            Key::Char('g') => self.move_cursor(Move::First),
            Key::Char('G') => self.move_cursor(Move::Last),
            Key::Char('l' | ']') | Key::Right => self.next_tab(),
            Key::Char('h' | '[') | Key::Left => self.prev_tab(),
            Key::Char(digit) => {
                if let Some(panel) = Panel::from_digit(digit) {
                    self.set_focus(panel);
                }
            }
            Key::Backspace => {}
        }
    }

    /// Keys while `/` filter editing is active. Letters go to the filter, not to bindings.
    fn filter_key(&mut self, key: Key, commands: &mut Vec<Command>) {
        match key {
            Key::CtrlC => commands.push(Command::Quit),
            Key::Enter => self.filtering = false,
            Key::Esc => {
                self.filtering = false;
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

    /// Rebuilds the visible list from `all_jobs`, keeping the cursor on the same job when it
    /// survives the filter.
    fn apply_filter(&mut self) {
        let keep = self.jobs.selected().map(|job| job.id);
        // The visible list is a copy of the matching jobs: a few hundred small structs per
        // keystroke, and `Selectable` stays a plain list with a cursor.
        let visible: Vec<Job> = self
            .all_jobs
            .iter()
            .filter(|job| self.filter.matches(job, self.me.as_ref()))
            .cloned()
            .collect();
        self.jobs.set_items(visible);
        if let Some(id) = keep {
            self.jobs.select_where(|job| job.id == id);
        }
        self.select_runs();
    }

    fn set_focus(&mut self, panel: Panel) {
        self.focus = panel;
        if panel.is_side() && self.context != panel {
            self.context = panel;
            self.tab = 0;
        }
    }

    /// Cursor keys act on the focused panel's list. Only jobs has one so far.
    fn move_cursor(&mut self, movement: Move) {
        match self.focus {
            Panel::Jobs => {
                self.jobs.apply(movement);
                self.select_runs();
            }
            Panel::Status | Panel::Pipelines | Panel::Main => {}
        }
    }

    /// Points the runs view at the selected job and schedules a fetch for when the cursor rests.
    fn select_runs(&mut self) {
        let selected = self.jobs.selected().map(|job| job.id);
        if selected == self.runs_job {
            return;
        }
        self.runs_job = selected;
        self.runs = if selected.is_some() {
            Load::Loading
        } else {
            Load::Idle
        };
        self.pending_runs = selected.map(|job_id| (job_id, RUNS_DEBOUNCE_TICKS));
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
    use crate::api::models::{JobSettings, LifeCycleState, ResultState, RunState};

    pub fn job(id: i64, name: &str) -> Job {
        Job {
            id,
            creator_user_name: "someone@example.com".to_owned(),
            run_as_user_name: "someone@example.com".to_owned(),
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
        }
    }

    pub fn api_call(path: &str, status: Option<u16>, ms: u64) -> ApiCall {
        ApiCall {
            method: "GET",
            path: path.to_owned(),
            status,
            duration: Duration::from_millis(ms),
        }
    }

    pub fn app() -> App {
        App::new("dev", "https://adb-1.azuredatabricks.net", TimeZone::UTC)
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
        assert_eq!(app.spinner, 1);
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
        app.update(Message::JobsFailed("boom".to_owned()));
        assert!(!app.loading);
        assert_eq!(app.error.as_deref(), Some("boom"));
    }

    #[test]
    fn success_after_failure_clears_error() {
        let mut app = app();
        app.update(Message::JobsFailed("boom".to_owned()));
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
        assert!(app.runs.is_loading());
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
            Load::Loaded(vec![run(10, 1000, 2000, Some(ResultState::Success))])
        );
    }

    #[test]
    fn stale_runs_are_dropped() {
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
            Load::Loaded(vec![run(11, 1000, 2000, Some(ResultState::Failed))])
        );
    }

    #[test]
    fn runs_failure_is_kept_for_the_selected_job_only() {
        let mut app = loaded();
        app.update(Message::RunsFailed {
            job_id: 2,
            error: "nope".to_owned(),
        });
        assert_eq!(app.runs, Load::Loading);
        app.update(Message::RunsFailed {
            job_id: 1,
            error: "nope".to_owned(),
        });
        assert_eq!(app.runs, Load::Failed("nope".to_owned()));
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
        assert_eq!(app.active_tab(), None);
        app.update(key(Key::Char('l')));
        assert_eq!(app.tab, 0);
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
        assert_eq!(app.focus, Panel::Jobs, "/ focuses jobs");
        assert!(app.filtering);
        press(&mut app, "q");
        assert_eq!(app.filter.text, "q", "q is a letter now, not quit");
        app.update(key(Key::Backspace));
        press(&mut app, "GOLD");
        assert_eq!(names(&app), ["okonomi_gold", "ems_gold"]);
        assert_eq!(app.filter_summary(), "/GOLD · 2 of 3");
        app.update(key(Key::Enter));
        assert!(!app.filtering);
        assert_eq!(
            names(&app),
            ["okonomi_gold", "ems_gold"],
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
        assert!(!app.filtering);
        assert_eq!(names(&app), ["a", "b", "c"]);
        assert_eq!(app.filter_summary(), "3 of 3");
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
        assert_eq!(app.filter_summary(), "mine only · 0 of 2");
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        assert_eq!(names(&app), ["mine"]);
        press(&mut app, "m");
        assert_eq!(names(&app), ["mine", "theirs"]);
    }

    #[test]
    fn me_failure_is_kept() {
        let mut app = app();
        app.update(Message::MeFailed("403".to_owned()));
        assert_eq!(app.me_error.as_deref(), Some("403"));
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        assert_eq!(app.me_error, None);
        assert_eq!(app.me.as_ref().map(|me| me.tag.as_str()), Some("someone"));
    }
}
