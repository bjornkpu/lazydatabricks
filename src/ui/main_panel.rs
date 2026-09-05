//! `[0]`: tabs over whatever the context side panel has selected.

use jiff::SignedDuration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap};

use super::theme::Palette;
use super::{chrome, theme};
use crate::api::models::{ComputeKind, Run};
use crate::app::{App, Load, Panel, Tab};

/// Inner width below which the runs table drops its Run ID column: ids plus dates plus a
/// result word need about this much.
const NARROW_RUNS_TABLE: u16 = 50;

pub fn draw(app: &App, area: Rect, frame: &mut Frame) {
    let mut title = tabs_title(app);
    if let Some(run_id) = app.viewing_run {
        title.push_span(Span::raw(format!(" › run {run_id}")));
    }
    if app.runs_busy() || app.run_detail == Load::Loading {
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
        Some(Tab::Detail) if app.context == Panel::Compute => {
            cluster_detail(app, block, area, frame);
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
            theme::dim(app)
        };
        spans.push(Span::styled(tab.name(), style));
    }
    Line::from(spans)
}

fn runs(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    if app.viewing_run.is_some() {
        run_detail(app, block, area, frame);
        return;
    }
    let palette = theme::palette(app);
    let runs = match &app.runs {
        Load::Failed(error) => {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return;
        }
        Load::Loaded(runs) => runs,
        Load::Idle | Load::Loading => {
            frame.render_widget(block, area);
            return;
        }
    };
    // Too narrow for every column: the run id goes, never truncated, and Result stays.
    let skip = usize::from(block.inner(area).width < NARROW_RUNS_TABLE);
    let header = Row::new(
        ["Run ID", "Started", "Duration", "Result"]
            .into_iter()
            .skip(skip),
    )
    .style(Style::new().add_modifier(Modifier::BOLD));
    let rows = runs.items().iter().map(|run| {
        let (glyph, color) = theme::run_glyph(run);
        let started = run.start_time.map_or_else(
            || "-".to_owned(),
            |ts| theme::clock(ts, &app.tz, &app.date_format),
        );
        let duration =
            theme::run_duration(run, app.now).map_or_else(|| "-".to_owned(), theme::duration);
        let result = Line::from(vec![
            Span::styled(glyph.to_string(), theme::tint(app, color)),
            Span::raw(format!(" {}", theme::run_result(run))),
        ]);
        Row::new(
            [
                Cell::from(run.id.to_string()),
                Cell::from(started),
                Cell::from(duration),
                Cell::from(result),
            ]
            .into_iter()
            .skip(skip),
        )
    });
    let widths = [
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Fill(1),
    ]
    .into_iter()
    .skip(skip);
    let focused = app.focus == Panel::Main;
    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(chrome::highlight(focused, &palette))
        .highlight_symbol("› ");
    // Local widget state built from `App`: the render stays a pure function of the app.
    let mut state = TableState::default().with_selected(runs.selected_index());
    frame.render_stateful_widget(table, area, &mut state);
}

/// One run in full: its fields, then its tasks.
fn run_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let palette = theme::palette(app);
    let run = match &app.run_detail {
        Load::Failed(error) => {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return;
        }
        Load::Loaded(run) => run,
        Load::Idle | Load::Loading => {
            let text = app
                .viewing_run
                .map_or_else(String::new, |id| format!("Loading run {id}…"));
            frame.render_widget(Paragraph::new(text).block(block), area);
            return;
        }
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let errors = task_errors(app, run, &palette);
    // With failures to show, the task table gives up the lower half to them.
    let tasks_height = if errors.is_empty() {
        Constraint::Fill(1)
    } else {
        let rows = u16::try_from(run.tasks.len().saturating_add(1)).unwrap_or(u16::MAX);
        Constraint::Length(rows.min(inner.height / 2))
    };
    let [fields_area, tasks_area, errors_area] =
        Layout::vertical([Constraint::Length(7), tasks_height, Constraint::Fill(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(errors).wrap(Wrap { trim: false }),
        errors_area,
    );
    let dash = || "-".to_owned();
    let (glyph, color) = theme::run_glyph(run);
    let result = Line::from(vec![
        Span::styled(format!("{:<15}", "Result"), theme::dim(app)),
        Span::styled(glyph.to_string(), theme::tint(app, color)),
        Span::raw(format!(" {}", theme::run_result(run))),
    ]);
    let fields = vec![
        field(app, "Run ID", run.id.to_string()),
        field(
            app,
            "Started",
            run.start_time
                .map_or_else(dash, |ts| theme::clock(ts, &app.tz, &app.date_format)),
        ),
        field(
            app,
            "Duration",
            theme::run_duration(run, app.now).map_or_else(dash, theme::duration),
        ),
        result,
        field(
            app,
            "Message",
            if run.state.state_message.is_empty() {
                dash()
            } else {
                run.state.state_message.clone()
            },
        ),
        field(
            app,
            "URL",
            if run.page_url.is_empty() {
                dash()
            } else {
                run.page_url.clone()
            },
        ),
    ];
    frame.render_widget(Paragraph::new(fields), fields_area);
    let header = Row::new(["Task", "Started", "Duration", "Result"])
        .style(Style::new().add_modifier(Modifier::BOLD));
    let rows = run.tasks.iter().map(|task| {
        let (glyph, color) = theme::state_glyph(&task.state);
        let started = task
            .start_time
            .map_or_else(dash, |ts| theme::clock(ts, &app.tz, &app.date_format));
        let duration = theme::elapsed(task.start_time, task.end_time, app.now)
            .map_or_else(dash, theme::duration);
        Row::new([
            Cell::from(task.task_key.clone()),
            Cell::from(started),
            Cell::from(duration),
            Cell::from(Line::from(vec![
                Span::styled(glyph.to_string(), theme::tint(app, color)),
                Span::raw(format!(" {}", theme::state_result(&task.state))),
            ])),
        ])
    });
    let widths = [
        Constraint::Fill(1),
        Constraint::Length(12),
        Constraint::Length(9),
        Constraint::Length(12),
    ];
    frame.render_widget(Table::new(rows, widths).header(header), tasks_area);
}

/// Why each failed task failed: its state message, then the error and traceback from
/// `runs/get-output` as they arrive. Empty when every task succeeded.
// ponytail: no scrolling; the error line comes first and a long traceback is what `o` is for.
fn task_errors(app: &App, run: &Run, palette: &Palette) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for task in run.tasks.iter().filter(|task| task.state.is_failure()) {
        lines.push(Line::from(vec![
            Span::styled(
                task.task_key.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", task.state.state_message), theme::dim(app)),
        ]));
        match app.run_outputs.get(&task.run_id) {
            Some(Load::Loaded(output)) => {
                let error = Style::new().fg(palette.error);
                let dim = theme::dim(app);
                let text = |s: &Option<String>, style| {
                    s.iter()
                        .flat_map(|s| s.lines())
                        .map(|line| Line::styled(line.to_owned(), style))
                        .collect::<Vec<_>>()
                };
                let mut body = text(&output.error, error);
                body.extend(text(&output.error_trace, dim));
                if body.is_empty() {
                    body.push(Line::from("no error output"));
                }
                lines.extend(body);
            }
            Some(Load::Failed(error)) => {
                lines.push(Line::styled(
                    error.to_string(),
                    Style::new().fg(palette.error),
                ));
            }
            Some(Load::Loading | Load::Idle) | None => {
                lines.push(Line::from(format!(
                    "{} fetching output…",
                    app.spinner_glyph()
                )));
            }
        }
        lines.push(Line::default());
    }
    lines
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
        let created = update.creation_time.map_or_else(
            || "-".to_owned(),
            |ts| theme::clock(ts, &app.tz, &app.date_format),
        );
        // The first block of the UUID is enough to tell updates apart on screen.
        let short_id: String = update.id.chars().take(8).collect();
        Row::new([
            Cell::from(short_id),
            Cell::from(created),
            Cell::from(Line::from(vec![
                Span::styled(glyph.to_string(), theme::tint(app, color)),
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
        field(app, "Name", pipeline.name.clone()),
        field(app, "Pipeline ID", pipeline.id.clone()),
        field(app, "State", pipeline.state.as_str().to_owned()),
        field(app, "Creator", pipeline.creator_user_name.clone()),
        field(app, "Updates", pipeline.latest_updates.len().to_string()),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn cluster_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let Some(cluster) = app.compute.selected() else {
        frame.render_widget(block, area);
        return;
    };
    let dash = || "-".to_owned();
    let (glyph, color) = theme::cluster_glyph(cluster);
    let state = Line::from(vec![
        Span::styled(format!("{:<15}", "State"), theme::dim(app)),
        Span::styled(glyph.to_string(), theme::tint(app, color)),
        Span::raw(format!(" {}", cluster.state_label())),
    ]);
    let lines = vec![
        field(app, "Name", cluster.name.clone()),
        field(
            app,
            match cluster.kind {
                ComputeKind::Cluster => "Cluster ID",
                ComputeKind::Warehouse => "Warehouse ID",
            },
            cluster.id.clone(),
        ),
        state,
        field(
            app,
            "Message",
            if cluster.state_message.is_empty() {
                dash()
            } else {
                cluster.state_message.clone()
            },
        ),
        field(app, "Source", cluster.source.clone()),
        field(app, "Creator", cluster.creator_user_name.clone()),
        field(
            app,
            "Spark",
            if cluster.spark_version.is_empty() {
                dash()
            } else {
                cluster.spark_version.clone()
            },
        ),
        field(app, "Size", cluster.node_type_id.clone()),
        field(app, "Workers", cluster.workers()),
        field(
            app,
            "Started",
            cluster
                .start_time
                .map_or_else(dash, |ts| theme::clock(ts, &app.tz, &app.date_format)),
        ),
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
        field(app, "Name", settings.name.clone()),
        field(app, "Job ID", job.id.to_string()),
        field(app, "Creator", job.creator_user_name.clone()),
        field(app, "Run as", job.run_as_user_name.clone()),
        field(app, "Format", settings.format.clone().unwrap_or_else(dash)),
        field(
            app,
            "Max concurrent",
            settings
                .max_concurrent_runs
                .map_or_else(dash, |n| n.to_string()),
        ),
        field(
            app,
            "Timeout",
            settings.timeout_seconds.map_or_else(dash, |secs| {
                theme::duration(SignedDuration::from_secs(secs))
            }),
        ),
        field(app, "Tags", tags),
        field(
            app,
            "Schedule",
            settings.schedule.as_ref().map_or_else(dash, |schedule| {
                let paused = match schedule.pause_status.as_deref() {
                    Some("PAUSED") => " (paused)",
                    _ => "",
                };
                format!(
                    "{} {}{paused}",
                    schedule.quartz_cron_expression, schedule.timezone_id
                )
            }),
        ),
        field(
            app,
            "Deployment",
            settings
                .deployment
                .as_ref()
                .map_or_else(dash, |deployment| {
                    deployment.metadata_file_path.as_ref().map_or_else(
                        || deployment.kind.clone(),
                        |path| format!("{} {path}", deployment.kind),
                    )
                }),
        ),
        field(
            app,
            "Edit mode",
            settings.edit_mode.clone().unwrap_or_else(dash),
        ),
        Line::default(),
    ];
    let mut lines = lines;
    if app.detailed.contains(&job.id) {
        lines.push(Line::styled(
            format!("{:<24} {:<32} Cluster", "Task", "Type"),
            Style::new().add_modifier(Modifier::BOLD),
        ));
        lines.extend(settings.tasks.iter().map(|task| {
            Line::from(format!(
                "{:<24} {:<32} {}",
                chrome::fit(&task.task_key, 24).trim_end(),
                chrome::fit(&task.kind(), 32).trim_end(),
                task.cluster()
            ))
        }));
    } else {
        lines.push(Line::styled(
            format!("{} fetching tasks…", app.spinner_glyph()),
            theme::dim(app),
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn profile(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) {
    let lines = vec![
        field(app, "Profile", app.profile.clone()),
        field(app, "Host", app.host.clone()),
        field(app, "Jobs", app.jobs.items().len().to_string()),
        field(app, "Config", app.config_note.clone()),
    ];
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// One `Label   value` line. Values are owned because the frame outlives no borrow of `App`.
fn field(app: &App, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<15}"), theme::dim(app)),
        Span::raw(value),
    ])
}
