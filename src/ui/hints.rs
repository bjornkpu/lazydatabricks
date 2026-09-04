//! The contextual hint bar: only the actions valid for the focused panel, never a static list.
//! Labels come from the live keymap so overrides show up here too.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;

use crate::app::{Action, Keymap, Panel};

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[must_use]
pub fn hints(focus: Panel, filtering: bool, keys: &Keymap) -> String {
    if filtering {
        return " Type to filter │ Keep: Enter │ Clear: Esc │ Select: ↑/↓".to_owned();
    }
    let k = |action| keys.label(action);
    match focus {
        Panel::Status => format!(
            " Focus: 0-3/{} │ Screen: {} │ Log: {} │ Quit: {}",
            k(Action::NextPanel),
            k(Action::ScreenMode),
            k(Action::ToggleLog),
            k(Action::Quit)
        ),
        Panel::Jobs | Panel::Pipelines => format!(
            " Select: {}/{} │ Filter: {} │ Mine: {} │ Open: {} │ Focus: 0-3 │ Quit: {}",
            k(Action::Down),
            k(Action::Up),
            k(Action::Filter),
            k(Action::MineOnly),
            k(Action::Open),
            k(Action::Quit)
        ),
        Panel::Main => format!(
            " Tabs: {}/{} │ Back: {} │ Screen: {} │ Log: {} │ Quit: {}",
            k(Action::PrevTab),
            k(Action::NextTab),
            k(Action::Back),
            k(Action::ScreenMode),
            k(Action::ToggleLog),
            k(Action::Quit)
        ),
    }
}

pub fn draw(focus: Panel, filtering: bool, keys: &Keymap, area: Rect, frame: &mut Frame) {
    let dim = Style::new().add_modifier(Modifier::DIM);
    frame.render_widget(
        Paragraph::new(hints(focus, filtering, keys)).style(dim),
        area,
    );
    frame.render_widget(Paragraph::new(VERSION).style(dim).right_aligned(), area);
}
