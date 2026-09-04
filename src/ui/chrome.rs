//! Panel borders: numbered titles, focus accent, "n of m" counter.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Block;

use crate::app::Panel;

/// A bordered panel titled `─[n]─Name`, accented when focused. `suffix` goes after the name
/// (the jobs spinner); `counter` is the `n of m` bottom-right.
pub fn panel(panel: Panel, focused: bool, suffix: &str, counter: Option<&str>) -> Block<'static> {
    let title = format!("─[{}]─{}{suffix}", panel.number(), panel.name());
    let (border, title_style) = if focused {
        (
            Style::new().fg(Color::Green),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::new(), Style::new().add_modifier(Modifier::DIM))
    };
    let mut block = Block::bordered()
        .border_style(border)
        .title(Line::styled(title, title_style));
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
