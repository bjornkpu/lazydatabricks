//! Host and token discovery. Authentication is delegated to the Databricks CLI, which already
//! handles OAuth, PATs and keyring storage; we only read `~/.databrickscfg` for the host.

use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

/// The part of `databricks auth token` output we use.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Mints a bearer token for `profile` by shelling out to the Databricks CLI.
pub fn token(profile: &str) -> Result<String> {
    let out = Command::new("databricks")
        .args(["auth", "token", "-p", profile])
        .output()
        .context(
            "could not run `databricks`; install the CLI: winget install Databricks.DatabricksCLI",
        )?;
    ensure!(
        out.status.success(),
        "`databricks auth token -p {profile}` failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let parsed: TokenResponse = serde_json::from_slice(&out.stdout)
        .context("unexpected output from `databricks auth token`")?;
    Ok(parsed.access_token)
}

/// Reads the workspace host for `profile` from `~/.databrickscfg`.
pub fn host(profile: &str) -> Result<String> {
    let path = std::env::home_dir()
        .context("could not determine home directory")?
        .join(".databrickscfg");
    let cfg = std::fs::read_to_string(&path)
        .with_context(|| format!("could not read {}", path.display()))?;
    host_from_cfg(&cfg, profile)
        .with_context(|| format!("no host for profile [{profile}] in {}", path.display()))
}

/// Minimal INI scan: the `host` key inside the `[profile]` section. Trailing slash dropped so
/// paths can be appended blindly.
fn host_from_cfg(cfg: &str, profile: &str) -> Option<String> {
    let header = format!("[{profile}]");
    cfg.lines()
        .map(str::trim)
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "host").then(|| value.trim().trim_end_matches('/').to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: &str = "
[DEFAULT]
host         = https://adb-1.azuredatabricks.net/
auth_type    = databricks-cli

[dev]
host = https://adb-2.azuredatabricks.net
hostname_unrelated = nope
auth_type = databricks-cli
";

    #[test]
    fn finds_host_and_strips_trailing_slash() {
        assert_eq!(
            host_from_cfg(CFG, "DEFAULT").as_deref(),
            Some("https://adb-1.azuredatabricks.net")
        );
    }

    #[test]
    fn stops_at_next_section() {
        assert_eq!(
            host_from_cfg(CFG, "dev").as_deref(),
            Some("https://adb-2.azuredatabricks.net")
        );
    }

    #[test]
    fn unknown_profile_is_none() {
        assert_eq!(host_from_cfg(CFG, "prod"), None);
    }
}
