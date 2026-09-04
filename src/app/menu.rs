//! The `x` menu: actions for the selected item, each named after its target, confirmed before
//! anything is sent. No action is ever on a bare key.

use super::Command;

/// One entry in the menu. Carries everything needed to name the target and build the command,
/// so neither the menu nor the confirmation has to look anything up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    RunNow { job_id: i64, name: String },
    CancelRun { job_id: i64, run_id: i64 },
}

impl MenuItem {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Run now: {name}"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}"),
        }
    }

    /// The question asked before anything is sent.
    #[must_use]
    pub fn confirmation(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Start a run of \"{name}\" now?"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}?"),
        }
    }

    #[must_use]
    pub const fn command(&self) -> Command {
        match *self {
            Self::RunNow { job_id, .. } => Command::RunNow { job_id },
            Self::CancelRun { job_id, run_id } => Command::CancelRun { job_id, run_id },
        }
    }
}

/// Where key presses go. A state machine rather than a pile of booleans.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    /// `/` pressed: letters edit the filter.
    Filter,
    /// `x` pressed: the menu is open over the selected item.
    Menu {
        items: Vec<MenuItem>,
        selected: usize,
    },
    /// An entry was chosen; `y` sends it, anything else backs out.
    Confirm(MenuItem),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_name_the_target() {
        let run = MenuItem::RunNow {
            job_id: 7,
            name: "okonomi_gold".to_owned(),
        };
        assert_eq!(run.label(), "Run now: okonomi_gold");
        assert_eq!(run.confirmation(), "Start a run of \"okonomi_gold\" now?");
        assert_eq!(run.command(), Command::RunNow { job_id: 7 });
        let cancel = MenuItem::CancelRun {
            job_id: 7,
            run_id: 42,
        };
        assert_eq!(cancel.label(), "Cancel run 42");
        assert_eq!(
            cancel.command(),
            Command::CancelRun {
                job_id: 7,
                run_id: 42
            }
        );
    }
}
