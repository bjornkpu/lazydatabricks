//! The API log: every REST call with method, path, status and duration. lazygit's command log.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::{chrome, theme};
use crate::app::App;

/// Columns other than the path: `GET ` + ` ` + path + ` ` + `200` + ` ` + `12345ms`.
const FIXED_WIDTH: usize = 4 + 1 + 1 + 3 + 1 + 7;

pub fn draw(app: &App, area: Rect, frame: &mut Frame) {
    let dim = theme::dim(app);
    let block = Block::bordered().title(Line::styled("─API log", dim));
    let inner = block.inner(area);
    let rows = usize::from(inner.height);
    let path_width = usize::from(inner.width).saturating_sub(FIXED_WIDTH);
    let lines: Vec<Line> = app
        .api_log
        .iter()
        .rev()
        .take(rows)
        .rev()
        .map(|call| {
            let (status, color) = match call.status {
                Some(status @ 200..300) => (status.to_string(), Color::Green),
                Some(status) => (status.to_string(), Color::Red),
                None => ("ERR".to_owned(), Color::Red),
            };
            Line::from(vec![
                Span::raw(format!(
                    "{:<4} {} ",
                    call.method,
                    chrome::fit(&call.path, path_width)
                )),
                Span::styled(format!("{status:>3}"), theme::tint(app, color)),
                Span::raw(format!(" {:>5}ms", call.duration.as_millis())),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
