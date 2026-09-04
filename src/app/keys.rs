//! Keys to actions. Defaults follow lazygit; config replaces bindings per action, which matters
//! on a Norwegian layout where `[` and `]` need `AltGr`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use super::Key;

/// Everything a key can do outside filter editing. Digit keys focus panels and are not
/// remappable; filter editing has its own fixed keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
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
}

/// The active bindings. Lookup is a scan over a few dozen entries per key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap(BTreeMap<Action, Vec<Key>>);

impl Default for Keymap {
    fn default() -> Self {
        Self(BTreeMap::from([
            (Action::Quit, vec![Key::Char('q'), Key::CtrlC]),
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
        ]))
    }
}

impl Keymap {
    /// Defaults with each listed action's bindings replaced. Empty lists are ignored so an
    /// action can never end up unreachable by accident.
    #[must_use]
    pub fn with_overrides(overrides: &BTreeMap<Action, Vec<Key>>) -> Self {
        let mut keymap = Self::default();
        for (action, keys) in overrides {
            if !keys.is_empty() {
                keymap.0.insert(*action, keys.clone());
            }
        }
        keymap
    }

    #[must_use]
    pub fn action(&self, key: Key) -> Option<Action> {
        self.0
            .iter()
            .find(|(_, keys)| keys.contains(&key))
            .map(|(action, _)| *action)
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
            Self::CtrlC => f.write_str("^C"),
        }
    }
}

/// Key names as written in config: a single character, or one of the names below.
impl FromStr for Key {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        if let Some(c) = chars.next()
            && chars.next().is_none()
        {
            return Ok(Self::Char(c));
        }
        Ok(match s.to_ascii_lowercase().as_str() {
            "tab" => Self::Tab,
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            "enter" => Self::Enter,
            "esc" => Self::Esc,
            "backspace" => Self::Backspace,
            "ctrl+c" => Self::CtrlC,
            _ => {
                return Err(format!(
                    "unknown key {s:?}; use one character or tab, up, down, left, right, enter, esc, backspace, ctrl+c"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_lazygit() {
        let keys = Keymap::default();
        assert_eq!(keys.action(Key::Char('q')), Some(Action::Quit));
        assert_eq!(keys.action(Key::CtrlC), Some(Action::Quit));
        assert_eq!(keys.action(Key::Char(']')), Some(Action::NextTab));
        assert_eq!(keys.action(Key::Char('7')), None);
        assert_eq!(keys.label(Action::Down), "j");
        assert_eq!(keys.label(Action::Open), "Enter");
    }

    #[test]
    fn overrides_replace_whole_action() {
        let overrides = BTreeMap::from([
            (Action::NextTab, vec![Key::Char('ø')]),
            (Action::Quit, vec![]),
        ]);
        let keys = Keymap::with_overrides(&overrides);
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
    fn key_names_parse() {
        assert_eq!("l".parse::<Key>(), Ok(Key::Char('l')));
        assert_eq!("ø".parse::<Key>(), Ok(Key::Char('ø')));
        assert_eq!("Enter".parse::<Key>(), Ok(Key::Enter));
        assert_eq!("ctrl+c".parse::<Key>(), Ok(Key::CtrlC));
        assert!("ctrl+x".parse::<Key>().is_err());
        assert!("".parse::<Key>().is_err());
    }
}
