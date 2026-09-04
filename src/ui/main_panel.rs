//! `[0]`: tabs over whatever the context side panel has selected.

use jiff::SignedDuration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};

use super::{chrome, theme};
use crate::app::{App, Load, Panel, Tab};

pub fn draw(app: &App, area: Rect, frame: &mut Frame) {
    let mut title = tabs_title(app);
    if app.runs_busy() {
        title.push_span(Span::raw(format!(" {}", app.spinner_glyph())));
    }
    let palette = theme::palette(app);
    let block = chrome::panel(Panel::Main, app.focus == Panel::Main, title, None, &palette);
    match app.active_tab() {
        Some(Tab::Runs) => runs(app, block, area, frame),
        Some(Tab::Updates) => updates(app, block, area, frame),
        Some(Tab::Detail) if app.context == Panel::Pipelines => {
            pipeline_detail(app, block, area, frame);
        }
        Some(Tab::Detail) => detail(app, block, area, frame),
        Some(Tab::Profile) => profile(app, block, area, frame),
        None => frame.render_widget(block, area),
    }
}

/// `Runs - Detail`, the active one bold and underlined, the rest dim.
fn tabs_title(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, tab) in app.context.tabs().iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" - "));
        }
        let style = if index == app.tab {
            Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().add_modifier(Modifier::DIM)
        };
        spans.push(Span::styled(tab.name(), style));
    }
    Line::from(spans)
}

fn runs(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let runs = match &app.runs {
        Load::Failed(error) => {
            frame.render_widget(chrome::error(error, block, &theme::palette(app)), area);
            return;
        }
        Load::Loaded(runs) => runs.as_slice(),
        Load::Idle | Load::Loading => &[],
    };
    let header = Row::new(["Run ID", "Started", "Duration", "Result"])
        .style(Style::new().add_modifier(Modifier::BOLD));
    let rows = runs.iter().map(|run| {
        let (glyph, color) = theme::run_glyph(run);
        let started = run
            .start_time
            .map_or_else(|| "-".to_owned(), |ts| theme::clock(ts, &app.tz));
        let duration = theme::run_duration(run).map_or_else(|| "-".to_owned(), theme::duration);
        let result = Line::from(vec![
            Span::styled(glyph.to_string(), Style::new().fg(color)),
            Span::raw(format!(" {}", theme::run_result(run))),
        ]);
        Row::new([
            Cell::from(run.id.to_string()),
            Cell::from(started),
            Cell::from(duration),
            Cell::from(result),
        ])
    });
    let widths = [
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Fill(1),
    ];
    frame.render_widget(Table::new(rows, widths).header(header).block(block), area);
}

/// The latest updates Databricks lists with the pipeline. No extra call; a handful of rows.
fn updates(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let Some(pipeline) = app.pipelines.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let header =
        Row::new(["Update", "Created", "State"]).style(Style::new().add_modifier(Modifier::BOLD));
    let rows = pipeline.latest_updates.iter().map(|update| {
        let (glyph, color) = match update.state {
            s if s.is_done() && s == crate::api::models::UpdateState::Completed => {
                ('✓', ratatui::style::Color::Green)
            }
            s if s.is_done() => ('✗', ratatui::style::Color::Red),
            _ => ('◐', ratatui::style::Color::Yellow),
        };
        let created = update
            .creation_time
            .map_or_else(|| "-".to_owned(), |ts| theme::clock(ts, &app.tz));
        // The first block of the UUID is enough to tell updates apart on screen.
        let short_id: String = update.id.chars().take(8).collect();
        Row::new([
            Cell::from(short_id),
            Cell::from(created),
            Cell::from(Line::from(vec![
                Span::styled(glyph.to_string(), Style::new().fg(color)),
                Span::raw(format!(" {}", update.state.as_str())),
            ])),
        ])
    });
    let widths = [
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Fill(1),
    ];
    frame.render_widget(Table::new(rows, widths).header(header).block(block), area);
}

fn pipeline_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let Some(pipeline) = app.pipelines.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let lines = vec![
        field("Name", pipeline.name.clone()),
        field("Pipeline ID", pipeline.id.clone()),
        field("State", pipeline.state.as_str().to_owned()),
        field("Creator", pipeline.creator_user_name.clone()),
        field("Updates", pipeline.latest_updates.len().to_string()),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let Some(job) = app.jobs.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let settings = &job.settings;
    let dash = || "-".to_owned();
    let tags = if settings.tags.is_empty() {
        dash()
    } else {
        settings
            .tags
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let lines = vec![
        field("Name", settings.name.clone()),
        field("Job ID", job.id.to_string()),
        field("Creator", job.creator_user_name.clone()),
        field("Run as", job.run_as_user_name.clone()),
        field("Format", settings.format.clone().unwrap_or_else(dash)),
        field(
            "Max concurrent",
            settings
                .max_concurrent_runs
                .map_or_else(dash, |n| n.to_string()),
        ),
        field(
            "Timeout",
            settings.timeout_seconds.map_or_else(dash, |secs| {
                theme::duration(SignedDuration::from_secs(secs))
            }),
        ),
        field("Tags", tags),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn profile(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let lines = vec![
        field("Profile", app.profile.clone()),
        field("Host", app.host.clone()),
        field("Jobs", app.jobs.items().len().to_string()),
        field("Config", app.config_note.clone()),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One `Label   value` line. Values are owned because the frame outlives no borrow of `App`.
fn field(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<15}"),
            Style::new().add_modifier(Modifier::DIM),
        ),
        Span::raw(value),
    ])
}
