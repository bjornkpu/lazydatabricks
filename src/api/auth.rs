//! Host and token discovery. Authentication is delegated to the Databricks CLI, which already
//! handles OAuth, PATs and keyring storage; we only read `~/.databrickscfg` for the host.

use std::io::ErrorKind;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::error::AppError;

/// The part of `databricks auth token` output we use.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Seconds until expiry. The CLI always sends it; 3600 is its documented value.
    expires_in: Option<u64>,
}

/// A bearer token and when it stops working. Never written to disk: the CLI's keyring is the
/// source of truth and minting again is cheap.
#[derive(Debug, Clone)]
pub struct Token {
    pub access_token: String,
    expires_at: Instant,
}

impl Token {
    /// True once less than `margin` of the token's life is left, so callers refresh before a
    /// request can fail on expiry.
    #[must_use]
    pub fn expires_within(&self, margin: Duration) -> bool {
        self.expires_at.saturating_duration_since(Instant::now()) <= margin
    }

    /// Marks the token unusable now, for when Databricks rejects it before its time.
    pub fn expire(&mut self) {
        self.expires_at = Instant::now();
    }
}

/// Mints a bearer token for `profile` by shelling out to the Databricks CLI.
pub fn mint(profile: &str) -> Result<Token, AppError> {
    let failed = |detail: String| AppError::CliFailed {
        profile: profile.to_owned(),
        detail,
    };
    let out = Command::new("databricks")
        .args(["auth", "token", "-p", profile])
        .output()
        .map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                AppError::CliMissing
            } else {
                failed(error.to_string())
            }
        })?;
    if !out.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    let parsed: TokenResponse =
        serde_json::from_slice(&out.stdout).map_err(|error| AppError::CliOutput {
            detail: error.to_string(),
        })?;
    let expires_at = Instant::now()
        .checked_add(Duration::from_secs(parsed.expires_in.unwrap_or(3600)))
        .ok_or_else(|| AppError::Internal("token expiry out of range".to_owned()))?;
    Ok(Token {
        access_token: parsed.access_token,
        expires_at,
    })
}

/// Reads the workspace host for `profile` from `~/.databrickscfg`.
pub fn host(profile: &str) -> Result<String, AppError> {
    let path = std::env::home_dir()
        .ok_or(AppError::HomeDir)?
        .join(".databrickscfg");
    let cfg = std::fs::read_to_string(&path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            // First run: the login command is the answer, not "file not found".
            AppError::NoCfg {
                profile: profile.to_owned(),
                path: path.display().to_string(),
            }
        } else {
            AppError::FileRead {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
        }
    })?;
    host_from_cfg(&cfg, profile).ok_or_else(|| AppError::NoHost {
        profile: profile.to_owned(),
        path: path.display().to_string(),
    })
}

/// Every `[section]` in `~/.databrickscfg`, in file order: what the profile menu offers.
pub fn profiles() -> Result<Vec<String>, AppError> {
    let path = std::env::home_dir()
        .ok_or(AppError::HomeDir)?
        .join(".databrickscfg");
    let cfg = std::fs::read_to_string(&path).map_err(|error| AppError::FileRead {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    Ok(profiles_from_cfg(&cfg))
}

fn profiles_from_cfg(cfg: &str) -> Vec<String> {
    cfg.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix('[')?.strip_suffix(']'))
        .map(|name| name.trim().to_owned())
        .collect()
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
    fn every_section_is_a_profile() {
        assert_eq!(profiles_from_cfg(CFG), vec!["DEFAULT", "dev"]);
        assert!(profiles_from_cfg("").is_empty());
    }

    #[test]
    fn token_expiry_margin() {
        let token = |secs| Token {
            access_token: String::new(),
            expires_at: Instant::now()
                .checked_add(Duration::from_secs(secs))
                .unwrap(),
        };
        let margin = Duration::from_secs(300);
        assert!(!token(3600).expires_within(margin));
        assert!(token(60).expires_within(margin));
        assert!(
            token(0).expires_within(margin),
            "already expired counts too"
        );
    }

    #[test]
    fn expired_token_is_due_for_refresh() {
        let mut token = Token {
            access_token: "abc".to_owned(),
            expires_at: Instant::now()
                .checked_add(Duration::from_secs(3600))
                .unwrap(),
        };
        token.expire();
        assert!(token.expires_within(Duration::ZERO));
    }

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
