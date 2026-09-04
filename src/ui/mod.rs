//! Rendering. A pure function of `&App`; nothing here mutates state.

use ratatui::Frame;
use ratatui::widgets::Block;

use crate::app::App;

/// Draws the whole screen for the current state.
pub fn draw(_app: &App, frame: &mut Frame) {
    let block = Block::bordered().title(" lazydatabricks ");
    frame.render_widget(block, frame.area());
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn empty_box_80x24() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let app = App::default();
        terminal.draw(|frame| draw(&app, frame)).unwrap();
        insta::assert_snapshot!(terminal.backend());
    }
}
