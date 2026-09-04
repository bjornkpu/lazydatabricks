//! The side column: `[1]` Status, `[2]` Jobs, `[3]` Pipelines.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListState, Paragraph};

use super::{chrome, theme};
use crate::app::{App, InputMode, Panel};

pub fn status(app: &App, area: Rect, frame: &mut Frame) {
    let block = chrome::panel(
        Panel::Status,
        app.focus == Panel::Status,
        Line::default(),
        None,
        &theme::palette(app),
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
    let filtering = app.input == InputMode::Filter;
    if filtering || !app.filter.text.is_empty() {
        let cursor = if filtering { "▌" } else { "" };
        suffix.push_span(Span::styled(
            format!(" /{}{cursor}", app.filter.text),
            Style::new().fg(Color::Yellow),
        ));
    }
    let counter = app.jobs.counter();
    let palette = theme::palette(app);
    let block = chrome::panel(Panel::Jobs, focused, suffix, Some(&counter), &palette);
    if let Some(error) = &app.error {
        frame.render_widget(chrome::error(error, block, &palette), area);
        return;
    }
    // `2m ✓ name`: age of the latest run, its result, then the name. Blank age and a dot when
    // no run is known yet.
    let rows = app.jobs.items().iter().map(|job| {
        let latest = app.latest_runs.get(&job.id);
        let age = match (latest.and_then(|run| run.start_time), app.now) {
            (Some(started), Some(now)) => theme::age_short(now.duration_since(started)),
            _ => String::new(),
        };
        let (glyph, color) = latest.map_or(('·', Color::DarkGray), theme::run_glyph);
        Line::from(vec![
            Span::styled(
                format!("{age:>3} "),
                Style::new().add_modifier(Modifier::DIM),
            ),
            Span::styled(glyph.to_string(), Style::new().fg(color)),
            Span::raw(format!(" {}", job.settings.name)),
        ])
    });
    let list = List::new(rows)
        .block(block)
        .highlight_style(chrome::highlight(focused, &palette));
    // Local widget state built from `App`: the render stays a pure function of the app.
    let mut state = ListState::default().with_selected(app.jobs.selected_index());
    frame.render_stateful_widget(list, area, &mut state);
}

pub fn pipelines(app: &App, area: Rect, frame: &mut Frame) {
    let focused = app.focus == Panel::Pipelines;
    let spinner = if app.pipelines_loading() {
        Line::from(format!(" {}", app.spinner_glyph()))
    } else {
        Line::default()
    };
    let counter = app.pipelines.counter();
    let palette = theme::palette(app);
    let block = chrome::panel(Panel::Pipelines, focused, spinner, Some(&counter), &palette);
    if let Some(error) = &app.pipelines_error {
        frame.render_widget(chrome::error(error, block, &palette), area);
        return;
    }
    let rows = app.pipelines.items().iter().map(|pipeline| {
        let (glyph, color) = theme::pipeline_glyph(pipeline);
        Line::from(vec![
            Span::styled(glyph.to_string(), Style::new().fg(color)),
            Span::raw(format!(" {}", pipeline.name)),
        ])
    });
    let list = List::new(rows)
        .block(block)
        .highlight_style(chrome::highlight(focused, &palette));
    let mut state = ListState::default().with_selected(app.pipelines.selected_index());
    frame.render_stateful_widget(list, area, &mut state);
}
