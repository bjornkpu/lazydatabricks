//! One error type for everything that can go wrong, worded for the person at the keyboard.
//! `Display` says what happened and what to do next; `Debug` keeps the structure for logs.
//! Sources are kept as text so the type stays `Clone + Eq` and can travel in messages and sit in
//! app state.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("databricks CLI not found. Install it: winget install Databricks.DatabricksCLI")]
    CliMissing,
    #[error(
        "`databricks auth token -p {profile}` failed: {detail}. Run `databricks auth login -p {profile}`"
    )]
    CliFailed { profile: String, detail: String },
    #[error("unexpected output from `databricks auth token`: {detail}")]
    CliOutput { detail: String },
    #[error("could not determine the home directory")]
    HomeDir,
    #[error("could not determine the config directory")]
    ConfigDir,
    #[error("could not read {path}: {detail}")]
    FileRead { path: String, detail: String },
    #[error("invalid config {path}: {detail}")]
    ConfigParse { path: String, detail: String },
    #[error(
        "no profile [{profile}] in {path}. Run `databricks auth login --host https://<workspace-url> -p {profile}`"
    )]
    NoHost { profile: String, path: String },
    #[error(
        "no {path} yet. Run `databricks auth login --host https://<workspace-url> -p {profile}` once; it creates the file"
    )]
    NoCfg { profile: String, path: String },
    #[error(
        "HTTP {status} for {path}: token rejected. Run `databricks auth login -p {profile}`, then press r"
    )]
    Unauthorized {
        status: u16,
        path: String,
        profile: String,
    },
    #[error("HTTP {status} for {path}: {message}")]
    Http {
        status: u16,
        path: String,
        message: String,
    },
    #[error("{path} timed out. Check network or VPN, then press r")]
    Timeout { path: String },
    #[error("network error for {path}: {detail}")]
    Network { path: String, detail: String },
    #[error("malformed JSON from {path}: {detail}. The API shape may have changed")]
    Json { path: String, detail: String },
    #[error("could not {what}: {detail}")]
    Shell { what: String, detail: String },
    #[error("internal error: {0}")]
    Internal(String),
}

/// What Databricks puts in an error body.
#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    message: String,
}

impl AppError {
    /// A few words for a border or a column: the kind of failure, not the story.
    #[must_use]
    pub fn short(&self) -> String {
        match self {
            Self::Unauthorized { .. } => "token rejected".to_owned(),
            Self::Http { status, .. } => format!("HTTP {status}"),
            Self::Timeout { .. } => "timed out".to_owned(),
            Self::Network { .. } => "network error".to_owned(),
            Self::Json { .. } => "bad JSON".to_owned(),
            Self::CliMissing
            | Self::CliFailed { .. }
            | Self::CliOutput { .. }
            | Self::HomeDir
            | Self::ConfigDir
            | Self::FileRead { .. }
            | Self::ConfigParse { .. }
            | Self::NoHost { .. }
            | Self::NoCfg { .. }
            | Self::Shell { .. }
            | Self::Internal(_) => self.to_string(),
        }
    }

    /// Classifies a transport-level reqwest failure for `path`.
    #[must_use]
    pub fn from_reqwest(error: &reqwest::Error, path: &str) -> Self {
        let path = path.to_owned();
        if error.is_timeout() {
            Self::Timeout { path }
        } else if error.is_decode() {
            Self::Json {
                path,
                detail: error.to_string(),
            }
        } else {
            Self::Network {
                path,
                detail: error.to_string(),
            }
        }
    }

    /// Classifies a non-success status. 401 is always the token; 403 is the token only when
    /// Databricks says so, otherwise it is a permission on the resource. Other statuses carry the
    /// body's `error_code: message`, or the raw body when it is not the usual JSON.
    #[must_use]
    pub fn from_status(status: u16, path: &str, body: &str, profile: &str) -> Self {
        if status == 401 || (status == 403 && body.to_lowercase().contains("token")) {
            return Self::Unauthorized {
                status,
                path: path.to_owned(),
                profile: profile.to_owned(),
            };
        }
        let message = match serde_json::from_str::<ErrorBody>(body) {
            Ok(ErrorBody {
                error_code,
                message,
            }) if !message.is_empty() => {
                if error_code.is_empty() {
                    message
                } else {
                    format!("{error_code}: {message}")
                }
            }
            _ => body.chars().take(200).collect(),
        };
        Self::Http {
            status,
            path: path.to_owned(),
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_is_401_or_a_403_about_the_token() {
        let error = AppError::from_status(401, "/api/x", "", "dev");
        assert!(matches!(error, AppError::Unauthorized { status: 401, .. }));
        assert!(error.to_string().contains("databricks auth login -p dev"));

        let error = AppError::from_status(
            403,
            "/api/x",
            r#"{"error_code":"PERMISSION_DENIED","message":"Invalid access token."}"#,
            "dev",
        );
        assert!(matches!(error, AppError::Unauthorized { status: 403, .. }));
    }

    #[test]
    fn short_names_the_kind() {
        assert_eq!(
            AppError::Timeout {
                path: "/x".to_owned()
            }
            .short(),
            "timed out"
        );
        assert_eq!(
            AppError::from_status(429, "/x", "", "dev").short(),
            "HTTP 429"
        );
        assert_eq!(
            AppError::Internal("boom".to_owned()).short(),
            "internal error: boom"
        );
    }

    #[test]
    fn forbidden_resource_is_plain_http() {
        let error = AppError::from_status(
            403,
            "/api/2.2/jobs/runs/list?job_id=1",
            r#"{"error_code":"PERMISSION_DENIED","message":"User does not have permission."}"#,
            "dev",
        );
        assert_eq!(
            error.to_string(),
            "HTTP 403 for /api/2.2/jobs/runs/list?job_id=1: PERMISSION_DENIED: User does not have permission."
        );
    }

    #[test]
    fn non_json_bodies_are_quoted_and_capped() {
        let body = "x".repeat(500);
        let error = AppError::from_status(502, "/api/x", &body, "dev");
        let AppError::Http { message, .. } = error else {
            panic!("expected Http");
        };
        assert_eq!(message.len(), 200);
    }

    #[test]
    fn cli_failures_point_at_login() {
        let error = AppError::CliFailed {
            profile: "dev".to_owned(),
            detail: "token expired".to_owned(),
        };
        assert!(error.to_string().contains("databricks auth login -p dev"));
    }
}
