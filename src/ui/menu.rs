//! The `x` menu and its confirmation, drawn over everything else.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListState, Paragraph};

use super::chrome;
use crate::app::{App, InputMode, MenuItem};

pub fn draw(app: &App, frame: &mut Frame) {
    match &app.input {
        InputMode::Menu { items, selected } => menu(app, items, *selected, frame),
        InputMode::Confirm(item) => confirm(item, frame),
        InputMode::Normal | InputMode::Filter => {}
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
    let area = centered(columns(width), rows(items.len()), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(Color::Yellow))
        .title(" Actions ")
        .title_bottom(Line::from(footer).centered());
    let list = List::new(labels)
        .block(block)
        .highlight_style(chrome::highlight(true));
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn confirm(item: &MenuItem, frame: &mut Frame) {
    let question = item.confirmation();
    let footer = "y: yes │ any other key: no";
    let width = question.chars().count().max(footer.chars().count());
    let area = centered(columns(width), rows(1), frame.area());
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(Color::Red))
        .title(" Confirm ")
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(Paragraph::new(question).centered().block(block), area);
}

/// Text width plus borders and a space each side, as a terminal column count.
fn columns(text_width: usize) -> u16 {
    u16::try_from(text_width)
        .unwrap_or(u16::MAX)
        .saturating_add(4)
}

/// Line count plus the two border rows.
fn rows(lines: usize) -> u16 {
    u16::try_from(lines).unwrap_or(u16::MAX).saturating_add(2)
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    area
}
