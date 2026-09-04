//! Rendering. A pure function of `&App`; nothing here mutates state.
//!
//! This file splits the frame and dispatches; the panels draw themselves in submodules.

mod chrome;
mod hints;
mod side;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;

use crate::app::{App, Panel, ScreenMode};

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
        draw_panel(app, Panel::Main, main, frame);
    }
    hints::draw(app.focus, hint_bar, frame);
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
        Panel::Status => Constraint::Length(3),
        Panel::Jobs | Panel::Pipelines | Panel::Main => Constraint::Fill(1),
    }
}

fn draw_panel(app: &App, panel: Panel, area: Rect, frame: &mut Frame) {
    match panel {
        Panel::Status => side::status(app, area, frame),
        Panel::Jobs => side::jobs(app, area, frame),
        Panel::Pipelines => side::pipelines(app, area, frame),
        Panel::Main => main_placeholder(app, area, frame),
    }
}

/// `[0]` until M4 gives it tabs: shows what is selected.
fn main_placeholder(app: &App, area: Rect, frame: &mut Frame) {
    let block = chrome::panel(Panel::Main, app.focus == Panel::Main, "", None);
    let text = app.jobs.selected().map_or_else(String::new, |job| {
        format!("{}\njob_id {}", job.settings.name, job.job_id)
    });
    frame.render_widget(Paragraph::new(text).block(block), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::tests::{app, job};
    use crate::app::{Key, Message};

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
    fn jobs_focused_80x24() {
        let mut app = loaded();
        press(&mut app, "j");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn status_focused_80x24() {
        let mut app = loaded();
        press(&mut app, "1");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn main_focused_80x24() {
        let mut app = loaded();
        press(&mut app, "0");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn half_mode_jobs_focused_80x24() {
        let mut app = loaded();
        press(&mut app, "+");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn full_mode_jobs_focused_80x24() {
        let mut app = loaded();
        press(&mut app, "++");
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn error_80x24() {
        let mut app = app();
        app.update(Message::JobsFailed(
            "HTTP status client error (403 Forbidden) for url (https://adb-1.azuredatabricks.net/api/2.2/jobs/list?limit=25)".to_owned(),
        ));
        insta::assert_snapshot!(render(&app));
    }
}
