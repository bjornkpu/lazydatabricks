//! Rendering. A pure function of `&App`; nothing here mutates state.
//!
//! This file splits the frame and dispatches; the panels draw themselves in submodules.

mod apilog;
mod chrome;
mod help;
mod hints;
mod main_panel;
mod menu;
mod popup;
mod side;
mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, Panel, ScreenMode, SideLayout};

/// Rows for the API log under the main panel.
const API_LOG_HEIGHT: u16 = 7;
/// Height shares for the side panel in context against one each for the rest, when
/// `expand_focused` is on. lazygit's `expandedSidePanelWeight` default.
const EXPANDED_WEIGHT: u16 = 2;

/// The focused panel's rows as plain text, one line each, with words for glyphs: what `Y` puts
/// on the clipboard. Teams and Slack render words; they mangle `◐`.
#[must_use]
pub fn visible_text(app: &App) -> String {
    let age = |ts: Option<jiff::Timestamp>| match (ts, app.now) {
        (Some(ts), Some(now)) => theme::age_short(now.duration_since(ts)),
        _ => String::new(),
    };
    let lines: Vec<String> = match app.focus {
        Panel::Status => {
            let counts = app.counts();
            vec![
                format!("{} {}", app.profile, app.filter_summary()),
                format!(
                    "{} running · {} failed · {} compute up",
                    counts.running, counts.failed, counts.compute_up
                ),
            ]
        }
        Panel::Jobs => app
            .jobs
            .items()
            .iter()
            .map(|job| {
                let latest = app.latest_runs.get(&job.id);
                let result = latest.map_or("-", theme::run_result);
                format!(
                    "{:>3} {result} {}",
                    age(latest.and_then(|run| run.start_time)),
                    job.settings.name
                )
            })
            .collect(),
        Panel::Pipelines => app
            .pipelines
            .items()
            .iter()
            .map(|pipeline| {
                let latest = pipeline.latest_updates.first();
                let state =
                    latest.map_or_else(|| pipeline.state.as_str(), |update| update.state.as_str());
                format!(
                    "{:>3} {state} {}",
                    age(latest.and_then(|update| update.creation_time)),
                    pipeline.name
                )
            })
            .collect(),
        Panel::Compute => app
            .compute
            .items()
            .iter()
            .map(|cluster| format!("{} {}", cluster.state_label(), cluster.name))
            .collect(),
        Panel::Main => match &app.runs {
            crate::app::Load::Loaded(runs) if app.active_tab() == Some(crate::app::Tab::Runs) => {
                runs.items()
                    .iter()
                    .map(|run| {
                        let started = run.start_time.map_or_else(
                            || "-".to_owned(),
                            |ts| theme::clock(ts, &app.tz, &app.date_format),
                        );
                        let duration = theme::run_duration(run, app.now)
                            .map_or_else(|| "-".to_owned(), theme::duration);
                        format!("{} {started} {duration} {}", run.id, theme::run_result(run))
                    })
                    .collect()
            }
            _ => Vec::new(),
        },
    };
    lines.join("\n")
}

/// What the draw learned that `App` cannot know without a terminal: how far the main panel's
/// text can scroll, and which of its lines match the search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Drawn {
    pub limit: usize,
    pub matches: Vec<usize>,
}

/// Draws the whole screen for the current state.
pub fn draw(app: &App, frame: &mut Frame) -> Drawn {
    let [body, hint_bar] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    let limit = if app.mode == ScreenMode::Full {
        draw_panel(app, app.focus, body, frame)
    } else {
        let side_width = if app.mode == ScreenMode::Half {
            Constraint::Ratio(1, 2)
        } else {
            Constraint::Ratio(1, 3)
        };
        // Portrait: the side column goes on top when the terminal looks taller than wide. A
        // cell is about twice as tall as it is wide, hence the factor.
        let [side, main] = if body.width < body.height.saturating_mul(2) {
            Layout::vertical([Constraint::Ratio(1, 2), Constraint::Fill(1)]).areas(body)
        } else {
            Layout::horizontal([side_width, Constraint::Fill(1)]).areas(body)
        };
        let collapse_unfocused = app.mode == ScreenMode::Half && app.focus.is_side();
        // The compute panel folds away in a workspace that has none.
        let panels: Vec<Panel> = Panel::SIDE
            .into_iter()
            .filter(|panel| *panel != Panel::Compute || app.show_compute())
            .collect();
        let constraints: Vec<Constraint> = panels
            .iter()
            .map(|panel| side_constraint(app, *panel, collapse_unfocused))
            .collect();
        let areas = Layout::vertical(constraints).split(side);
        for (panel, area) in panels.into_iter().zip(areas.iter()) {
            draw_panel(app, panel, *area, frame);
        }
        if app.show_api_log {
            let [main, log] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(API_LOG_HEIGHT)])
                    .areas(main);
            apilog::draw(app, log, frame);
            draw_panel(app, Panel::Main, main, frame)
        } else {
            draw_panel(app, Panel::Main, main, frame)
        }
    };
    hints::draw(app, hint_bar, frame);
    menu::draw(app, frame);
    popup::draw(app, frame);
    help::draw(app, frame);
    limit
}

/// Height of one side panel. Status is two lines of text; the lists share the rest, with the
/// one in context taking more when config says so. The context panel, not the focused one, so
/// the column holds still when focus moves to `[0]` and back.
fn side_constraint(app: &App, panel: Panel, collapse_unfocused: bool) -> Constraint {
    if collapse_unfocused {
        return if panel == app.focus {
            Constraint::Fill(1)
        } else {
            Constraint::Length(1)
        };
    }
    if panel == Panel::Status {
        // Identity, filter summary, counts, plus the border.
        return Constraint::Length(5);
    }
    if app.side_layout == SideLayout::Expand && panel == app.context {
        Constraint::Fill(EXPANDED_WEIGHT)
    } else {
        Constraint::Fill(1)
    }
}

/// Side panels never scroll as text, so only the main panel reports anything.
fn draw_panel(app: &App, panel: Panel, area: Rect, frame: &mut Frame) -> Drawn {
    match panel {
        Panel::Status => side::status(app, area, frame),
        Panel::Jobs => side::jobs(app, area, frame),
        Panel::Pipelines => side::pipelines(app, area, frame),
        Panel::Compute => side::compute(app, area, frame),
        Panel::Main => return main_panel::draw(app, area, frame),
    }
    Drawn::default()
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::api::models::ResultState;
    use crate::app::tests::{api_call, app, job, pipeline, run, task, theirs};
    use crate::app::{Key, Message};
    use crate::config::Theme;
    use crate::error::AppError;

    fn render(app: &App) -> String {
        render_at(app, 80, 24)
    }

    fn render_at(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                draw(app, frame);
            })
            .unwrap();
        terminal.backend().to_string()
    }

    fn loaded() -> App {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![
            job(1, "[someone] okonomi_gold"),
            job(2, "nightly_bronze_ingest"),
            job(3, "weekly_report"),
        ]));
        app.update(Message::ApiCalled(api_call(
            "/api/2.2/jobs/list?limit=25",
            Some(200),
            84,
        )));
        app
    }

    /// Jobs loaded and the first job's runs arrived.
    fn with_runs() -> App {
        let mut app = loaded();
        app.update(Message::RunsLoaded {
            job_id: 1,
            runs: vec![
                run(50_851_892_761_075, 1_788_257_300_000, 0, None),
                run(
                    50_851_892_761_073,
                    1_788_170_893_271,
                    1_788_170_965_431,
                    Some(ResultState::Success),
                ),
                run(
                    50_851_892_761_074,
                    1_788_084_493_271,
                    1_788_084_551_000,
                    Some(ResultState::Failed),
                ),
            ],
        });
        app.update(Message::ApiCalled(api_call(
            "/api/2.2/jobs/runs/list?job_id=1&limit=25",
            Some(200),
            131,
        )));
        // A fixed "now" two hours after the newest run, and one older run for job 2.
        app.update(Message::Clock(
            jiff::Timestamp::from_millisecond(1_788_264_500_000).unwrap(),
        ));
        let mut nightly = run(
            50_851_892_761_070,
            1_788_170_000_000,
            1_788_170_060_000,
            Some(ResultState::Failed),
        );
        nightly.job_id = 2;
        app.update(Message::RecentRunsLoaded(vec![nightly]));
        app
    }

    fn press(app: &mut App, keys: &str) {
        for key in keys.chars() {
            app.update(Message::Key(Key::Char(key)));
        }
    }

    #[test]
    fn loading_80x24() {
        let mut app = app();
        app.update(Message::Tick);
        app.update(Message::Tick);
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn no_jobs_80x24() {
        let mut app = app();
        app.update(Message::JobsLoaded(vec![]));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn runs_tab_80x24() {
        insta::assert_snapshot!(render(&with_runs()));
    }

    #[test]
    fn runs_loading_80x24() {
        let mut app = loaded();
        press(&mut app, "j");
        app.update(Message::Tick);
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn runs_error_80x24() {
        let mut app = loaded();
        app.update(Message::RunsFailed {
            job_id: 1,
            error: AppError::Http {
                status: 429,
                path: "/api/2.2/jobs/runs/list?job_id=1&limit=25".to_owned(),
                message: "REQUEST_LIMIT_EXCEEDED: Too many requests".to_owned(),
            },
        });
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn detail_tab_80x24() {
        let mut app = with_runs();
        press(&mut app, "l");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn profile_tab_status_focused_80x24() {
        let mut app = with_runs();
        press(&mut app, "1");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn main_focused_log_hidden_80x24() {
        let mut app = with_runs();
        press(&mut app, "0@");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn portrait_50x40() {
        let app = with_runs();
        insta::assert_snapshot!(render_at(&app, 50, 40));
    }

    #[test]
    fn paused_job_row_80x24() {
        let mut app = with_runs();
        let mut paused = job(2, "nightly_bronze_ingest");
        paused.settings.schedule = Some(crate::api::models::CronSchedule {
            quartz_cron_expression: "0 0 4 * * ?".to_owned(),
            timezone_id: "Europe/Oslo".to_owned(),
            pause_status: Some("PAUSED".to_owned()),
        });
        app.update(Message::JobLoaded(paused));
        press(&mut app, "jx");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn even_side_panels_80x24() {
        let mut app = with_runs();
        app.side_layout = SideLayout::Even;
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn half_mode_jobs_focused_80x24() {
        let mut app = with_runs();
        press(&mut app, "+");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn full_mode_main_focused_80x24() {
        let mut app = with_runs();
        press(&mut app, "0++");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn api_log_truncates_long_paths_80x24() {
        let mut app = with_runs();
        app.update(Message::ApiCalled(api_call(
            "/api/2.2/jobs/runs/list?job_id=1025322370191789&limit=25&page_token=CAIo0JeenYM0Sg80MzEwMTU5NzM2MjUyMDA=",
            Some(500),
            12_345,
        )));
        app.update(Message::ApiCalled(api_call(
            "/api/2.2/jobs/list",
            None,
            30_000,
        )));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn filtering_80x24() {
        let mut app = with_runs();
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        press(&mut app, "/gol");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn mine_only_80x24() {
        let mut app = with_runs();
        app.update(Message::MeLoaded("someone@example.com".to_owned()));
        let mut all = app.all_jobs.clone();
        all.push(theirs(4, "[other] aktorer_ingest"));
        app.update(Message::JobsLoaded(all));
        press(&mut app, "m");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn me_failed_80x24() {
        let mut app = with_runs();
        app.update(Message::MeFailed(AppError::Unauthorized {
            status: 401,
            path: "/api/2.0/preview/scim/v2/Me".to_owned(),
            profile: "dev".to_owned(),
        }));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn menu_80x24() {
        let mut app = with_runs();
        press(&mut app, "x");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn confirm_80x24() {
        let mut app = with_runs();
        app.allow_actions = true;
        press(&mut app, "x");
        app.update(Message::Key(Key::Enter));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn custom_command_menu_80x24() {
        let mut app = with_runs();
        app.custom = vec![crate::app::CustomCommand {
            name: "Job JSON".to_owned(),
            key: None,
            context: crate::app::Context::Jobs,
            command: "databricks jobs get {{job_id}}".to_owned(),
            output: crate::app::CommandOutput::Popup,
            confirm: false,
        }];
        press(&mut app, "xjj");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn command_output_80x24() {
        let mut app = with_runs();
        app.update(Message::ShellFinished {
            name: "Job JSON".to_owned(),
            output: Ok(
                "{\n  \"job_id\": 1,\n  \"settings\": {\n    \"name\": \"okonomi_gold\"\n  }\n}\n"
                    .to_owned(),
            ),
        });
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn update_available_80x24() {
        let mut app = with_runs();
        app.update(Message::UpdateChecked(Ok("9.9.9".to_owned())));
        press(&mut app, "1");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn compare_runs_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        app.update(Message::Key(Key::Enter));
        let earlier: crate::api::models::Run =
            serde_json::from_str(include_str!("../../tests/fixtures/run_get.json")).unwrap();
        app.update(Message::RunDetailLoaded(earlier.clone()));
        press(&mut app, "W");
        app.update(Message::Key(Key::Esc));
        press(&mut app, "j");
        app.update(Message::Key(Key::Enter));
        let mut later = earlier;
        later.id = 50_851_892_761_074;
        later.state.result_state = Some(ResultState::Failed);
        later.end_time = later
            .end_time
            .map(|end| end + jiff::SignedDuration::from_secs(95));
        later.tasks[1].state.result_state = Some(ResultState::Failed);
        later.tasks[1].end_time = later.tasks[1]
            .end_time
            .map(|end| end + jiff::SignedDuration::from_secs(95));
        app.update(Message::RunDetailLoaded(later));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn search_80x24() {
        let mut app = with_runs();
        let full: crate::api::models::Job =
            serde_json::from_str(include_str!("../../tests/fixtures/job_get.json")).unwrap();
        let mut full = full;
        full.id = 1;
        app.update(Message::JobLoaded(full));
        press(&mut app, "0ll/cron");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut drawn = Drawn::default();
        terminal.draw(|frame| drawn = draw(&app, frame)).unwrap();
        assert!(!drawn.matches.is_empty(), "the JSON has a cron line");
        app.update(Message::Matches(drawn.matches));
        app.update(Message::Key(Key::Enter));
        press(&mut app, "n");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn bulk_menu_80x24() {
        let mut app = with_runs();
        press(&mut app, "vjx");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn range_select_80x24() {
        let mut app = with_runs();
        press(&mut app, "vj");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn filter_menu_80x24() {
        let mut app = with_runs();
        press(&mut app, "F");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn name_replacements_80x24() {
        let mut app = with_runs();
        app.replacements = vec![("[someone] ".to_owned(), String::new())];
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn config_tab_80x24() {
        let mut app = with_runs();
        press(&mut app, "1l");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn prompt_80x24() {
        let mut app = with_runs();
        press(&mut app, ":jobs get {{job_id}} --output json");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn copy_menu_80x24() {
        let mut app = with_runs();
        press(&mut app, "0jy");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn profile_menu_80x24() {
        let mut app = with_runs();
        app.profiles = ["DEFAULT", "dev", "prod"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        press(&mut app, "p");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn params_prompt_80x24() {
        let mut app = with_runs();
        app.allow_actions = true;
        press(&mut app, "xj");
        app.update(Message::Key(Key::Enter));
        press(&mut app, "date=2026-09-01 mode=full");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn repair_menu_80x24() {
        let mut app = with_runs();
        press(&mut app, "0jjx");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn clusters_80x24() {
        let mut app = with_runs();
        let page: crate::api::models::ClustersList =
            serde_json::from_str(include_str!("../../tests/fixtures/clusters_list.json")).unwrap();
        app.update(Message::ComputeLoaded(page.clusters));
        press(&mut app, "4j");
        insta::assert_snapshot!(render(&app));
        press(&mut app, "x");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn compute_warehouses_80x24() {
        let mut app = with_runs();
        let page: crate::api::models::WarehousesList =
            serde_json::from_str(include_str!("../../tests/fixtures/warehouses_list.json"))
                .unwrap();
        app.update(Message::ComputeLoaded(
            page.warehouses.into_iter().map(Into::into).collect(),
        ));
        press(&mut app, "4j");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn compute_hidden_80x24() {
        let mut app = with_runs();
        app.update(Message::ComputeLoaded(vec![]));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn output_tab_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        app.update(Message::Key(Key::Enter));
        let detail: crate::api::models::Run =
            serde_json::from_str(include_str!("../../tests/fixtures/run_get.json")).unwrap();
        let first = detail.tasks[0].run_id;
        app.update(Message::RunDetailLoaded(detail));
        press(&mut app, "]]]");
        // The tab asks for the first task's output on the next tick; the reply lands after.
        app.update(Message::Tick);
        app.update(Message::RunOutputLoaded {
            run_id: first,
            output: crate::api::models::RunOutput {
                notebook_output: Some(crate::api::models::NotebookOutput {
                    result: Some("{\"rows\": 42, \"status\": \"ok\"}".to_owned()),
                    truncated: false,
                }),
                ..Default::default()
            },
        });
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn json_tab_80x24() {
        let mut app = with_runs();
        let full: crate::api::models::Job =
            serde_json::from_str(include_str!("../../tests/fixtures/job_get.json")).unwrap();
        let mut full = full;
        full.id = 1;
        app.update(Message::JobLoaded(full));
        press(&mut app, "ll");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn detail_tab_full_80x24() {
        let mut app = with_runs();
        press(&mut app, "l");
        let full: crate::api::models::Job =
            serde_json::from_str(include_str!("../../tests/fixtures/job_get.json")).unwrap();
        // The fixture's id matches job 1 in `loaded()` only by settings; align the id.
        let mut full = full;
        full.id = 1;
        app.update(Message::JobLoaded(full));
        press(&mut app, "0++");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn mono_theme_80x24() {
        let mut app = with_runs();
        app.theme = Theme::Mono;
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn visible_text_is_words_not_glyphs() {
        let mut app = with_runs();
        assert_eq!(
            visible_text(&app),
            " 2h RUNNING [someone] okonomi_gold\n 1d FAILED nightly_bronze_ingest\n    - weekly_report"
        );
        press(&mut app, "0");
        let runs = visible_text(&app);
        assert!(
            runs.starts_with("50851892761075 01.09 10:08 2h00m RUNNING\n"),
            "{runs}"
        );
        assert!(runs.ends_with("57s FAILED"), "{runs}");
        press(&mut app, "3");
        assert_eq!(visible_text(&app), "", "empty list, empty text");
    }

    #[test]
    fn confirm_actions_80x24() {
        let mut app = with_runs();
        press(&mut app, "A");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn read_only_notice_80x24() {
        let mut app = with_runs();
        press(&mut app, "x");
        app.update(Message::Key(Key::Enter));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn run_started_80x24() {
        let mut app = with_runs();
        app.update(Message::RunStarted {
            job_id: 1,
            run_id: 50_851_892_761_076,
        });
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn help_jobs_80x24() {
        let mut app = with_runs();
        press(&mut app, "?");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn help_main_80x24() {
        let mut app = with_runs();
        press(&mut app, "0?");
        insta::assert_snapshot!(render(&app));
    }

    /// Snapshots are text, so the theme is checked on the focused border's colour instead.
    #[test]
    fn light_theme_changes_the_accent() {
        let accent = |theme| {
            let mut app = with_runs();
            app.theme = theme;
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| {
                    draw(&app, frame);
                })
                .unwrap();
            // Top-left corner of the focused Jobs panel, just under the 5-row Status panel.
            terminal.backend().buffer().cell((0, 5)).unwrap().fg
        };
        assert_eq!(accent(Theme::Dark), ratatui::style::Color::Green);
        assert_eq!(accent(Theme::Light), ratatui::style::Color::Blue);
    }

    #[test]
    fn pipelines_focused_updates_tab_80x24() {
        let mut app = with_runs();
        app.update(Message::PipelinesLoaded(vec![
            pipeline(
                "2b8e9c4d-3f2a-4e1b-9c7d-6a5b4c3d2e1f",
                "[someone] felles_gold",
                "someone@example.com",
            ),
            pipeline(
                "0120d44b-406a-42a6-b072-5796077af583",
                "[other] hubspot",
                "other@example.com",
            ),
        ]));
        press(&mut app, "3");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn pipelines_detail_tab_80x24() {
        let mut app = with_runs();
        app.update(Message::PipelinesLoaded(vec![pipeline(
            "2b8e9c4d-3f2a-4e1b-9c7d-6a5b4c3d2e1f",
            "[someone] felles_gold",
            "someone@example.com",
        )]));
        press(&mut app, "3l");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn runs_cursor_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn run_detail_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        app.update(Message::Key(Key::Enter));
        let detail: crate::api::models::Run =
            serde_json::from_str(include_str!("../../tests/fixtures/run_get.json")).unwrap();
        app.update(Message::RunDetailLoaded(detail));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn run_detail_failed_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        app.update(Message::Key(Key::Enter));
        let mut detail: crate::api::models::Run =
            serde_json::from_str(include_str!("../../tests/fixtures/run_get.json")).unwrap();
        detail.state.result_state = Some(ResultState::Failed);
        detail.state.state_message = "Task endring_sluttdato_fakta failed".to_owned();
        detail.tasks[1] = task(
            detail.tasks[1].run_id,
            "endring_sluttdato_fakta",
            Some(ResultState::Failed),
        );
        let task_run_id = detail.tasks[1].run_id;
        app.update(Message::RunDetailLoaded(detail));
        insta::assert_snapshot!(render(&app));
        let output =
            serde_json::from_str(include_str!("../../tests/fixtures/run_output.json")).unwrap();
        app.update(Message::RunOutputLoaded {
            run_id: task_run_id,
            output,
        });
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn run_detail_scrolled_80x24() {
        let mut app = with_runs();
        press(&mut app, "0j");
        app.update(Message::Key(Key::Enter));
        let mut detail: crate::api::models::Run =
            serde_json::from_str(include_str!("../../tests/fixtures/run_get.json")).unwrap();
        detail.state.result_state = Some(ResultState::Failed);
        detail.tasks[1] = task(
            detail.tasks[1].run_id,
            "endring_sluttdato_fakta",
            Some(ResultState::Failed),
        );
        let task_run_id = detail.tasks[1].run_id;
        app.update(Message::RunDetailLoaded(detail));
        let output =
            serde_json::from_str(include_str!("../../tests/fixtures/run_output.json")).unwrap();
        app.update(Message::RunOutputLoaded {
            run_id: task_run_id,
            output,
        });
        press(&mut app, "G");
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut drawn = Drawn::default();
        terminal.draw(|frame| drawn = draw(&app, frame)).unwrap();
        assert!(drawn.limit > 0, "the trace does not fit");
        app.update(Message::ScrollLimit(drawn.limit));
        assert_eq!(app.main_scroll, drawn.limit);
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn pipelines_menu_80x24() {
        let mut app = with_runs();
        let mut running = pipeline(
            "3c9f0d5e-4a3b-4f2c-8d6e-7b6c5d4e3f2a",
            "[someone] aktorer_ingest",
            "someone@example.com",
        );
        running.state = crate::api::models::PipelineState::Running;
        app.update(Message::PipelinesLoaded(vec![running]));
        press(&mut app, "3x");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn failed_only_80x24() {
        let mut app = with_runs();
        press(&mut app, "f");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn stale_after_failed_refresh_80x24() {
        let mut app = with_runs();
        app.update(Message::JobsFailed(AppError::Timeout {
            path: "/api/2.2/jobs/list?limit=25".to_owned(),
        }));
        // The notice carries the full text until the next key; the border keeps the kind.
        app.update(Message::Key(Key::Char('k')));
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn error_80x24() {
        let mut app = app();
        app.update(Message::JobsFailed(AppError::Unauthorized {
            status: 401,
            path: "/api/2.2/jobs/list?limit=25".to_owned(),
            profile: "dev".to_owned(),
        }));
        insta::assert_snapshot!(render(&app));
    }
}
