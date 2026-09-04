//! The side column: `[1]` Status, `[2]` Jobs, `[3]` Pipelines.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListState, Paragraph, Wrap};

use super::{chrome, theme};
use crate::app::{App, Panel};

pub fn status(app: &App, area: Rect, frame: &mut Frame) {
    let block = chrome::panel(
        Panel::Status,
        app.focus == Panel::Status,
        Line::default(),
        None,
    );
    // Whether we know who we are: resolved, failed, or still asking.
    let (glyph, color) = match (&app.me, &app.me_error) {
        (Some(_), _) => ('✓', Color::Green),
        (None, Some(_)) => ('✗', Color::Red),
        (None, None) => ('◐', Color::Yellow),
    };
    let host = app.host.trim_start_matches("https://");
    let identity = Line::from(vec![
        Span::styled(glyph.to_string(), Style::new().fg(color)),
        Span::raw(format!(" {} → {host}", app.profile)),
    ]);
    let mut summary = app.filter_summary();
    if let Some(age) = app.jobs_age() {
        summary.push_str(" · ");
        summary.push_str(&theme::age(age));
    }
    let second = app
        .me_error
        .as_ref()
        .map_or(summary, |error| format!("me: {error}"));
    frame.render_widget(
        Paragraph::new(vec![identity, Line::from(second)]).block(block),
        area,
    );
}

pub fn jobs(app: &App, area: Rect, frame: &mut Frame) {
    let focused = app.focus == Panel::Jobs;
    let mut suffix = Line::default();
    if app.loading {
        suffix.push_span(format!(" {}", app.spinner_glyph()));
    }
    if app.filtering || !app.filter.text.is_empty() {
        let cursor = if app.filtering { "▌" } else { "" };
        suffix.push_span(Span::styled(
            format!(" /{}{cursor}", app.filter.text),
            Style::new().fg(Color::Yellow),
        ));
    }
    let counter = app.jobs.counter();
    let block = chrome::panel(Panel::Jobs, focused, suffix, Some(&counter));
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
