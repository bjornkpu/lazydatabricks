//! The `x` menu: actions for the selected item, each named after its target, confirmed before
//! anything is sent. No action is ever on a bare key.

use std::collections::BTreeMap;

use super::Command;

/// One entry in the menu. Carries everything needed to name the target and build the command,
/// so neither the menu nor the confirmation has to look anything up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    RunNow {
        job_id: i64,
        name: String,
    },
    /// Opens the parameter prompt instead of a confirmation.
    RunWith {
        job_id: i64,
        name: String,
    },
    RepairRun {
        job_id: i64,
        run_id: i64,
    },
    CancelRun {
        job_id: i64,
        run_id: i64,
    },
    StartUpdate {
        pipeline_id: String,
        name: String,
    },
    StopPipeline {
        pipeline_id: String,
        name: String,
    },
}

impl MenuItem {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Run now: {name}"),
            Self::RunWith { name, .. } => format!("Run with parameters: {name}"),
            Self::RepairRun { run_id, .. } => format!("Repair run {run_id}"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}"),
            Self::StartUpdate { name, .. } => format!("Start update: {name}"),
            Self::StopPipeline { name, .. } => format!("Stop: {name}"),
        }
    }

    /// The question asked before anything is sent. For `RunWith` it is the prompt's title.
    #[must_use]
    pub fn confirmation(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Start a run of \"{name}\" now?"),
            Self::RunWith { name, .. } => format!("Parameters for \"{name}\""),
            Self::RepairRun { run_id, .. } => format!("Re-run the failed tasks of run {run_id}?"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}?"),
            Self::StartUpdate { name, .. } => format!("Start an update of \"{name}\" now?"),
            Self::StopPipeline { name, .. } => format!("Stop the running update of \"{name}\"?"),
        }
    }

    /// The command that carries this out. Pipeline ids are strings, so the command owns a copy.
    /// `RunWith` never gets here without parameters; see `with_params`.
    #[must_use]
    pub fn command(&self) -> Command {
        match self {
            Self::RunNow { job_id, .. } | Self::RunWith { job_id, .. } => Command::RunNow {
                job_id: *job_id,
                params: BTreeMap::new(),
            },
            Self::RepairRun { job_id, run_id } => Command::RepairRun {
                job_id: *job_id,
                run_id: *run_id,
            },
            Self::CancelRun { job_id, run_id } => Command::CancelRun {
                job_id: *job_id,
                run_id: *run_id,
            },
            Self::StartUpdate { pipeline_id, .. } => Command::StartUpdate {
                pipeline_id: pipeline_id.clone(),
            },
            Self::StopPipeline { pipeline_id, .. } => Command::StopPipeline {
                pipeline_id: pipeline_id.clone(),
            },
        }
    }
}

/// `key=value` pairs separated by whitespace, as typed in the parameter prompt.
pub fn parse_params(text: &str) -> Result<BTreeMap<String, String>, String> {
    text.split_whitespace()
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) if !key.is_empty() => Ok((key.to_owned(), value.to_owned())),
            _ => Err(format!("expected key=value, got {pair:?}")),
        })
        .collect()
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
    /// *Run with parameters* chosen: one line of `key=value` pairs, `Enter` sends.
    Params {
        job_id: i64,
        name: String,
        text: String,
    },
    /// `?` pressed: the keybindings overlay is up.
    Help,
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
        assert_eq!(
            run.command(),
            Command::RunNow {
                job_id: 7,
                params: BTreeMap::new()
            }
        );
        let repair = MenuItem::RepairRun {
            job_id: 7,
            run_id: 42,
        };
        assert_eq!(repair.label(), "Repair run 42");
        assert_eq!(repair.confirmation(), "Re-run the failed tasks of run 42?");
        assert_eq!(
            repair.command(),
            Command::RepairRun {
                job_id: 7,
                run_id: 42
            }
        );
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

    #[test]
    fn params_are_key_value_pairs() {
        let params = parse_params("  date=2026-09-01 mode=full empty= ").unwrap();
        assert_eq!(params["date"], "2026-09-01");
        assert_eq!(params["mode"], "full");
        assert_eq!(params["empty"], "");
        assert!(parse_params("").unwrap().is_empty());
        assert!(parse_params("date").unwrap_err().contains("date"));
        assert!(parse_params("=x").is_err(), "a key is required");
    }
}
