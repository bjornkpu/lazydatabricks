//! The `x` menu: actions for the selected item, each named after its target, confirmed before
//! anything is sent. No action is ever on a bare key.

use std::collections::BTreeMap;

use super::custom::Output;
use super::{Command, Status};

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
    PauseSchedule {
        job_id: i64,
        name: String,
    },
    ResumeSchedule {
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
    StartCluster {
        cluster_id: String,
        name: String,
    },
    TerminateCluster {
        cluster_id: String,
        name: String,
    },
    StartWarehouse {
        warehouse_id: String,
        name: String,
    },
    StopWarehouse {
        warehouse_id: String,
        name: String,
    },
    /// A profile from `~/.databrickscfg`, from the `p` menu.
    SwitchProfile {
        name: String,
    },
    /// One thing `y` can put on the clipboard: `label` says what, `text` is it.
    CopyText {
        label: String,
        text: String,
    },
    /// One setting from the `F` menu. Applied in `update`; no command leaves the program.
    Filter(FilterChoice),
    /// `q` with `confirm_on_quit`: the question before leaving.
    Quit,
    /// One action over a `v` range: the label and question name the count, the commands are
    /// one per row.
    Bulk {
        label: String,
        confirmation: String,
        commands: Vec<Command>,
    },
    /// A custom command from config, its placeholders already filled in.
    Shell {
        name: String,
        command: String,
        output: Output,
        confirm: bool,
    },
}

impl MenuItem {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Run now: {name}"),
            Self::RunWith { name, .. } => format!("Run with parameters: {name}"),
            Self::PauseSchedule { name, .. } => format!("Pause schedule: {name}"),
            Self::ResumeSchedule { name, .. } => format!("Resume schedule: {name}"),
            Self::RepairRun { run_id, .. } => format!("Repair run {run_id}"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}"),
            Self::StartUpdate { name, .. } => format!("Start update: {name}"),
            Self::StopPipeline { name, .. } => format!("Stop: {name}"),
            Self::StartCluster { name, .. } => format!("Start cluster: {name}"),
            Self::TerminateCluster { name, .. } => format!("Terminate cluster: {name}"),
            Self::StartWarehouse { name, .. } => format!("Start warehouse: {name}"),
            Self::StopWarehouse { name, .. } => format!("Stop warehouse: {name}"),
            Self::Shell { name, .. } => name.clone(),
            Self::SwitchProfile { name } => format!("Switch to {name}"),
            Self::CopyText { label, .. } => format!("Copy {label}"),
            Self::Filter(choice) => choice.label(),
            Self::Bulk { label, .. } => label.clone(),
            Self::Quit => "Quit".to_owned(),
        }
    }

    /// Whether the `allow_actions` opt-in gates this entry. Custom commands and profile
    /// switches are not Databricks writes, so they are always live.
    #[must_use]
    pub const fn needs_actions(&self) -> bool {
        !matches!(
            self,
            Self::Shell { .. }
                | Self::SwitchProfile { .. }
                | Self::CopyText { .. }
                | Self::Filter(_)
                | Self::Quit
        )
    }

    /// The question asked before anything is sent. For `RunWith` it is the prompt's title.
    #[must_use]
    pub fn confirmation(&self) -> String {
        match self {
            Self::RunNow { name, .. } => format!("Start a run of \"{name}\" now?"),
            Self::RunWith { name, .. } => format!("Parameters for \"{name}\""),
            Self::PauseSchedule { name, .. } => format!("Pause the schedule of \"{name}\"?"),
            Self::ResumeSchedule { name, .. } => format!("Resume the schedule of \"{name}\"?"),
            Self::RepairRun { run_id, .. } => format!("Re-run the failed tasks of run {run_id}?"),
            Self::CancelRun { run_id, .. } => format!("Cancel run {run_id}?"),
            Self::StartUpdate { name, .. } => format!("Start an update of \"{name}\" now?"),
            Self::StopPipeline { name, .. } => format!("Stop the running update of \"{name}\"?"),
            Self::StartCluster { name, .. } => format!("Start cluster \"{name}\"?"),
            Self::TerminateCluster { name, .. } => format!("Terminate cluster \"{name}\"?"),
            Self::StartWarehouse { name, .. } => format!("Start warehouse \"{name}\"?"),
            Self::StopWarehouse { name, .. } => format!("Stop warehouse \"{name}\"?"),
            Self::Shell { command, .. } => format!("Run `{command}`?"),
            Self::SwitchProfile { name } => format!("Switch to {name}?"),
            Self::CopyText { label, .. } => format!("Copy {label}?"),
            Self::Filter(choice) => format!("{}?", choice.label()),
            Self::Bulk { confirmation, .. } => confirmation.clone(),
            Self::Quit => "Quit lazydatabricks?".to_owned(),
        }
    }

    /// The commands that carry this out: none for a filter choice, which is state, not IO.
    /// Pipeline ids are strings, so the command owns a copy. `RunWith` never gets here without
    /// parameters; see `with_params`.
    #[must_use]
    pub fn commands(&self) -> Vec<Command> {
        let command = match self {
            Self::Filter(_) => return Vec::new(),
            Self::Bulk { commands, .. } => return commands.clone(),
            Self::Quit => Command::Quit,
            Self::RunNow { job_id, .. } | Self::RunWith { job_id, .. } => Command::RunNow {
                job_id: *job_id,
                params: BTreeMap::new(),
            },
            Self::PauseSchedule { job_id, .. } => Command::SetSchedulePaused {
                job_id: *job_id,
                paused: true,
            },
            Self::ResumeSchedule { job_id, .. } => Command::SetSchedulePaused {
                job_id: *job_id,
                paused: false,
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
            Self::StartCluster { cluster_id, .. } => Command::StartCluster {
                cluster_id: cluster_id.clone(),
            },
            Self::TerminateCluster { cluster_id, .. } => Command::TerminateCluster {
                cluster_id: cluster_id.clone(),
            },
            Self::StartWarehouse { warehouse_id, .. } => Command::StartWarehouse {
                warehouse_id: warehouse_id.clone(),
            },
            Self::StopWarehouse { warehouse_id, .. } => Command::StopWarehouse {
                warehouse_id: warehouse_id.clone(),
            },
            Self::Shell {
                name,
                command,
                output,
                ..
            } => Command::Shell {
                name: name.clone(),
                command: command.clone(),
                output: *output,
            },
            Self::SwitchProfile { name } => Command::SwitchProfile(name.clone()),
            Self::CopyText { text, .. } => Command::Copy(text.clone()),
        };
        vec![command]
    }
}

/// What the `F` menu can set. lazygit's `ctrl+s` filter options, sized to our filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterChoice {
    Status(Status),
    MineOnly(bool),
    /// Drop the `/` text; carries it so the label can show what goes.
    ClearText(String),
}

impl FilterChoice {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Status(Status::All) => "Show all".to_owned(),
            Self::Status(status) => format!("Show {} only", status.as_str()),
            Self::MineOnly(true) => "Mine only: on".to_owned(),
            Self::MineOnly(false) => "Mine only: off".to_owned(),
            Self::ClearText(text) => format!("Clear text filter /{text}"),
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
    /// `/` pressed on a side list: letters edit the filter.
    Filter,
    /// `/` pressed in `[0]`: letters edit the search, which highlights rather than hides.
    Search,
    /// `x` pressed: the menu is open over the selected item.
    Menu {
        items: Vec<MenuItem>,
        selected: usize,
    },
    /// An entry was chosen; `y` sends it, anything else backs out.
    Confirm(MenuItem),
    /// `A` pressed in a read-only session: `y` enables actions until exit.
    ConfirmActions,
    /// *Run with parameters* chosen: one line of `key=value` pairs, `Enter` sends.
    Params {
        job_id: i64,
        name: String,
        text: String,
    },
    /// `?` pressed: the keybindings overlay is up, scrolled this many rows.
    Help { scroll: u16 },
    /// `:` pressed: arguments for one `databricks` CLI call are being typed.
    Prompt { text: String },
    /// A popup custom command finished: its output, scrolled this many rows.
    Output {
        title: String,
        lines: Vec<String>,
        scroll: usize,
    },
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
            run.commands(),
            vec![Command::RunNow {
                job_id: 7,
                params: BTreeMap::new()
            }]
        );
        let repair = MenuItem::RepairRun {
            job_id: 7,
            run_id: 42,
        };
        assert_eq!(repair.label(), "Repair run 42");
        assert_eq!(repair.confirmation(), "Re-run the failed tasks of run 42?");
        assert_eq!(
            repair.commands(),
            vec![Command::RepairRun {
                job_id: 7,
                run_id: 42
            }]
        );
        let cancel = MenuItem::CancelRun {
            job_id: 7,
            run_id: 42,
        };
        assert_eq!(cancel.label(), "Cancel run 42");
        assert_eq!(
            cancel.commands(),
            vec![Command::CancelRun {
                job_id: 7,
                run_id: 42
            }]
        );
        assert!(
            MenuItem::Filter(FilterChoice::MineOnly(true))
                .commands()
                .is_empty()
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
