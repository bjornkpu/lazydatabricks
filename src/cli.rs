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
}

#[cfg(test)]
mod tests {
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
        let bare = Cli::try_parse_from(["lazydatabricks"]).unwrap();
        assert_eq!(bare.profile, None);
        assert!(!bare.allow_actions);
    }
}
