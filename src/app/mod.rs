//! Application state and the one place it changes.

mod message;

pub use message::{Key, Message};

use crate::api::models::Job;

/// What the main loop does after a message has been handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// All application state. Rendering is a pure function of this.
#[derive(Debug, Default)]
pub struct App {
    pub jobs: Vec<Job>,
}

impl App {
    /// Folds one message into state. No IO happens here; quitting is signalled with
    /// `Flow::Quit` so the terminal restore in `main` gets to run.
    pub fn update(&mut self, message: Message) -> Flow {
        match message {
            Message::Key(Key::Char('q')) => Flow::Quit,
            Message::Key(Key::Char(_)) => Flow::Continue,
            Message::JobsLoaded(jobs) => {
                self.jobs = jobs;
                Flow::Continue
            }
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

    #[test]
    fn q_quits() {
        let mut app = App::default();
        assert_eq!(app.update(Message::Key(Key::Char('q'))), Flow::Quit);
    }

    #[test]
    fn other_keys_continue() {
        let mut app = App::default();
        assert_eq!(app.update(Message::Key(Key::Char('j'))), Flow::Continue);
    }

    #[test]
    fn jobs_loaded_replaces_list() {
        let mut app = App::default();
        app.update(Message::JobsLoaded(vec![job(1, "old")]));
        assert_eq!(
            app.update(Message::JobsLoaded(vec![job(2, "a"), job(3, "b")])),
            Flow::Continue
        );
        assert_eq!(app.jobs, vec![job(2, "a"), job(3, "b")]);
    }
}
