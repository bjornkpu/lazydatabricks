//! `config.toml` in the platform config directory. Every field is optional with a sane default,
//! so a missing file changes nothing about startup.

use std::collections::BTreeMap;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::Deserialize;

use crate::app::{Action, CustomCommand, Key, Keymap, Status};
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
// Mirrors the TOML file: each bool is a user switch, and `App` turns them into enums.
#[allow(clippy::struct_excessive_bools)]
pub struct Config {
    /// Databricks CLI profile. `DATABRICKS_CONFIG_PROFILE` wins over this; `DEFAULT` otherwise.
    pub profile: Option<String>,
    /// Several profiles to open at once, one workspace each; `p` cycles them. Wins over
    /// `profile` when not empty.
    pub profiles: Vec<String>,
    /// Upper bound on jobs fetched across pages.
    pub max_jobs: usize,
    /// Start with the "mine only" filter on.
    pub mine_only: bool,
    /// Start with this name filter applied.
    pub filter: Option<String>,
    /// Start showing `all` (default), `failed` or `active` rows only. `f` cycles it.
    pub status: Status,
    /// Show the `[4] Compute` panel (clusters and SQL warehouses). `false` never fetches them.
    pub compute: bool,
    /// The side panel in context takes twice the height of the others (lazygit's
    /// `expandFocusedSidePanel`). `false` shares the column evenly.
    pub expand_focused: bool,
    /// Enable run-now and cancel in the `x` menu. Read-only without it (or `--allow-actions`).
    pub allow_actions: bool,
    /// Value of the `dev` tag that marks a job as mine. Derived from the email when unset.
    pub dev_tag: Option<String>,
    /// Other names that count as me for the mine filter, as creator or run-as: service
    /// principals that deploy my bundles.
    pub me_aliases: Vec<String>,
    /// Chrome colours: `dark` (default) or `light`. Status glyphs keep their colours either way.
    pub theme: Theme,
    /// Order of the job and pipeline lists: `activity` (default), `name` or `created`. `s`
    /// cycles it while running.
    pub sort: Sort,
    /// `strftime` pattern for absolute times in tables. Default `%d.%m %H:%M`.
    pub date_format: String,
    /// Background refresh interval for the jobs list.
    pub jobs_ttl_secs: u64,
    /// How long cached runs are shown before being refetched.
    pub runs_ttl_secs: u64,
    /// Key overrides by action; each list replaces that action's default bindings entirely.
    pub keys: BTreeMap<Action, Vec<Key>>,
    /// Shell lines offered in the `x` menu, lazygit style. See `app::custom`.
    pub commands: Vec<CustomCommand>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: None,
            profiles: Vec::new(),
            max_jobs: 200,
            mine_only: false,
            filter: None,
            status: Status::All,
            compute: true,
            expand_focused: true,
            allow_actions: false,
            dev_tag: None,
            me_aliases: Vec::new(),
            theme: Theme::Dark,
            sort: Sort::Activity,
            date_format: "%d.%m %H:%M".to_owned(),
            jobs_ttl_secs: 300,
            runs_ttl_secs: 120,
            keys: BTreeMap::new(),
            commands: Vec::new(),
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
    /// No colours, no dim text: focus is a double border, the cursor is reverse video. Also
    /// what `NO_COLOR` selects.
    Mono,
}

/// The config that was loaded and where it came from, for the Profile tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
    /// False when the file did not exist and defaults are in use.
    pub found: bool,
}

/// Reads the config file if present. A missing default file is defaults; a missing `explicit`
/// one is an error, as is an unreadable or invalid file, since silently ignoring a typo would
/// be worse.
pub fn load(explicit: Option<PathBuf>) -> Result<Loaded, AppError> {
    let path = match explicit {
        Some(path) => path,
        None => ProjectDirs::from("", "", "lazydatabricks")
            .ok_or(AppError::ConfigDir)?
            .config_dir()
            .join("config.toml"),
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !explicit_given(&path) => {
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

/// `load` treats a path it did not derive itself as required. Derived paths end in the platform
/// config dir; a user path is whatever they typed.
fn explicit_given(path: &std::path::Path) -> bool {
    ProjectDirs::from("", "", "lazydatabricks")
        .is_none_or(|dirs| !path.starts_with(dirs.config_dir()))
}

/// Parses the file and checks the values `toml` cannot: a `date_format` that would fail on
/// every render is rejected here, once, instead of there, sixty times a second.
fn parse(text: &str) -> Result<Config, String> {
    let config: Config = toml::from_str(text).map_err(|error| error.to_string())?;
    let keymap = Keymap::with_overrides(&config.keys)?;
    let mut taken = std::collections::BTreeSet::new();
    for custom in &config.commands {
        if custom.command.trim().is_empty() {
            return Err(format!("command {:?} has no command line", custom.name));
        }
        let Some(key) = custom.key else {
            continue;
        };
        if let Some(action) = keymap.action(key) {
            return Err(format!(
                "command {:?}: key {key} is already {action:?}",
                custom.name
            ));
        }
        if !taken.insert(key) {
            return Err(format!("command {:?}: key {key} used twice", custom.name));
        }
    }
    let sample = jiff::Zoned::new(jiff::Timestamp::UNIX_EPOCH, jiff::tz::TimeZone::UTC);
    jiff::fmt::strtime::format(&config.date_format, &sample)
        .map_err(|error| format!("date_format {:?}: {error}", config.date_format))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_commands_parse_and_keep_their_keys_free() {
        let config = parse(
            r#"
            [[commands]]
            name = "Job JSON"
            key = "J"
            context = "jobs"
            command = "databricks jobs get {{job_id}}"

            [[commands]]
            name = "Deploy"
            command = "databricks bundle deploy"
            output = "terminal"
            confirm = true
            "#,
        )
        .unwrap();
        assert_eq!(config.commands.len(), 2);
        assert_eq!(config.commands[0].key, Some(Key::Char('J')));
        assert_eq!(config.commands[0].context, crate::app::Context::Jobs);
        assert_eq!(config.commands[1].context, crate::app::Context::Any);
        assert_eq!(
            config.commands[1].output,
            crate::app::CommandOutput::Terminal
        );
        assert!(config.commands[1].confirm);
        let taken =
            parse("[[commands]]\nname = \"x\"\nkey = \"m\"\ncommand = \"true\"").unwrap_err();
        assert!(taken.contains("already MineOnly"), "{taken}");
        let twice = parse(
            "[[commands]]\nname = \"a\"\nkey = \"J\"\ncommand = \"true\"\n[[commands]]\nname = \"b\"\nkey = \"J\"\ncommand = \"true\"",
        )
        .unwrap_err();
        assert!(twice.contains("used twice"), "{twice}");
        assert!(parse("[[commands]]\nname = \"a\"\ncommand = \" \"").is_err());
    }

    #[test]
    fn expand_focused_is_a_switch() {
        assert!(parse("").unwrap().expand_focused);
        assert!(!parse("expand_focused = false").unwrap().expand_focused);
    }

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
        assert_eq!(
            parse("profiles = [\"dev\", \"prod\"]").unwrap().profiles,
            vec!["dev".to_owned(), "prod".to_owned()]
        );
        assert!(config.mine_only);
        assert_eq!(config.dev_tag.as_deref(), Some("bk"));
        assert_eq!(
            parse("me_aliases = [\"sp-1\"]").unwrap().me_aliases,
            vec!["sp-1".to_owned()]
        );
        assert_eq!(config.max_jobs, 200);
        assert_eq!(
            config.keys[&Action::NextTab],
            vec![Key::Char('ø'), Key::Right]
        );
        assert_eq!(config.keys[&Action::Quit], vec![Key::Char('q')]);
        assert_eq!(parse("theme = \"light\"").unwrap().theme, Theme::Light);
        assert_eq!(parse("sort = \"name\"").unwrap().sort, Sort::Name);
        assert_eq!(parse("status = \"failed\"").unwrap().status, Status::Failed);
        assert!(parse("").unwrap().compute);
        assert!(!parse("compute = false").unwrap().compute);
        assert_eq!(Sort::Created.next(), Sort::Activity);
        assert!(parse("theme = \"neon\"").is_err());
    }

    #[test]
    fn date_format_is_checked_at_load() {
        assert_eq!(
            parse("date_format = \"%Y-%m-%d %H:%M\"")
                .unwrap()
                .date_format,
            "%Y-%m-%d %H:%M"
        );
        let error = parse("date_format = \"%!\"").unwrap_err();
        assert!(error.contains("date_format"), "{error}");
    }

    #[test]
    fn typos_are_errors() {
        let error = parse("max_job = 5").unwrap_err();
        assert!(error.contains("max_job"), "{error}");
        let error = parse("[keys]\nnext_tab = [\"alt+x\"]").unwrap_err();
        assert!(error.contains("alt+x"), "{error}");
        assert!(parse("[keys]\nfly = [\"f\"]").is_err());
        let error = parse("[keys]\nsort = [\"m\"]").unwrap_err();
        assert!(error.contains("bound to both"), "{error}");
    }
}
