//! Which panel has focus, how much of the screen it takes, and which main-panel tabs it offers.

/// The panels, numbered as in their titles. `Main` is `[0]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Status,
    Jobs,
    Pipelines,
    Compute,
    Main,
}

impl Panel {
    /// The side column, top to bottom.
    pub const SIDE: [Self; 4] = [Self::Status, Self::Jobs, Self::Pipelines, Self::Compute];

    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Status => 1,
            Self::Jobs => 2,
            Self::Pipelines => 3,
            Self::Compute => 4,
        }
    }

    /// Title text after the number. The main panel's title is its tab list instead.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Main => "",
            Self::Status => "Status",
            Self::Jobs => "Jobs",
            Self::Pipelines => "Pipelines",
            Self::Compute => "Compute",
        }
    }

    /// The panel a digit key focuses.
    #[must_use]
    pub const fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '0' => Some(Self::Main),
            '1' => Some(Self::Status),
            '2' => Some(Self::Jobs),
            '3' => Some(Self::Pipelines),
            '4' => Some(Self::Compute),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_side(self) -> bool {
        !matches!(self, Self::Main)
    }

    /// `Tab`: next side panel, wrapping. From the main panel, back to the first side panel.
    #[must_use]
    pub const fn next_side(self) -> Self {
        match self {
            Self::Status => Self::Jobs,
            Self::Jobs => Self::Pipelines,
            Self::Pipelines => Self::Compute,
            Self::Compute | Self::Main => Self::Status,
        }
    }

    /// Main-panel tabs for this side panel. Empty until the panel has data worth viewing.
    #[must_use]
    pub const fn tabs(self) -> &'static [Tab] {
        match self {
            Self::Status => &[Tab::Profile, Tab::Config],
            Self::Jobs => &[Tab::Runs, Tab::Detail, Tab::Json, Tab::Output],
            Self::Pipelines => &[Tab::Updates, Tab::Detail, Tab::Json],
            Self::Compute => &[Tab::Detail],
            Self::Main => &[],
        }
    }
}

/// How the side column shares its height. An enum for the same reason as `ComputePanel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideLayout {
    /// The panel in context gets `EXPANDED_WEIGHT` shares, the other lists one each.
    Expand,
    Even,
}

impl SideLayout {
    #[must_use]
    pub const fn from_config(expand: bool) -> Self {
        if expand { Self::Expand } else { Self::Even }
    }
}

/// Whether config allows the `[4] Compute` panel at all. An enum rather than a bool so the
/// state struct stays readable; `App` already carries three flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputePanel {
    Enabled,
    Disabled,
}

impl ComputePanel {
    #[must_use]
    pub const fn from_config(enabled: bool) -> Self {
        if enabled {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }

    #[must_use]
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// A view in the main panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Profile,
    Runs,
    Updates,
    Detail,
    /// The raw settings, pretty-printed: `jobs/get` or `pipelines/get` as Databricks sent it.
    Json,
    /// What every task of the viewed run printed or returned.
    Output,
    /// The effective configuration as TOML.
    Config,
}

impl Tab {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Profile => "Profile",
            Self::Runs => "Runs",
            Self::Updates => "Updates",
            Self::Detail => "Detail",
            Self::Json => "JSON",
            Self::Output => "Output",
            Self::Config => "Config",
        }
    }
}

/// How much room the focused panel gets. `+` cycles these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenMode {
    /// Side column one third, all side panels visible.
    #[default]
    Normal,
    /// Side column one half; unfocused side panels collapse to their title line.
    Half,
    /// The focused panel alone.
    Full,
}

impl ScreenMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Normal => Self::Half,
            Self::Half => Self::Full,
            Self::Full => Self::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_map_to_panels() {
        assert_eq!(Panel::from_digit('0'), Some(Panel::Main));
        assert_eq!(Panel::from_digit('2'), Some(Panel::Jobs));
        assert_eq!(Panel::from_digit('4'), Some(Panel::Compute));
        assert_eq!(Panel::from_digit('5'), None);
        assert_eq!(Panel::from_digit('j'), None);
    }

    #[test]
    fn tab_cycles_side_panels_only() {
        assert_eq!(Panel::Status.next_side(), Panel::Jobs);
        assert_eq!(Panel::Jobs.next_side(), Panel::Pipelines);
        assert_eq!(Panel::Pipelines.next_side(), Panel::Compute);
        assert_eq!(Panel::Compute.next_side(), Panel::Status);
        assert_eq!(Panel::Main.next_side(), Panel::Status);
    }

    #[test]
    fn jobs_offer_runs_first() {
        assert_eq!(Panel::Jobs.tabs().first(), Some(&Tab::Runs));
        assert_eq!(Panel::Pipelines.tabs().first(), Some(&Tab::Updates));
        assert!(Panel::Main.tabs().is_empty());
    }

    #[test]
    fn screen_mode_cycles() {
        assert_eq!(ScreenMode::Normal.next(), ScreenMode::Half);
        assert_eq!(ScreenMode::Half.next(), ScreenMode::Full);
        assert_eq!(ScreenMode::Full.next(), ScreenMode::Normal);
    }
}
