//! Panel borders: numbered titles, focus accent, "n of m" counter.

use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::theme::Palette;
use crate::app::Panel;
use crate::error::AppError;

/// A bordered panel titled `─[n]─Name`, accented when focused. `suffix` continues the title
/// (a spinner, the tab list); `counter` is the `n of m` bottom-right.
pub fn panel(
    panel: Panel,
    focused: bool,
    suffix: Line<'static>,
    counter: Option<&str>,
    palette: &Palette,
) -> Block<'static> {
    let (border, title_style) = if focused {
        (
            Style::new().fg(palette.accent),
            Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
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

/// A failure in place of a panel's content: red, wrapped, worded by `AppError`.
pub fn error(error: &AppError, block: Block<'static>, palette: &Palette) -> Paragraph<'static> {
    Paragraph::new(error.to_string())
        .style(Style::new().fg(palette.error))
        .wrap(Wrap { trim: false })
        .block(block)
}

/// Highlight for the selected row: loud when the panel is focused, dim when it is not, so the
/// cursor never disappears on `Tab`.
#[must_use]
pub const fn highlight(focused: bool, palette: &Palette) -> Style {
    if focused {
        palette.highlight
    } else {
        palette.highlight_unfocused
    }
}

/// Text width plus borders and a space each side, as a terminal column count.
#[must_use]
pub fn columns(text_width: usize) -> u16 {
    u16::try_from(text_width)
        .unwrap_or(u16::MAX)
        .saturating_add(4)
}

/// Line count plus the two border rows.
#[must_use]
pub fn rows(lines: usize) -> u16 {
    u16::try_from(lines).unwrap_or(u16::MAX).saturating_add(2)
}

/// A box of at most `width` x `height` in the middle of `area`, for overlays.
#[must_use]
pub fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    area
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
