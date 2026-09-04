//! Application state and the one place it changes.

mod message;

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
    pub jobs: Vec<Job>,
    /// A fetch is in flight. True from launch until the first `JobsLoaded` or `JobsFailed`.
    pub loading: bool,
    /// Index into `SPINNER`.
    pub spinner: usize,
    pub error: Option<String>,
}

impl Default for App {
    /// A freshly launched app: `main` has already kicked off the first fetch.
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            loading: true,
            spinner: 0,
            error: None,
        }
    }
}

impl App {
    /// Folds one message into state. No IO happens here; quitting is signalled with
    /// `Flow::Quit` so the terminal restore in `main` gets to run.
    pub fn update(&mut self, message: Message) -> Flow {
        match message {
            Message::Key(Key::Char('q')) => return Flow::Quit,
            Message::Key(Key::Char(_)) => {}
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
                self.jobs = jobs;
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

    #[test]
    fn q_quits_even_while_loading() {
        let mut app = App::default();
        assert!(app.loading);
        assert_eq!(app.update(Message::Key(Key::Char('q'))), Flow::Quit);
    }

    #[test]
    fn other_keys_continue() {
        let mut app = App::default();
        assert_eq!(app.update(Message::Key(Key::Char('j'))), Flow::Continue);
    }

    #[test]
    fn jobs_loaded_replaces_list_and_stops_loading() {
        let mut app = App::default();
        app.update(Message::JobsLoaded(vec![job(1, "old")]));
        assert_eq!(
            app.update(Message::JobsLoaded(vec![job(2, "a"), job(3, "b")])),
            Flow::Continue
        );
        assert_eq!(app.jobs, vec![job(2, "a"), job(3, "b")]);
        assert!(!app.loading);
    }

    #[test]
    fn tick_advances_spinner_only_while_loading() {
        let mut app = App::default();
        app.update(Message::Tick);
        assert_eq!(app.spinner, 1);
        app.update(Message::JobsLoaded(vec![]));
        app.update(Message::Tick);
        assert_eq!(app.spinner, 1);
    }

    #[test]
    fn spinner_wraps() {
        let mut app = App::default();
        for _ in 0..SPINNER.len() {
            app.update(Message::Tick);
        }
        assert_eq!(app.spinner, 0);
    }

    #[test]
    fn failure_stops_loading_and_keeps_message() {
        let mut app = App::default();
        app.update(Message::JobsFailed("boom".to_owned()));
        assert!(!app.loading);
        assert_eq!(app.error.as_deref(), Some("boom"));
    }

    #[test]
    fn success_after_failure_clears_error() {
        let mut app = App::default();
        app.update(Message::JobsFailed("boom".to_owned()));
        app.update(Message::JobsLoaded(vec![job(1, "a")]));
        assert_eq!(app.error, None);
    }
}
