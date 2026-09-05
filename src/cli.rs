//! Command line flags. A flag beats the environment, which beats the config file.

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about = "A lazygit-style TUI for Databricks jobs")]
pub struct Cli {
    /// Databricks CLI profile from ~/.databrickscfg.
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
    }
}
