//! The side column: `[1]` Status, `[2]` Jobs, `[3]` Pipelines.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListState, Paragraph};

use super::{chrome, theme};
use crate::app::{App, InputMode, Panel};
use crate::error::AppError;

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
    suffix.push_span(Span::styled(
        format!(" by {}", app.sort.as_str()),
        Style::new().add_modifier(Modifier::DIM),
    ));
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
    let mut block = chrome::panel(Panel::Jobs, focused, suffix, Some(&counter), &palette);
    if let Some(error) = &app.error {
        if app.all_jobs.is_empty() {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return;
        }
        block = stale(block, error, &counter, area, &palette);
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
    let mut spinner = if app.pipelines_loading() {
        Line::from(format!(" {}", app.spinner_glyph()))
    } else {
        Line::default()
    };
    spinner.push_span(Span::styled(
        format!(" by {}", app.sort.as_str()),
        Style::new().add_modifier(Modifier::DIM),
    ));
    let counter = app.pipelines.counter();
    let palette = theme::palette(app);
    let mut block = chrome::panel(Panel::Pipelines, focused, spinner, Some(&counter), &palette);
    if let Some(error) = &app.pipelines_error {
        if app.all_pipelines.is_empty() {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return;
        }
        block = stale(block, error, &counter, area, &palette);
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

/// The last refresh failed but there is a list to show: the error goes in the bottom border,
/// left of the counter, and the list stays. Stale beats blank.
fn stale(
    block: Block<'static>,
    error: &AppError,
    counter: &str,
    area: Rect,
    palette: &theme::Palette,
) -> Block<'static> {
    // Borders, a space each side, and the counter with its own gap.
    let width = usize::from(area.width)
        .saturating_sub(4)
        .saturating_sub(counter.chars().count())
        .saturating_sub(1);
    block.title_bottom(Line::styled(
        chrome::fit(&format!("✗ {}", error.short()), width)
            .trim_end()
            .to_owned(),
        Style::new().fg(palette.error),
    ))
}
