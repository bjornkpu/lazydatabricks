//! The side column: `[1]` Status, `[2]` Jobs, `[3]` Pipelines.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListState, Paragraph, Wrap};

use super::chrome;
use crate::app::{App, Panel};

pub fn status(app: &App, area: Rect, frame: &mut Frame) {
    let block = chrome::panel(
        Panel::Status,
        app.focus == Panel::Status,
        Line::default(),
        None,
    );
    let host = app.host.trim_start_matches("https://");
    let text = format!("{} → {host}", app.profile);
    frame.render_widget(Paragraph::new(text).block(block), area);
}

pub fn jobs(app: &App, area: Rect, frame: &mut Frame) {
    let focused = app.focus == Panel::Jobs;
    let spinner = if app.loading {
        Line::from(format!(" {}", app.spinner_glyph()))
    } else {
        Line::default()
    };
    let counter = app.jobs.counter();
    let block = chrome::panel(Panel::Jobs, focused, spinner, Some(&counter));
    if let Some(error) = &app.error {
        let paragraph = Paragraph::new(error.as_str())
            .style(Style::new().fg(Color::Red))
            .wrap(Wrap { trim: false })
            .block(block);
        frame.render_widget(paragraph, area);
        return;
    }
    let names = app
        .jobs
        .items()
        .iter()
        .map(|job| job.settings.name.as_str());
    let list = List::new(names)
        .block(block)
        .highlight_style(chrome::highlight(focused));
    // Local widget state built from `App`: the render stays a pure function of the app.
    let mut state = ListState::default().with_selected(app.jobs.selected_index());
    frame.render_stateful_widget(list, area, &mut state);
}

pub fn pipelines(app: &App, area: Rect, frame: &mut Frame) {
    let block = chrome::panel(
        Panel::Pipelines,
        app.focus == Panel::Pipelines,
        Line::default(),
        Some("0 of 0"),
    );
    frame.render_widget(block, area);
}
