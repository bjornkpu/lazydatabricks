//! Which panel has focus, and how much of the screen it takes.

/// The panels, numbered as in their titles. `Main` is `[0]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Status,
    Jobs,
    Pipelines,
    Main,
}

impl Panel {
    /// The side column, top to bottom.
    pub const SIDE: [Self; 3] = [Self::Status, Self::Jobs, Self::Pipelines];

    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Status => 1,
            Self::Jobs => 2,
            Self::Pipelines => 3,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Main => "Main",
            Self::Status => "Status",
            Self::Jobs => "Jobs",
            Self::Pipelines => "Pipelines",
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
            Self::Pipelines | Self::Main => Self::Status,
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
        assert_eq!(Panel::from_digit('4'), None);
        assert_eq!(Panel::from_digit('j'), None);
    }

    #[test]
    fn tab_cycles_side_panels_only() {
        assert_eq!(Panel::Status.next_side(), Panel::Jobs);
        assert_eq!(Panel::Jobs.next_side(), Panel::Pipelines);
        assert_eq!(Panel::Pipelines.next_side(), Panel::Status);
        assert_eq!(Panel::Main.next_side(), Panel::Status);
    }

    #[test]
    fn screen_mode_cycles() {
        assert_eq!(ScreenMode::Normal.next(), ScreenMode::Half);
        assert_eq!(ScreenMode::Half.next(), ScreenMode::Full);
        assert_eq!(ScreenMode::Full.next(), ScreenMode::Normal);
    }
}
