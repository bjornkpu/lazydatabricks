//! The contextual hint bar: only the actions valid for the focused panel, never a static list.
//! Labels come from the live keymap so overrides show up here too. A notice (something just
//! happened, or could not) takes the bar over until the next key.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{Action, App, InputMode, Keymap, Panel};

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

#[must_use]
pub fn hints(focus: Panel, input: &InputMode, keys: &Keymap, viewing_run: bool) -> String {
    let k = |action| keys.label(action);
    match input {
        InputMode::Filter => {
            return " Type to filter │ Keep: Enter │ Clear: Esc │ Select: ↑/↓".to_owned();
        }
        InputMode::Menu { .. } => return " Choose: j/k │ Confirm: Enter │ Close: Esc".to_owned(),
        InputMode::Confirm(_) => return " Send: y │ Back: any other key".to_owned(),
        InputMode::ConfirmActions => return " Enable: y │ Back: any other key".to_owned(),
        InputMode::Params { .. } => {
            return " Type key=value pairs, space separated │ Start: Enter │ Cancel: Esc"
                .to_owned();
        }
        InputMode::Help { .. } => return " Scroll: j/k │ Close: Esc".to_owned(),
        InputMode::Normal => {}
    }
    match focus {
        Panel::Status => format!(
            " Focus: 0-4/{} │ Screen: {} │ Log: {} │ Quit: {} │ Keys: {}",
            k(Action::NextPanel),
            k(Action::ScreenMode),
            k(Action::ToggleLog),
            k(Action::Quit),
            k(Action::Help)
        ),
        Panel::Jobs | Panel::Pipelines | Panel::Compute => format!(
            " Move: {}/{} │ Filter: {} │ Mine: {} │ Status: {} │ Actions: {} │ Quit: {} │ Keys: {}",
            k(Action::Down),
            k(Action::Up),
            k(Action::Filter),
            k(Action::MineOnly),
            k(Action::StatusFilter),
            k(Action::Menu),
            k(Action::Quit),
            k(Action::Help)
        ),
        Panel::Main if viewing_run => format!(
            " Back: {} │ Browser: {} │ Copy URL: {} │ Actions: {} │ Quit: {} │ Keys: {}",
            k(Action::Back),
            k(Action::Browse),
            k(Action::Copy),
            k(Action::Menu),
            k(Action::Quit),
            k(Action::Help)
        ),
        Panel::Main => format!(
            " Move: {}/{} │ Open: {} │ Back: {} │ Actions: {} │ Quit: {} │ Keys: {}",
            k(Action::Down),
            k(Action::Up),
            k(Action::Open),
            k(Action::Back),
            k(Action::Menu),
            k(Action::Quit),
            k(Action::Help)
        ),
    }
}

pub fn draw(app: &App, area: Rect, frame: &mut Frame) {
    let dim = theme::dim(app);
    if let Some(notice) = &app.notice {
        let style = Style::new().fg(theme::palette(app).notice);
        frame.render_widget(Paragraph::new(format!(" {notice}")).style(style), area);
        return;
    }
    let text = hints(app.focus, &app.input, &app.keys, app.viewing_run.is_some());
    // The version stamp yields to the hints on narrow terminals.
    let fits = text
        .chars()
        .count()
        .saturating_add(VERSION.len())
        .saturating_add(1)
        <= usize::from(area.width);
    frame.render_widget(Paragraph::new(text).style(dim), area);
    if fits {
        frame.render_widget(Paragraph::new(VERSION).style(dim).right_aligned(), area);
    }
}
