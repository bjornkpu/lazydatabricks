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
        InputMode::TypeToConfirm {
            item,
            expected,
            text,
        } => type_to_confirm(app, &item.confirmation(), expected, text, frame),
        InputMode::ConfirmActions => confirm(
            app,
            "Enable run, repair, cancel and start for this session?",
            frame,
        ),
        InputMode::Params { name, text, .. } => params(app, name, text, frame),
        InputMode::Prompt { text } => prompt(app, text, frame),
        InputMode::Normal
        | InputMode::Filter
        | InputMode::Search
        | InputMode::Help { .. }
        | InputMode::Output { .. } => {}
    }
}

fn menu(app: &App, items: &[MenuItem], selected: usize, frame: &mut Frame) {
    let footer = if app.allow_actions || !items.iter().any(MenuItem::needs_actions) {
        "Enter: choose │ Esc: close"
    } else {
        "read-only: --allow-actions to enable"
    };
    let title = match items.first() {
        Some(MenuItem::SwitchProfile { .. }) => " Profiles ",
        Some(MenuItem::CopyText { .. }) => " Copy ",
        Some(MenuItem::Filter(_)) => " Filter ",
        _ => " Actions ",
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
        .title(title)
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

/// An irreversible action: the question, then the name being typed back under it.
fn type_to_confirm(app: &App, question: &str, expected: &str, text: &str, frame: &mut Frame) {
    let footer = "type the name │ Enter: do it │ Esc: cancel";
    let typed = format!(" {text}▌");
    let width = question
        .chars()
        .count()
        .max(footer.chars().count())
        .max(expected.chars().count().saturating_add(2))
        .max(typed.chars().count());
    let area = chrome::centered(chrome::columns(width), chrome::rows(2), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(theme::palette(app).danger))
        .title(" Confirm ")
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(
        Paragraph::new(vec![Line::from(question).centered(), Line::from(typed)]).block(block),
        area,
    );
}

/// The `:` prompt: one `databricks` CLI line, the profile added on Enter.
fn prompt(app: &App, text: &str, frame: &mut Frame) {
    let title = " databricks … ";
    let footer = "{{job_id}} {{run_id}} {{name}} {{host}} expand │ Enter: run │ Esc: cancel";
    let shown = format!(" databricks {text}▌");
    let width = footer
        .chars()
        .count()
        .max(title.chars().count())
        .max(shown.chars().count().saturating_add(1));
    let area = chrome::centered(chrome::columns(width), chrome::rows(1), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(theme::palette(app).notice))
        .title(title)
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(Paragraph::new(shown).block(block), area);
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
