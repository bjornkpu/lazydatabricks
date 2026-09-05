//! Custom commands from config: lazygit's `customCommands`. One shell line with placeholders,
//! offered in the `x` menu for its context and, optionally, on a key of its own. The Databricks
//! CLI covers what this TUI never will; this is the seam that lets it in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Key;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCommand {
    /// Shown in the menu and in the notice.
    pub name: String,
    /// Fires it without the menu. Must not collide with a binding; config load checks.
    #[serde(default)]
    pub key: Option<Key>,
    #[serde(default)]
    pub context: Context,
    /// Shell line with `{{job_id}}`-style placeholders; see `App::template_vars`.
    pub command: String,
    #[serde(default)]
    pub output: Output,
    /// Ask before running.
    #[serde(default)]
    pub confirm: bool,
}

/// Where a command is offered. `runs` needs a run under the cursor in `[0]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Context {
    #[default]
    Any,
    Jobs,
    Runs,
    Pipelines,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    /// Captured and shown in an overlay when it finishes.
    #[default]
    Popup,
    /// The TUI steps aside; the command has the terminal until it exits and Enter is pressed.
    Terminal,
}

/// Replaces every `{{name}}` from `vars`. An unknown name is an error naming it, so a typo in
/// config shows up as words the first time the command is offered, not as a broken shell line.
pub fn expand(template: &str, vars: &BTreeMap<&str, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (before, after) = rest.split_at(start);
        out.push_str(before);
        let Some(end) = after.find("}}") else {
            return Err(format!("unclosed {{{{ in {template:?}"));
        };
        let name = after.get(2..end).unwrap_or_default().trim();
        let value = vars
            .get(name)
            .ok_or_else(|| format!("no {{{{{name}}}}} here"))?;
        out.push_str(value);
        rest = after.get(end.saturating_add(2)..).unwrap_or_default();
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> BTreeMap<&'static str, String> {
        BTreeMap::from([("job_id", "7".to_owned()), ("profile", "dev".to_owned())])
    }

    #[test]
    fn expands_every_placeholder() {
        assert_eq!(
            expand("databricks jobs get {{job_id}} -p {{ profile }}", &vars()).unwrap(),
            "databricks jobs get 7 -p dev"
        );
        assert_eq!(expand("plain", &vars()).unwrap(), "plain");
    }

    #[test]
    fn names_the_unknown_placeholder() {
        assert_eq!(
            expand("{{run_id}}", &vars()).unwrap_err(),
            "no {{run_id}} here"
        );
        assert!(
            expand("{{job_id", &vars())
                .unwrap_err()
                .starts_with("unclosed")
        );
    }
}
