//! `[0]`: tabs over whatever the context side panel has selected.

use jiff::SignedDuration;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState};

use super::theme::Palette;
use super::{Drawn, chrome, theme};
use crate::api::models::{ComputeKind, Run, TaskRun};
use crate::app::{App, InputMode, Load, Panel, Tab};

/// Inner width below which the runs table drops its Run ID column: ids plus dates plus a
/// result word need about this much.
const NARROW_RUNS_TABLE: u16 = 50;

/// Returns what the text view learned: how far it can scroll and which lines match the search.
/// Tables report nothing.
pub fn draw(app: &App, area: Rect, frame: &mut Frame) -> Drawn {
    let mut title = tabs_title(app);
    if let Some(run_id) = app.viewing_run {
        title.push_span(Span::raw(format!(" › run {run_id}")));
        if let Some(other) = app.compare.as_ref().filter(|other| other.id != run_id) {
            title.push_span(Span::styled(format!(" vs {}", other.id), theme::dim(app)));
        }
    }
    let searching = app.input == InputMode::Search;
    if searching || !app.search.is_empty() {
        let cursor = if searching { "▌" } else { "" };
        title.push_span(Span::styled(
            format!(" /{}{cursor}", app.search),
            theme::tint(app, ratatui::style::Color::Yellow),
        ));
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
            pipeline_detail(app, block, area, frame)
        }
        Some(Tab::Detail) if app.context == Panel::Compute => {
            cluster_detail(app, block, area, frame)
        }
        Some(Tab::Detail) => detail(app, block, area, frame),
        Some(Tab::Json) => json(app, block, area, frame),
        Some(Tab::Output) => output(app, block, area, frame),
        Some(Tab::Profile) => profile(app, block, area, frame),
        Some(Tab::Config) => config(app, block, area, frame),
        None => {
            frame.render_widget(block, area);
            Drawn::default()
        }
    }
}

/// Every task's output for the viewed run; task headers bold, the rest as printed.
fn output(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let lines = app
        .output_lines()
        .into_iter()
        .map(|line| {
            if line.starts_with("▸ ") {
                Line::styled(line, Style::new().add_modifier(Modifier::BOLD))
            } else {
                Line::raw(line)
            }
        })
        .collect();
    text_view(app, lines, block, area, frame)
}

/// The selection's settings as Databricks sent them, once fetched.
fn json(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    match app.json_view() {
        Load::Idle => {
            frame.render_widget(block, area);
            Drawn::default()
        }
        Load::Loading => text_view(
            app,
            vec![Line::styled(
                format!("{} fetching…", app.spinner_glyph()),
                theme::dim(app),
            )],
            block,
            area,
            frame,
        ),
        Load::Failed(error) => {
            frame.render_widget(chrome::error(&error, block, &theme::palette(app)), area);
            Drawn::default()
        }
        Load::Loaded(text) => text_view(
            app,
            text.lines()
                .map(|line| Line::raw(line.to_owned()))
                .collect(),
            block,
            area,
            frame,
        ),
    }
}

/// Renders lines with the main panel's scroll applied and the search's matches highlighted.
/// Returns how far it can scroll (the lines that do not fit) and which lines matched; `App`
/// learns both through messages, since it cannot see the terminal.
fn text_view(
    app: &App,
    mut lines: Vec<Line<'static>>,
    block: Block<'static>,
    area: Rect,
    frame: &mut Frame,
) -> Drawn {
    let height = usize::from(block.inner(area).height);
    let limit = lines.len().saturating_sub(height);
    let scroll = u16::try_from(app.main_scroll.min(limit)).unwrap_or(u16::MAX);
    let mut matches = Vec::new();
    if !app.search.is_empty() {
        let needle = app.search.to_lowercase();
        let mark = theme::palette(app).highlight_unfocused;
        for (index, line) in lines.iter_mut().enumerate() {
            if line.to_string().to_lowercase().contains(&needle) {
                matches.push(index);
                *line = std::mem::take(line).style(mark);
            }
        }
    }
    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
    Drawn { limit, matches }
}

/// Splits text into rows of at most `width` characters, so the line count the scroll clamps
/// to is exact. Tracebacks are ASCII; grapheme width is not a concern here.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in text.lines() {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            rows.push(String::new());
        }
        rows.extend(chars.chunks(width).map(|chunk| chunk.iter().collect()));
    }
    rows
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

fn runs(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    if app.viewing_run.is_some() {
        return run_detail(app, block, area, frame);
    }
    let palette = theme::palette(app);
    let runs = match &app.runs {
        Load::Failed(error) => {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return Drawn::default();
        }
        Load::Loaded(runs) => runs,
        Load::Idle | Load::Loading => {
            frame.render_widget(block, area);
            return Drawn::default();
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
    Drawn::default()
}

/// One run in full: its fields, its tasks, then why the failed ones failed. One scrolling text.
fn run_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let palette = theme::palette(app);
    let run = match &app.run_detail {
        Load::Failed(error) => {
            frame.render_widget(chrome::error(error, block, &palette), area);
            return Drawn::default();
        }
        Load::Loaded(run) => run,
        Load::Idle | Load::Loading => {
            let text = app
                .viewing_run
                .map_or_else(String::new, |id| format!("Loading run {id}…"));
            frame.render_widget(Paragraph::new(text).block(block), area);
            return Drawn::default();
        }
    };
    let width = usize::from(block.inner(area).width);
    let dash = || "-".to_owned();
    let (glyph, color) = theme::run_glyph(run);
    let result = Line::from(vec![
        Span::styled(format!("{:<15}", "Result"), theme::dim(app)),
        Span::styled(glyph.to_string(), theme::tint(app, color)),
        Span::raw(format!(" {}", theme::run_result(run))),
    ]);
    let mut lines = vec![
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
        Line::default(),
        Line::styled(
            format!("{:<16}{:<13}{:<10}Result", "Task", "Started", "Duration"),
            Style::new().add_modifier(Modifier::BOLD),
        ),
    ];
    lines.extend(run.tasks.iter().map(|task| {
        let (glyph, color) = theme::state_glyph(&task.state);
        let started = task
            .start_time
            .map_or_else(dash, |ts| theme::clock(ts, &app.tz, &app.date_format));
        let duration = theme::elapsed(task.start_time, task.end_time, app.now)
            .map_or_else(dash, theme::duration);
        Line::from(vec![
            Span::raw(format!(
                "{:<16}{started:<13}{duration:<10}",
                chrome::fit(&task.task_key, 15)
            )),
            Span::styled(glyph.to_string(), theme::tint(app, color)),
            Span::raw(format!(" {}", theme::state_result(&task.state))),
        ])
    }));
    if let Some(other) = app.compare.as_ref().filter(|other| other.id != run.id) {
        lines.push(Line::default());
        lines.extend(comparison(app, run, other));
    }
    let errors = task_errors(app, run, &palette, width);
    if !errors.is_empty() {
        lines.push(Line::default());
        lines.extend(errors);
        // Each task ends in a separator; `G` should land on text, not on it.
        lines.pop();
    }
    text_view(app, lines, block, area, frame)
}

/// lazygit's diffing mode, for runs: the marked run's result and duration against this one's,
/// then every task both runs have, with the earlier figure first. Answers "what changed since
/// yesterday" without two browser tabs.
fn comparison(app: &App, run: &Run, other: &Run) -> Vec<Line<'static>> {
    let dash = || "-".to_owned();
    let delta = match (
        theme::run_duration(other, app.now),
        theme::run_duration(run, app.now),
    ) {
        (Some(then), Some(now)) => {
            let diff = now.checked_sub(then).unwrap_or(SignedDuration::ZERO);
            let sign = if diff.is_negative() { "-" } else { "+" };
            format!("{sign}{}", theme::duration(diff.abs()))
        }
        _ => dash(),
    };
    let mut lines = vec![Line::styled(
        format!(
            "vs run {}  {}  {} then, {} now ({delta})",
            other.id,
            theme::run_result(other),
            theme::run_duration(other, app.now).map_or_else(dash, theme::duration),
            theme::run_duration(run, app.now).map_or_else(dash, theme::duration),
        ),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for task in &run.tasks {
        let Some(then) = other
            .tasks
            .iter()
            .find(|then| then.task_key == task.task_key)
        else {
            continue;
        };
        let elapsed = |task: &TaskRun| {
            theme::elapsed(task.start_time, task.end_time, app.now)
                .map_or_else(dash, theme::duration)
        };
        lines.push(Line::from(format!(
            "{:<16}{:>9} then {:>9} now   {} then, {} now",
            chrome::fit(&task.task_key, 15),
            elapsed(then),
            elapsed(task),
            theme::state_result(&then.state),
            theme::state_result(&task.state),
        )));
    }
    lines
}

/// Why each failed task failed: its state message, then the error and traceback from
/// `runs/get-output` as they arrive, wrapped to `width`. Empty when every task succeeded.
fn task_errors(app: &App, run: &Run, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for task in run.tasks.iter().filter(|task| task.state.is_failure()) {
        // Bold key, then the dim message flowing on from it and wrapping under it.
        let head = format!("{}  ", task.task_key);
        let mut message = wrap(
            &task.state.state_message,
            width.saturating_sub(head.chars().count()),
        )
        .into_iter();
        lines.push(Line::from(vec![
            Span::styled(head, Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(message.next().unwrap_or_default(), theme::dim(app)),
        ]));
        lines.extend(
            wrap(&message.collect::<Vec<_>>().join(" "), width)
                .into_iter()
                .filter(|row| !row.is_empty())
                .map(|row| Line::styled(row, theme::dim(app))),
        );
        match app.run_outputs.get(&task.run_id) {
            Some(Load::Loaded(output)) => {
                let error = Style::new().fg(palette.error);
                let dim = theme::dim(app);
                let text = |s: &Option<String>, style| {
                    s.iter()
                        .flat_map(|s| wrap(s, width))
                        .map(|row| Line::styled(row, style))
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
fn updates(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let Some(pipeline) = app.pipelines.selected() else {
        frame.render_widget(block, area);
        return Drawn::default();
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
    Drawn::default()
}

fn pipeline_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let Some(pipeline) = app.pipelines.selected() else {
        frame.render_widget(block, area);
        return Drawn::default();
    };
    let lines = vec![
        field(app, "Name", pipeline.name.clone()),
        field(app, "Pipeline ID", pipeline.id.clone()),
        field(app, "State", pipeline.state.as_str().to_owned()),
        field(app, "Creator", pipeline.creator_user_name.clone()),
        field(app, "Updates", pipeline.latest_updates.len().to_string()),
    ];
    text_view(app, lines, block, area, frame)
}

fn cluster_detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let Some(cluster) = app.compute.selected() else {
        frame.render_widget(block, area);
        return Drawn::default();
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
    text_view(app, lines, block, area, frame)
}

fn detail(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let Some(job) = app.jobs.selected() else {
        frame.render_widget(block, area);
        return Drawn::default();
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
    text_view(app, lines, block, area, frame)
}

fn profile(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let lines = vec![
        field(app, "Profile", app.profile.clone()),
        field(app, "Host", app.host.clone()),
        field(app, "Jobs", app.jobs.items().len().to_string()),
        field(app, "Config", app.config_note.clone()),
    ];
    text_view(app, lines, block, area, frame)
}

/// The effective configuration: where it came from, then the TOML this run is using.
fn config(app: &App, block: Block<'static>, area: Rect, frame: &mut Frame) -> Drawn {
    let mut lines = vec![
        Line::styled(format!("# {}", app.config_note), theme::dim(app)),
        Line::default(),
    ];
    lines.extend(
        app.config_text
            .lines()
            .map(|line| Line::raw(line.to_owned())),
    );
    text_view(app, lines, block, area, frame)
}

/// One `Label   value` line. Values are owned because the frame outlives no borrow of `App`.
fn field(app: &App, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<15}"), theme::dim(app)),
        Span::raw(value),
    ])
}
