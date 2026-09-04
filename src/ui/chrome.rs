//! Panel borders: numbered titles, focus accent, "n of m" counter.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;

use crate::app::Panel;

/// A bordered panel titled `─[n]─Name`, accented when focused. `suffix` continues the title
/// (a spinner, the tab list); `counter` is the `n of m` bottom-right.
pub fn panel(
    panel: Panel,
    focused: bool,
    suffix: Line<'static>,
    counter: Option<&str>,
) -> Block<'static> {
    let (border, title_style) = if focused {
        (
            Style::new().fg(Color::Green),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::new(), Style::new().add_modifier(Modifier::DIM))
    };
    let mut title = Line::from(Span::styled(
        format!("─[{}]─{}", panel.number(), panel.name()),
        title_style,
    ));
    title.extend(suffix);
    let mut block = Block::bordered().border_style(border).title(title);
    if let Some(counter) = counter {
        block = block.title_bottom(Line::from(counter.to_owned()).right_aligned());
    }
    block
}

/// Highlight for the selected row: loud when the panel is focused, dim when it is not, so the
/// cursor never disappears on `Tab`.
#[must_use]
pub const fn highlight(focused: bool) -> Style {
    if focused {
        Style::new()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(Color::DarkGray)
    }
}

/// Fits `s` into `width` columns: pads short, truncates long with `…`.
#[must_use]
pub fn fit(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return format!("{s:<width$}");
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcd", 4), "abcd");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("æøå", 2), "æ…");
        assert_eq!(fit("abc", 0), "…");
    }
}
