//! Rendering. A pure function of `&App`; nothing here mutates state.
//!
//! This file splits the frame and dispatches; the panels draw themselves in submodules.

mod apilog;
mod chrome;
mod help;
mod hints;
mod main_panel;
mod menu;
mod side;
mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, Panel, ScreenMode};

/// Rows for the API log under the main panel.
const API_LOG_HEIGHT: u16 = 7;

/// Draws the whole screen for the current state.
pub fn draw(app: &App, frame: &mut Frame) {
    let [body, hint_bar] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    if app.mode == ScreenMode::Full {
        draw_panel(app, app.focus, body, frame);
    } else {
        let side_width = if app.mode == ScreenMode::Half {
            Constraint::Ratio(1, 2)
        } else {
            Constraint::Ratio(1, 3)
        };
        let [side, main] = Layout::horizontal([side_width, Constraint::Fill(1)]).areas(body);
        let collapse_unfocused = app.mode == ScreenMode::Half && app.focus.is_side();
        let constraints =
            Panel::SIDE.map(|panel| side_constraint(panel, app.focus, collapse_unfocused));
        let areas: [Rect; 3] = Layout::vertical(constraints).areas(side);
        for (panel, area) in Panel::SIDE.into_iter().zip(areas) {
            draw_panel(app, panel, area, frame);
        }
        if app.show_api_log {
            let [main, log] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(API_LOG_HEIGHT)])
                    .areas(main);
            draw_panel(app, Panel::Main, main, frame);
            apilog::draw(app, log, frame);
        } else {
            draw_panel(app, Panel::Main, main, frame);
        }
    }
    hints::draw(app, hint_bar, frame);
    menu::draw(app, frame);
    help::draw(app, frame);
}

/// Height of one side panel. Status is two lines of text; the lists share the rest.
fn side_constraint(panel: Panel, focus: Panel, collapse_unfocused: bool) -> Constraint {
    if collapse_unfocused {
        return if panel == focus {
            Constraint::Fill(1)
        } else {
            Constraint::Length(1)
        };
    }
    match panel {
        Panel::Status => Constraint::Length(4),
        Panel::Jobs | Panel::Pipelines | Panel::Main => Constraint::Fill(1),
    }
}

fn draw_panel(app: &App, panel: Panel, area: Rect, frame: &mut Frame) {
    match panel {
        Panel::Status => side::status(app, area, frame),
        Panel::Jobs => side::jobs(app, area, frame),
        Panel::Pipelines => side::pipelines(app, area, frame),
        Panel::Main => main_panel::draw(app, area, frame),
    }
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
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(app, frame)).unwrap();
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
            terminal.draw(|frame| draw(&app, frame)).unwrap();
            // Top-left corner of the focused Jobs panel, just under the 4-row Status panel.
            terminal.backend().buffer().cell((0, 4)).unwrap().fg
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
