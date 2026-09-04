//! The contextual hint bar: only the actions valid for the focused panel, never a static list.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;

use crate::app::Panel;

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[must_use]
pub const fn hints(focus: Panel, filtering: bool) -> &'static str {
    if filtering {
        return " Type to filter │ Keep: Enter │ Clear: Esc │ Select: ↑/↓";
    }
    match focus {
        Panel::Status => " Focus: 0-3/Tab │ Screen: + │ Log: @ │ Quit: q",
        Panel::Jobs | Panel::Pipelines => {
            " Select: j/k │ Filter: / │ Mine: m │ Open: Enter │ Focus: 0-3 │ Quit: q"
        }
        Panel::Main => " Tabs: h/l │ Back: Esc │ Screen: + │ Log: @ │ Quit: q",
    }
}

pub fn draw(focus: Panel, filtering: bool, area: Rect, frame: &mut Frame) {
    let dim = Style::new().add_modifier(Modifier::DIM);
    frame.render_widget(Paragraph::new(hints(focus, filtering)).style(dim), area);
    frame.render_widget(Paragraph::new(VERSION).style(dim).right_aligned(), area);
}
