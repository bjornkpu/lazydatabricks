//! Command line flags. A flag beats the environment, which beats the config file.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about = "A lazygit-style TUI for Databricks jobs")]
pub struct Cli {
    /// Print JSON instead of opening the TUI.
    #[command(subcommand)]
    pub command: Option<Sub>,
    /// Databricks CLI profile from ~/.databrickscfg. Comma-separated opens several; p cycles.
    #[arg(short, long)]
    pub profile: Option<String>,
    /// Start with this name filter applied.
    #[arg(long)]
    pub filter: Option<String>,
    /// Enable run-now and cancel in the x menu. Read-only without it.
    #[arg(long)]
    pub allow_actions: bool,
    /// Config file to use instead of the platform default. Also `LAZYDATABRICKS_CONFIG`.
    #[arg(long)]
    pub config: Option<std::path::PathBuf>,
}

/// Non-interactive listings for scripts: the same models the TUI holds, as a JSON array.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Sub {
    /// Jobs, up to the configured maximum.
    Jobs,
    /// The most recent runs of one job.
    Runs { job_id: i64 },
    /// Pipelines, up to the configured maximum.
    Pipelines,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn flags_parse() {
        let cli = Cli::try_parse_from([
            "lazydatabricks",
            "-p",
            "dev",
            "--filter",
            "gold",
            "--allow-actions",
        ])
        .unwrap();
        assert_eq!(cli.profile.as_deref(), Some("dev"));
        assert_eq!(cli.filter.as_deref(), Some("gold"));
        assert!(cli.allow_actions);
        assert_eq!(cli.config, None);
        let with_config = Cli::try_parse_from(["lazydatabricks", "--config", "team.toml"]).unwrap();
        assert_eq!(with_config.config.as_deref(), Some(Path::new("team.toml")));
        let bare = Cli::try_parse_from(["lazydatabricks"]).unwrap();
        assert_eq!(bare.profile, None);
        assert!(!bare.allow_actions);
        assert_eq!(bare.command, None);
        let runs = Cli::try_parse_from(["lazydatabricks", "-p", "dev", "runs", "42"]).unwrap();
        assert_eq!(runs.command, Some(Sub::Runs { job_id: 42 }));
        assert_eq!(runs.profile.as_deref(), Some("dev"));
        assert!(Cli::try_parse_from(["lazydatabricks", "runs"]).is_err());
    }
}
