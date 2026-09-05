//! `?`: the bindings that apply right now, read from the live keymap so overrides show.

use ratatui::Frame;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{chrome, theme};
use crate::app::{Action, App, InputMode, Keymap, Panel};

pub fn draw(app: &App, frame: &mut Frame) {
    let InputMode::Help { scroll } = app.input else {
        return;
    };
    let palette = theme::palette(app);
    let rows = bindings(app.focus, &app.keys);
    let key_width = rows
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let width = rows
        .iter()
        .map(|(_, d)| {
            key_width
                .saturating_add(2)
                .saturating_add(d.chars().count())
        })
        .max()
        .unwrap_or(0);
    let area = chrome::centered(
        chrome::columns(width),
        chrome::rows(rows.len()),
        frame.area(),
    );
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:>key_width$}  "),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(*description),
            ])
        })
        .collect();
    let panel_name = match app.focus {
        Panel::Main => "Main",
        other => other.name(),
    };
    // Rows that do not fit scroll with j/k; the scroll stops at the last row.
    let hidden = chrome::rows(rows.len()).saturating_sub(area.height);
    let footer = if hidden > 0 {
        "j/k: scroll │ Esc: close"
    } else {
        "Esc: close"
    };
    let block = Block::bordered()
        .border_type(palette.border)
        .border_style(Style::new().fg(palette.accent))
        .title(format!(" Keybindings ─ {panel_name} "))
        .title_bottom(Line::from(footer).centered());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll.min(hidden), 0))
            .block(block),
        area,
    );
}

/// The focused panel's bindings first, then the ones that work everywhere.
fn bindings(focus: Panel, keys: &Keymap) -> Vec<(String, &'static str)> {
    let k = |action| keys.labels(action);
    let mut rows = match focus {
        Panel::Jobs => vec![
            (k(Action::Down), "next job"),
            (k(Action::Up), "previous job"),
            (k(Action::PageDown), "ten jobs down"),
            (k(Action::PageUp), "ten jobs up"),
            (k(Action::First), "first job"),
            (k(Action::Last), "last job"),
            (k(Action::Filter), "filter by name"),
            (k(Action::MineOnly), "toggle mine only"),
            (k(Action::StatusFilter), "cycle status: all, failed, active"),
            (k(Action::Open), "focus the main panel"),
            (
                k(Action::Menu),
                "actions menu: run, pause schedule, repair, cancel",
            ),
            (k(Action::Refresh), "refresh jobs"),
            (k(Action::Browse), "open job in browser"),
            (k(Action::Copy), "copy: URL, job ID, name, JSON"),
            (k(Action::CopyTable), "copy the job list as text"),
            (k(Action::Sort), "cycle sort: activity, name, created"),
        ],
        Panel::Main => vec![
            (k(Action::Down), "next run, or scroll a line"),
            (k(Action::Up), "previous run, or scroll a line"),
            (k(Action::PageDown), "ten runs or lines down"),
            (k(Action::PageUp), "ten runs or lines up"),
            (
                k(Action::Open),
                "open the run; on JSON and Output, page the text",
            ),
            (k(Action::NextTab), "next tab"),
            (k(Action::PrevTab), "previous tab"),
            (k(Action::Back), "back: close the run, then the panel"),
            (k(Action::Menu), "actions menu"),
            (k(Action::Refresh), "refresh runs"),
            (k(Action::Browse), "open job in browser"),
            (k(Action::Copy), "copy: URL, run ID, job ID, name, JSON"),
            (k(Action::CopyTable), "copy the runs table as text"),
        ],
        Panel::Pipelines => vec![
            (k(Action::Down), "next pipeline"),
            (k(Action::Up), "previous pipeline"),
            (k(Action::PageDown), "ten pipelines down"),
            (k(Action::PageUp), "ten pipelines up"),
            (k(Action::Filter), "filter by name"),
            (k(Action::MineOnly), "toggle mine only"),
            (k(Action::StatusFilter), "cycle status: all, failed, active"),
            (k(Action::Open), "focus the main panel"),
            (k(Action::Menu), "actions menu"),
            (k(Action::Refresh), "refresh pipelines"),
            (k(Action::Browse), "open pipeline in browser"),
            (k(Action::Copy), "copy: URL, pipeline ID, name, JSON"),
            (k(Action::CopyTable), "copy the pipeline list as text"),
            (k(Action::Sort), "cycle sort: activity, name"),
        ],
        Panel::Compute => vec![
            (k(Action::Down), "next cluster or warehouse"),
            (k(Action::Up), "previous cluster or warehouse"),
            (k(Action::Filter), "filter by name or creator"),
            (k(Action::MineOnly), "toggle mine only"),
            (k(Action::StatusFilter), "cycle status: all, error, active"),
            (k(Action::Open), "focus the main panel"),
            (k(Action::Menu), "actions menu: start, terminate or stop"),
            (k(Action::Refresh), "refresh compute"),
            (k(Action::Browse), "open in browser"),
            (k(Action::Copy), "copy: URL, ID, name"),
            (k(Action::CopyTable), "copy the compute list as text"),
        ],
        Panel::Status => vec![(k(Action::Refresh), "refresh jobs, pipelines and compute")],
    };
    rows.extend([
        ("0-4".to_owned(), "focus panel by number"),
        (k(Action::NextPanel), "next side panel"),
        (k(Action::RefreshAll), "refresh everything"),
        (k(Action::ScreenMode), "cycle screen mode"),
        (k(Action::ToggleLog), "toggle the API log"),
        (
            k(Action::ToggleActions),
            "enable or disable actions this session",
        ),
        (
            k(Action::SwitchProfile),
            "switch profile: any in ~/.databrickscfg",
        ),
        (k(Action::Help), "this list"),
        (k(Action::Quit), "quit"),
    ]);
    rows
}
