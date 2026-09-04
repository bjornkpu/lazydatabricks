//! The contextual hint bar: only the actions valid for the focused panel, never a static list.
//! Labels come from the live keymap so overrides show up here too. A notice (something just
//! happened, or could not) takes the bar over until the next key.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;

use crate::app::{Action, App, InputMode, Keymap, Panel};

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[must_use]
pub fn hints(focus: Panel, input: &InputMode, keys: &Keymap) -> String {
    let k = |action| keys.label(action);
    match input {
        InputMode::Filter => {
            return " Type to filter │ Keep: Enter │ Clear: Esc │ Select: ↑/↓".to_owned();
        }
        InputMode::Menu { .. } => return " Choose: j/k │ Confirm: Enter │ Close: Esc".to_owned(),
        InputMode::Confirm(_) => return " Send: y │ Back: any other key".to_owned(),
        InputMode::Normal => {}
    }
    match focus {
        Panel::Status => format!(
            " Focus: 0-3/{} │ Screen: {} │ Log: {} │ Quit: {}",
            k(Action::NextPanel),
            k(Action::ScreenMode),
            k(Action::ToggleLog),
            k(Action::Quit)
        ),
        Panel::Jobs | Panel::Pipelines => format!(
            " Select: {}/{} │ Filter: {} │ Mine: {} │ Open: {} │ Actions: {} │ Quit: {}",
            k(Action::Down),
            k(Action::Up),
            k(Action::Filter),
            k(Action::MineOnly),
            k(Action::Open),
            k(Action::Menu),
            k(Action::Quit)
        ),
        Panel::Main => format!(
            " Tabs: {}/{} │ Back: {} │ Actions: {} │ Screen: {} │ Log: {} │ Quit: {}",
            k(Action::PrevTab),
            k(Action::NextTab),
            k(Action::Back),
            k(Action::Menu),
            k(Action::ScreenMode),
            k(Action::ToggleLog),
            k(Action::Quit)
        ),
    }
}

pub fn draw(app: &App, area: Rect, frame: &mut Frame) {
    let dim = Style::new().add_modifier(Modifier::DIM);
    if let Some(notice) = &app.notice {
        let style = Style::new().fg(Color::Yellow);
        frame.render_widget(Paragraph::new(format!(" {notice}")).style(style), area);
        return;
    }
    let text = hints(app.focus, &app.input, &app.keys);
    frame.render_widget(Paragraph::new(text).style(dim), area);
    frame.render_widget(Paragraph::new(VERSION).style(dim).right_aligned(), area);
}
