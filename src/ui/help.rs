//! `?`: the bindings that apply right now, read from the live keymap so overrides show.

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{chrome, theme};
use crate::app::{Action, App, InputMode, Keymap, Panel};

pub fn draw(app: &App, frame: &mut Frame) {
    if app.input != InputMode::Help {
        return;
    }
    let palette = theme::palette(app);
    let rows = bindings(app.focus, &app.keys);
    let key_width = rows
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let width = rows
        .iter()
        .map(|(_, d)| {
            key_width
                .saturating_add(2)
                .saturating_add(d.chars().count())
        })
        .max()
        .unwrap_or(0);
    let area = chrome::centered(
        chrome::columns(width),
        chrome::rows(rows.len()),
        frame.area(),
    );
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:>key_width$}  "),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(*description),
            ])
        })
        .collect();
    let panel_name = match app.focus {
        Panel::Main => "Main",
        other => other.name(),
    };
    let block = Block::bordered()
        .border_style(Style::new().fg(palette.accent))
        .title(format!(" Keybindings ─ {panel_name} "))
        .title_bottom(Line::from("Esc: close").centered());
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The focused panel's bindings first, then the ones that work everywhere.
fn bindings(focus: Panel, keys: &Keymap) -> Vec<(String, &'static str)> {
    let k = |action| keys.labels(action);
    let mut rows = match focus {
        Panel::Jobs => vec![
            (k(Action::Down), "next job"),
            (k(Action::Up), "previous job"),
            (k(Action::First), "first job"),
            (k(Action::Last), "last job"),
            (k(Action::Filter), "filter by name"),
            (k(Action::MineOnly), "toggle mine only"),
            (k(Action::Open), "focus the main panel"),
            (k(Action::Menu), "actions menu"),
            (k(Action::Refresh), "refresh jobs"),
            (k(Action::Browse), "open job in browser"),
            (k(Action::Copy), "copy job URL"),
        ],
        Panel::Main => vec![
            (k(Action::Down), "next run"),
            (k(Action::Up), "previous run"),
            (k(Action::Open), "open the run: tasks and message"),
            (k(Action::NextTab), "next tab"),
            (k(Action::PrevTab), "previous tab"),
            (k(Action::Back), "back: close the run, then the panel"),
            (k(Action::Menu), "actions menu"),
            (k(Action::Refresh), "refresh runs"),
            (k(Action::Browse), "open job in browser"),
            (k(Action::Copy), "copy job URL"),
        ],
        Panel::Pipelines => vec![
            (k(Action::Down), "next pipeline"),
            (k(Action::Up), "previous pipeline"),
            (k(Action::Filter), "filter by name"),
            (k(Action::MineOnly), "toggle mine only"),
            (k(Action::Open), "focus the main panel"),
            (k(Action::Refresh), "refresh pipelines"),
            (k(Action::Browse), "open pipeline in browser"),
            (k(Action::Copy), "copy pipeline URL"),
        ],
        Panel::Status => vec![(k(Action::Refresh), "refresh jobs and pipelines")],
    };
    rows.extend([
        ("0-3".to_owned(), "focus panel by number"),
        (k(Action::NextPanel), "next side panel"),
        (k(Action::RefreshAll), "refresh everything"),
        (k(Action::ScreenMode), "cycle screen mode"),
        (k(Action::ToggleLog), "toggle the API log"),
        (k(Action::Help), "this list"),
        (k(Action::Quit), "quit"),
    ]);
    rows
}
