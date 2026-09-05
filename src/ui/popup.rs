//! What a custom command printed, over everything else. Scrolls like the help overlay.

use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{chrome, theme};
use crate::app::{App, InputMode};

pub fn draw(app: &App, frame: &mut Frame) {
    let InputMode::Output {
        title,
        lines,
        scroll,
    } = &app.input
    else {
        return;
    };
    let palette = theme::palette(app);
    let screen = frame.area();
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count().saturating_add(2));
    // Two columns and one row of margin keep the frame visible around the overlay.
    let area = chrome::centered(
        chrome::columns(width).min(screen.width.saturating_sub(4)),
        chrome::rows(lines.len()).min(screen.height.saturating_sub(2)),
        screen,
    );
    let hidden = chrome::rows(lines.len()).saturating_sub(area.height);
    let footer = if hidden > 0 {
        "j/k: scroll │ y: copy │ Esc: close"
    } else {
        "y: copy │ Esc: close"
    };
    let block = Block::bordered()
        .border_type(palette.border)
        .border_style(Style::new().fg(palette.accent))
        .title(format!(" {title} "))
        .title_bottom(Line::from(footer).centered());
    let scroll = u16::try_from((*scroll).min(usize::from(hidden))).unwrap_or(u16::MAX);
    let text: Vec<Line> = lines.iter().map(|line| Line::raw(line.as_str())).collect();
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(text).scroll((scroll, 0)).block(block), area);
}
