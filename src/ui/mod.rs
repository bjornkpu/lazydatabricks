//! Rendering. A pure function of `&App`; nothing here mutates state.

use ratatui::Frame;
use ratatui::widgets::{Block, List};

use crate::app::App;

/// Draws the whole screen for the current state.
pub fn draw(app: &App, frame: &mut Frame) {
    let names = app.jobs.iter().map(|job| job.settings.name.as_str());
    let list = List::new(names).block(Block::bordered().title(" Jobs "));
    frame.render_widget(list, frame.area());
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::Message;
    use crate::app::tests::job;

    fn render(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw(app, frame)).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn empty_80x24() {
        insta::assert_snapshot!(render(&App::default()));
    }

    #[test]
    fn jobs_80x24() {
        let mut app = App::default();
        app.update(Message::JobsLoaded(vec![
            job(1, "[someone] okonomi_gold"),
            job(2, "nightly_bronze_ingest"),
            job(3, "weekly_report"),
        ]));
        insta::assert_snapshot!(render(&app));
    }
}
