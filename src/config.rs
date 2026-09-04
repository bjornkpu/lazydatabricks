//! `config.toml` in the platform config directory. Every field is optional with a sane default,
//! so a missing file changes nothing about startup.

use std::collections::BTreeMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::Deserialize;

use crate::app::{Action, Key};
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Databricks CLI profile. `DATABRICKS_CONFIG_PROFILE` wins over this; `DEFAULT` otherwise.
    pub profile: Option<String>,
    /// Upper bound on jobs fetched across pages.
    pub max_jobs: usize,
    /// Start with the "mine only" filter on.
    pub mine_only: bool,
    /// Start with this name filter applied.
    pub filter: Option<String>,
    /// Enable run-now and cancel in the `x` menu. Read-only without it (or `--allow-actions`).
    pub allow_actions: bool,
    /// Value of the `dev` tag that marks a job as mine. Derived from the email when unset.
    pub dev_tag: Option<String>,
    /// Chrome colours: `dark` (default) or `light`. Status glyphs keep their colours either way.
    pub theme: Theme,
    /// Order of the job and pipeline lists: `activity` (default), `name` or `created`. `s`
    /// cycles it while running.
    pub sort: Sort,
    /// Background refresh interval for the jobs list.
    pub jobs_ttl_secs: u64,
    /// How long cached runs are shown before being refetched.
    pub runs_ttl_secs: u64,
    /// Key overrides by action; each list replaces that action's default bindings entirely.
    pub keys: BTreeMap<Action, Vec<Key>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: None,
            max_jobs: 200,
            mine_only: false,
            filter: None,
            allow_actions: false,
            dev_tag: None,
            theme: Theme::Dark,
            sort: Sort::Activity,
            jobs_ttl_secs: 300,
            runs_ttl_secs: 120,
            keys: BTreeMap::new(),
        }
    }
}

/// List order. Ties always break on name, so the order is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Newest run or update first; never-run items last.
    #[default]
    Activity,
    Name,
    /// Newest created first. Pipelines carry no creation time in the list, so they fall back to
    /// name.
    Created,
}

impl Sort {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Activity => Self::Name,
            Self::Name => Self::Created,
            Self::Created => Self::Activity,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Name => "name",
            Self::Created => "created",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// The config that was loaded and where it came from, for the Profile tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
    /// False when the file did not exist and defaults are in use.
    pub found: bool,
}

/// Reads the config file if present. A missing file is defaults; an unreadable or invalid one
/// is an error, since silently ignoring a typo would be worse.
pub fn load() -> Result<Loaded, AppError> {
    let path = ProjectDirs::from("", "", "lazydatabricks")
        .ok_or(AppError::ConfigDir)?
        .config_dir()
        .join("config.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Loaded {
                config: Config::default(),
                path,
                found: false,
            });
        }
        Err(error) => {
            return Err(AppError::FileRead {
                path: path.display().to_string(),
                detail: error.to_string(),
            });
        }
    };
    let config = parse(&text).map_err(|detail| AppError::ConfigParse {
        path: path.display().to_string(),
        detail,
    })?;
    Ok(Loaded {
        config,
        path,
        found: true,
    })
}

fn parse(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_is_defaults() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let config = parse(
            r#"
            profile = "dev"
            mine_only = true
            dev_tag = "bk"

            [keys]
            next_tab = ["ø", "right"]
            quit = ["q"]
            "#,
        )
        .unwrap();
        assert_eq!(config.profile.as_deref(), Some("dev"));
        assert!(config.mine_only);
        assert_eq!(config.dev_tag.as_deref(), Some("bk"));
        assert_eq!(config.max_jobs, 200);
        assert_eq!(
            config.keys[&Action::NextTab],
            vec![Key::Char('ø'), Key::Right]
        );
        assert_eq!(config.keys[&Action::Quit], vec![Key::Char('q')]);
        assert_eq!(parse("theme = \"light\"").unwrap().theme, Theme::Light);
        assert_eq!(parse("sort = \"name\"").unwrap().sort, Sort::Name);
        assert_eq!(Sort::Created.next(), Sort::Activity);
        assert!(parse("theme = \"neon\"").is_err());
    }

    #[test]
    fn typos_are_errors() {
        let error = parse("max_job = 5").unwrap_err();
        assert!(error.contains("max_job"), "{error}");
        let error = parse("[keys]\nnext_tab = [\"ctrl+x\"]").unwrap_err();
        assert!(error.contains("ctrl+x"), "{error}");
        assert!(parse("[keys]\nfly = [\"f\"]").is_err());
    }
}
