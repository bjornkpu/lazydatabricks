//! Rendering. A pure function of `&App`; nothing here mutates state.

use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, List, Paragraph, Wrap};

use crate::app::{App, SPINNER};

/// Draws the whole screen for the current state.
pub fn draw(app: &App, frame: &mut Frame) {
    let title = if app.loading {
        let glyph = SPINNER.get(app.spinner).copied().unwrap_or(' ');
        format!(" Jobs {glyph} ")
    } else {
        " Jobs ".to_owned()
    };
    let block = Block::bordered().title(title);
    if let Some(error) = &app.error {
        let paragraph = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false })
            .block(block);
        frame.render_widget(paragraph, frame.area());
        return;
    }
    let names = app.jobs.iter().map(|job| job.settings.name.as_str());
    frame.render_widget(List::new(names).block(block), frame.area());
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
    fn loading_80x24() {
        let mut app = App::default();
        app.update(Message::Tick);
        app.update(Message::Tick);
        insta::assert_snapshot!(render(&app));
    }

    #[test]
    fn no_jobs_80x24() {
        let mut app = App::default();
        app.update(Message::JobsLoaded(vec![]));
        insta::assert_snapshot!(render(&app));
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

    #[test]
    fn error_80x24() {
        let mut app = App::default();
        app.update(Message::JobsFailed(
            "HTTP status client error (403 Forbidden) for url (https://adb-1.azuredatabricks.net/api/2.2/jobs/list?limit=25)".to_owned(),
        ));
        insta::assert_snapshot!(render(&app));
    }
}
