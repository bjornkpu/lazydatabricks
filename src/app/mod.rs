//! Application state and the one place it changes.

mod focus;
mod list;
mod message;

pub use focus::{Panel, ScreenMode};
pub use list::{Move, Selectable};
pub use message::{Key, Message};

use crate::api::models::Job;

/// Spinner frames, one per `Tick` while loading.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// What the main loop does after a message has been handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// All application state. Rendering is a pure function of this.
#[derive(Debug)]
pub struct App {
    pub profile: String,
    pub host: String,
    pub jobs: Selectable<Job>,
    /// A fetch is in flight. True from launch until the first `JobsLoaded` or `JobsFailed`.
    pub loading: bool,
    /// Index into `SPINNER`.
    pub spinner: usize,
    pub error: Option<String>,
    pub focus: Panel,
    pub mode: ScreenMode,
}

impl App {
    /// A freshly launched app: `main` has already kicked off the first fetch.
    #[must_use]
    pub fn new(profile: &str, host: &str) -> Self {
        Self {
            profile: profile.to_owned(),
            host: host.to_owned(),
            jobs: Selectable::default(),
            loading: true,
            spinner: 0,
            error: None,
            focus: Panel::Jobs,
            mode: ScreenMode::Normal,
        }
    }

    /// Folds one message into state. No IO happens here; quitting is signalled with
    /// `Flow::Quit` so the terminal restore in `main` gets to run.
    pub fn update(&mut self, message: Message) -> Flow {
        match message {
            Message::Key(Key::Char('q') | Key::CtrlC) => return Flow::Quit,
            Message::Key(Key::Char('+')) => self.mode = self.mode.next(),
            Message::Key(Key::Tab) => self.focus = self.focus.next_side(),
            Message::Key(Key::Enter) => self.focus = Panel::Main,
            Message::Key(Key::Char('j') | Key::Down) => self.move_cursor(Move::Down),
            Message::Key(Key::Char('k') | Key::Up) => self.move_cursor(Move::Up),
            Message::Key(Key::Char('g')) => self.move_cursor(Move::First),
            Message::Key(Key::Char('G')) => self.move_cursor(Move::Last),
            Message::Key(Key::Char(digit)) => {
                if let Some(panel) = Panel::from_digit(digit) {
                    self.focus = panel;
                }
            }
            Message::Tick => {
                if self.loading {
                    self.spinner = self
                        .spinner
                        .wrapping_add(1)
                        .checked_rem(SPINNER.len())
                        .unwrap_or(0);
                }
            }
            Message::JobsLoaded(jobs) => {
                self.jobs.set_items(jobs);
                self.loading = false;
                self.error = None;
            }
            Message::JobsFailed(error) => {
                self.loading = false;
                self.error = Some(error);
            }
        }
        Flow::Continue
    }

    /// Cursor keys act on the focused panel's list. Only jobs has one so far.
    fn move_cursor(&mut self, movement: Move) {
        match self.focus {
            Panel::Jobs => self.jobs.apply(movement),
            Panel::Status | Panel::Pipelines | Panel::Main => {}
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::api::models::JobSettings;

    pub fn job(id: i64, name: &str) -> Job {
        Job {
            job_id: id,
            settings: JobSettings {
                name: name.to_owned(),
            },
        }
    }

    pub fn app() -> App {
        App::new("dev", "https://adb-1.azuredatabricks.net")
    }

    fn key(key: Key) -> Message {
        Message::Key(key)
    }

    #[test]
    fn q_and_ctrl_c_quit_even_while_loading() {
        let mut app = app();
        assert!(app.loading);
        assert_eq!(app.update(key(Key::Char('q'))), Flow::Quit);
        assert_eq!(app.update(key(Key::CtrlC)), Flow::Quit);
    }

    #[test]
    fn other_keys_continue() {
        assert_eq!(app().update(key(Key::Char('z'))), Flow::Continue);
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
    fn tick_advances_spinner_only_while_loading() {
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
    fn digits_tab_and_enter_move_focus() {
        let mut app = app();
        assert_eq!(app.focus, Panel::Jobs);
        app.update(key(Key::Char('1')));
        assert_eq!(app.focus, Panel::Status);
        app.update(key(Key::Tab));
        assert_eq!(app.focus, Panel::Jobs);
        app.update(key(Key::Char('0')));
        assert_eq!(app.focus, Panel::Main);
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
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "a"),
            job(2, "b"),
            job(3, "c"),
        ]));
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
}
