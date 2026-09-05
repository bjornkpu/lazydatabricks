//! Keys to actions. Defaults follow lazygit; config replaces bindings per action, which matters
//! on a Norwegian layout where `[` and `]` need `AltGr`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::Key;

/// Everything a key can do outside filter editing. Digit keys focus panels and are not
/// remappable; filter editing has its own fixed keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Quit,
    ScreenMode,
    ToggleLog,
    Filter,
    MineOnly,
    Refresh,
    RefreshAll,
    NextPanel,
    Open,
    Back,
    Down,
    Up,
    First,
    Last,
    NextTab,
    PrevTab,
    /// Opens the `x` menu; the only way to reach run-now and cancel.
    Menu,
    /// The `?` overlay.
    Help,
    /// Open the selected item in the browser.
    Browse,
    /// Copy the selected item's URL.
    Copy,
    /// Copy the focused panel's rows as text.
    CopyTable,
    /// Cycle the list order: activity, name, created.
    Sort,
    /// Cycle the status filter: all, failed, active.
    StatusFilter,
    PageDown,
    PageUp,
    /// Enable (after a confirmation) or disable actions for this session.
    ToggleActions,
    /// Open the profile menu: every profile in `~/.databrickscfg`.
    SwitchProfile,
    /// The `:` prompt: one ad-hoc `databricks` CLI line with the selection filled in.
    Prompt,
    /// Open the config file in `$EDITOR`.
    EditConfig,
    /// The filter menu: status, mine only, clear the text.
    FilterMenu,
    /// Start or end a range in the focused list, lazygit's `v`.
    RangeSelect,
    /// Next line matching the `[0]` search.
    NextMatch,
    PrevMatch,
}

/// The active bindings. Lookup is a scan over a few dozen entries per key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap(BTreeMap<Action, Vec<Key>>);

impl Default for Keymap {
    fn default() -> Self {
        Self(BTreeMap::from([
            (Action::Quit, vec![Key::Char('q'), Key::Ctrl('c')]),
            (Action::ScreenMode, vec![Key::Char('+')]),
            (Action::ToggleLog, vec![Key::Char('@')]),
            (Action::Filter, vec![Key::Char('/')]),
            (Action::MineOnly, vec![Key::Char('m')]),
            (Action::Refresh, vec![Key::Char('r')]),
            (Action::RefreshAll, vec![Key::Char('R')]),
            (Action::NextPanel, vec![Key::Tab]),
            (Action::Open, vec![Key::Enter]),
            (Action::Back, vec![Key::Esc]),
            (Action::Down, vec![Key::Char('j'), Key::Down]),
            (Action::Up, vec![Key::Char('k'), Key::Up]),
            (Action::First, vec![Key::Char('g')]),
            (Action::Last, vec![Key::Char('G')]),
            (
                Action::NextTab,
                vec![Key::Char('l'), Key::Char(']'), Key::Right],
            ),
            (
                Action::PrevTab,
                vec![Key::Char('h'), Key::Char('['), Key::Left],
            ),
            (Action::Menu, vec![Key::Char('x')]),
            (Action::Help, vec![Key::Char('?')]),
            (Action::Browse, vec![Key::Char('o')]),
            (Action::Copy, vec![Key::Char('y')]),
            (Action::CopyTable, vec![Key::Char('Y')]),
            (Action::Sort, vec![Key::Char('s')]),
            (Action::StatusFilter, vec![Key::Char('f')]),
            (Action::PageDown, vec![Key::Ctrl('d')]),
            (Action::PageUp, vec![Key::Ctrl('u')]),
            (Action::ToggleActions, vec![Key::Char('A')]),
            (Action::SwitchProfile, vec![Key::Char('p')]),
            (Action::Prompt, vec![Key::Char(':')]),
            (Action::EditConfig, vec![Key::Char('e')]),
            (Action::FilterMenu, vec![Key::Char('F')]),
            (Action::RangeSelect, vec![Key::Char('v')]),
            (Action::NextMatch, vec![Key::Char('n')]),
            (Action::PrevMatch, vec![Key::Char('N')]),
        ]))
    }
}

impl Keymap {
    /// Defaults with each listed action's bindings replaced. Empty lists are ignored so an
    /// action can never end up unreachable by accident. One key on two actions is an error:
    /// which one won would otherwise depend on enum order.
    pub fn with_overrides(overrides: &BTreeMap<Action, Vec<Key>>) -> Result<Self, String> {
        let mut keymap = Self::default();
        for (action, keys) in overrides {
            if !keys.is_empty() {
                keymap.0.insert(*action, keys.clone());
            }
        }
        let mut seen: BTreeMap<String, Action> = BTreeMap::new();
        for (action, keys) in &keymap.0 {
            for key in keys {
                if let Some(other) = seen.insert(key.to_string(), *action) {
                    return Err(format!(
                        "key {key} is bound to both {other:?} and {action:?}"
                    ));
                }
            }
        }
        Ok(keymap)
    }

    #[must_use]
    pub fn action(&self, key: Key) -> Option<Action> {
        self.0
            .iter()
            .find(|(_, keys)| keys.contains(&key))
            .map(|(action, _)| *action)
    }

    /// Every binding of `action`, `j/↓` style, for the help overlay.
    #[must_use]
    pub fn labels(&self, action: Action) -> String {
        self.0.get(&action).map_or_else(
            || "-".to_owned(),
            |keys| {
                keys.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("/")
            },
        )
    }

    /// The first binding of `action`, as shown in the hint bar.
    #[must_use]
    pub fn label(&self, action: Action) -> String {
        self.0
            .get(&action)
            .and_then(|keys| keys.first())
            .map_or_else(|| "-".to_owned(), ToString::to_string)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Char(c) => write!(f, "{c}"),
            Self::Tab => f.write_str("Tab"),
            Self::Up => f.write_str("↑"),
            Self::Down => f.write_str("↓"),
            Self::Left => f.write_str("←"),
            Self::Right => f.write_str("→"),
            Self::Enter => f.write_str("Enter"),
            Self::Esc => f.write_str("Esc"),
            Self::Backspace => f.write_str("Bksp"),
            Self::Ctrl(c) => write!(f, "^{}", c.to_ascii_uppercase()),
        }
    }
}

/// Key names as written in config: a single character, `ctrl+` and a character, or one of the
/// names below.
impl FromStr for Key {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let single = |s: &str| {
            let mut chars = s.chars();
            chars.next().filter(|_| chars.next().is_none())
        };
        if let Some(c) = single(s) {
            return Ok(Self::Char(c));
        }
        let lower = s.to_ascii_lowercase();
        if let Some(c) = lower.strip_prefix("ctrl+").and_then(single) {
            return Ok(Self::Ctrl(c));
        }
        Ok(match lower.as_str() {
            "tab" => Self::Tab,
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            "enter" => Self::Enter,
            "esc" => Self::Esc,
            "backspace" => Self::Backspace,
            _ => {
                return Err(format!(
                    "unknown key {s:?}; use one character, ctrl+<character>, or tab, up, down, left, right, enter, esc, backspace"
                ));
            }
        })
    }
}

impl TryFrom<String> for Key {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// The config spelling, the inverse of `FromStr`: `Display` is for the hint bar (`^D`, `↑`),
/// this is what the file takes back (`ctrl+d`, `up`).
impl From<Key> for String {
    fn from(key: Key) -> Self {
        match key {
            Key::Char(c) => c.to_string(),
            Key::Ctrl(c) => format!("ctrl+{c}"),
            Key::Tab => "tab".to_owned(),
            Key::Up => "up".to_owned(),
            Key::Down => "down".to_owned(),
            Key::Left => "left".to_owned(),
            Key::Right => "right".to_owned(),
            Key::Enter => "enter".to_owned(),
            Key::Esc => "esc".to_owned(),
            Key::Backspace => "backspace".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_spelling_round_trips() {
        for key in [
            Key::Char('ø'),
            Key::Ctrl('d'),
            Key::Up,
            Key::Backspace,
            Key::Tab,
        ] {
            let spelled: String = key.into();
            assert_eq!(spelled.parse::<Key>().unwrap(), key, "{spelled}");
        }
    }

    #[test]
    fn defaults_follow_lazygit() {
        let keys = Keymap::default();
        assert_eq!(keys.action(Key::Char('q')), Some(Action::Quit));
        assert_eq!(keys.action(Key::Ctrl('c')), Some(Action::Quit));
        assert_eq!(keys.action(Key::Ctrl('d')), Some(Action::PageDown));
        assert_eq!(keys.labels(Action::PageUp), "^U");
        assert_eq!(keys.action(Key::Char(']')), Some(Action::NextTab));
        assert_eq!(keys.action(Key::Char('7')), None);
        assert_eq!(keys.label(Action::Down), "j");
        assert_eq!(keys.labels(Action::Down), "j/↓");
        assert_eq!(keys.label(Action::Open), "Enter");
        assert_eq!(keys.action(Key::Char('?')), Some(Action::Help));
    }

    #[test]
    fn overrides_replace_whole_action() {
        let overrides = BTreeMap::from([
            (Action::NextTab, vec![Key::Char('ø')]),
            (Action::Quit, vec![]),
        ]);
        let keys = Keymap::with_overrides(&overrides).unwrap();
        assert_eq!(keys.action(Key::Char('ø')), Some(Action::NextTab));
        assert_eq!(keys.action(Key::Char(']')), None, "old binding gone");
        assert_eq!(
            keys.action(Key::Char('q')),
            Some(Action::Quit),
            "empty list ignored"
        );
        assert_eq!(keys.label(Action::NextTab), "ø");
    }

    #[test]
    fn one_key_two_actions_is_an_error() {
        let overrides = BTreeMap::from([(Action::Sort, vec![Key::Char('m')])]);
        let error = Keymap::with_overrides(&overrides).unwrap_err();
        assert!(
            error.contains("MineOnly") && error.contains("Sort"),
            "{error}"
        );
    }

    #[test]
    fn key_names_parse() {
        assert_eq!("l".parse::<Key>(), Ok(Key::Char('l')));
        assert_eq!("ø".parse::<Key>(), Ok(Key::Char('ø')));
        assert_eq!("Enter".parse::<Key>(), Ok(Key::Enter));
        assert_eq!("ctrl+c".parse::<Key>(), Ok(Key::Ctrl('c')));
        assert_eq!("Ctrl+D".parse::<Key>(), Ok(Key::Ctrl('d')));
        assert!("ctrl+xy".parse::<Key>().is_err());
        assert!("alt+x".parse::<Key>().is_err());
        assert!("".parse::<Key>().is_err());
    }
}
