//! Application state and the one place it changes.

mod message;

pub use message::{Key, Message};

/// What the main loop does after a message has been handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// All application state. Rendering is a pure function of this.
#[derive(Debug, Default)]
pub struct App {}

impl App {
    /// Folds one message into state. No IO happens here; quitting is signalled with
    /// `Flow::Quit` so the terminal guard in `main` gets to run.
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        clippy::missing_const_for_fn,
        reason = "M0 has no state to mutate yet; rustc flags these as unfulfilled once M1 adds some"
    )]
    pub fn update(&mut self, message: Message) -> Flow {
        match message {
            Message::Key(Key::Char('q')) => Flow::Quit,
            Message::Key(Key::Char(_)) => Flow::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
