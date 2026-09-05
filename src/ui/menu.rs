//! The `x` menu and its confirmation, drawn over everything else.

use ratatui::Frame;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListState, Paragraph};

use super::{chrome, theme};
use crate::app::{App, InputMode, MenuItem};

pub fn draw(app: &App, frame: &mut Frame) {
    match &app.input {
        InputMode::Menu { items, selected } => menu(app, items, *selected, frame),
        InputMode::Confirm(item) => confirm(app, &item.confirmation(), frame),
        InputMode::ConfirmActions => confirm(
            app,
            "Enable run, repair, cancel and start for this session?",
            frame,
        ),
        InputMode::Params { name, text, .. } => params(app, name, text, frame),
        InputMode::Normal | InputMode::Filter | InputMode::Help { .. } => {}
    }
}

fn menu(app: &App, items: &[MenuItem], selected: usize, frame: &mut Frame) {
    let footer = if app.allow_actions {
        "Enter: choose │ Esc: close"
    } else {
        "read-only: --allow-actions to enable"
    };
    let labels: Vec<String> = items.iter().map(MenuItem::label).collect();
    let width = labels
        .iter()
        .map(String::as_str)
        .chain([footer])
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(0);
    let palette = theme::palette(app);
    let area = chrome::centered(
        chrome::columns(width),
        chrome::rows(items.len()),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(palette.notice))
        .title(" Actions ")
        .title_bottom(Line::from(footer).centered());
    let list = List::new(labels)
        .block(block)
        .highlight_style(chrome::highlight(true, &palette));
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn confirm(app: &App, question: &str, frame: &mut Frame) {
    let footer = "y: yes │ any other key: no";
    let width = question.chars().count().max(footer.chars().count());
    let area = chrome::centered(chrome::columns(width), chrome::rows(1), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(theme::palette(app).danger))
        .title(" Confirm ")
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(Paragraph::new(question).centered().block(block), area);
}

/// One line of `key=value` pairs for *Run with parameters*. Enter sends, so no second prompt.
fn params(app: &App, name: &str, text: &str, frame: &mut Frame) {
    let title = format!(" Parameters for {name} ");
    let footer = "key=value key2=value2 │ Enter: start │ Esc: cancel";
    let width = footer
        .chars()
        .count()
        .max(title.chars().count())
        .max(text.chars().count().saturating_add(2));
    let area = chrome::centered(chrome::columns(width), chrome::rows(1), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(theme::palette(app).notice))
        .title(title)
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(Paragraph::new(format!(" {text}▌")).block(block), area);
}
