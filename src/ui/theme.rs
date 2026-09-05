//! Glyphs, colours and time formatting shared by the panels.

use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp};
use ratatui::style::{Color, Modifier, Style};

use crate::api::models::{
    LifeCycleState, Pipeline, PipelineState, ResultState, Run, RunState, UpdateState,
};
use crate::app::App;
use crate::config::Theme;

/// Chrome colours for one theme. Status glyphs keep their semantic colours regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Focused borders and titles.
    pub accent: Color,
    /// The selected row in the focused panel.
    pub highlight: Style,
    /// The selected row elsewhere, so the cursor never vanishes.
    pub highlight_unfocused: Style,
    pub error: Color,
    /// Notices and the actions menu.
    pub notice: Color,
    /// The confirmation box.
    pub danger: Color,
}

#[must_use]
pub const fn palette(app: &App) -> Palette {
    match app.theme {
        Theme::Dark => Palette {
            accent: Color::Green,
            highlight: Style::new()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            highlight_unfocused: Style::new().bg(Color::DarkGray),
            error: Color::Red,
            notice: Color::Yellow,
            danger: Color::Red,
        },
        Theme::Light => Palette {
            accent: Color::Blue,
            highlight: Style::new()
                .bg(Color::LightBlue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            highlight_unfocused: Style::new().bg(Color::Gray).fg(Color::Black),
            error: Color::LightRed,
            notice: Color::Magenta,
            danger: Color::LightRed,
        },
    }
}

/// Status as a glyph, not a word: colour carries the state, the glyph makes it work without.
#[must_use]
pub const fn run_glyph(run: &Run) -> (char, Color) {
    state_glyph(&run.state)
}

#[must_use]
pub const fn state_glyph(state: &RunState) -> (char, Color) {
    match (state.life_cycle_state, state.result_state) {
        (_, Some(ResultState::Success)) => ('✓', Color::Green),
        (_, Some(_)) => ('✗', Color::Red),
        (
            LifeCycleState::Terminated
            | LifeCycleState::Skipped
            | LifeCycleState::InternalError
            | LifeCycleState::Unknown,
            None,
        ) => ('?', Color::DarkGray),
        (_, None) => ('◐', Color::Yellow),
    }
}

/// A pipeline's health at a glance: its latest update when it has one, else its own state.
#[must_use]
pub fn pipeline_glyph(pipeline: &Pipeline) -> (char, Color) {
    match pipeline.latest_updates.first().map(|update| update.state) {
        Some(UpdateState::Completed) => ('✓', Color::Green),
        Some(UpdateState::Failed | UpdateState::Canceled) => ('✗', Color::Red),
        Some(_) => ('◐', Color::Yellow),
        None => match pipeline.state {
            PipelineState::Failed => ('✗', Color::Red),
            PipelineState::Idle | PipelineState::Deleted | PipelineState::Unknown => {
                ('·', Color::DarkGray)
            }
            _ => ('◐', Color::Yellow),
        },
    }
}

/// The result if there is one, else where the run is in its life cycle.
#[must_use]
pub const fn run_result(run: &Run) -> &'static str {
    state_result(&run.state)
}

#[must_use]
pub const fn state_result(state: &RunState) -> &'static str {
    match state.result_state {
        Some(result) => result.as_str(),
        None => state.life_cycle_state.as_str(),
    }
}

/// Wall-clock start to end, when both are known.
#[must_use]
pub fn run_duration(run: &Run) -> Option<SignedDuration> {
    span(run.start_time, run.end_time)
}

#[must_use]
pub fn span(start: Option<Timestamp>, end: Option<Timestamp>) -> Option<SignedDuration> {
    Some(end?.duration_since(start?))
}

/// `ts` in `tz` as `format` says (`%d.%m %H:%M` by default). Absolute times belong in tables;
/// ages belong in side lists. The format was validated at config load, so a failure here is
/// a bug, and shows as one rather than a panic.
#[must_use]
pub fn clock(ts: Timestamp, tz: &TimeZone, format: &str) -> String {
    // TimeZone is an Arc inside; the clone is a refcount bump.
    jiff::fmt::strtime::format(format, &ts.to_zoned(tz.clone()))
        .unwrap_or_else(|_| "bad date_format".to_owned())
}

/// One number and one letter, for the leftmost column of a list: `2m`, `4h`, `1d`, `1w`, `3M`.
/// Under a minute is `now`; a start in the future (clock skew) reads as `now` too.
#[must_use]
pub fn age_short(d: SignedDuration) -> String {
    let secs = u64::try_from(d.as_secs()).unwrap_or(0);
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        86_400..604_800 => format!("{}d", secs / 86_400),
        604_800..2_592_000 => format!("{}w", secs / 604_800),
        _ => format!("{}M", secs / 2_592_000),
    }
}

/// `12s ago`, `3m05s ago`: how old the data on screen is.
#[must_use]
pub fn age(d: std::time::Duration) -> String {
    let d = SignedDuration::try_from(d).unwrap_or(SignedDuration::MAX);
    format!("{} ago", duration(d))
}

/// Compact duration: `58s`, `1m12s`, `2h05m`, `3d01h`. Negative durations read as `0s`.
#[must_use]
pub fn duration(d: SignedDuration) -> String {
    let secs = u64::try_from(d.as_secs()).unwrap_or(0);
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m{:02}s", secs / 60, secs % 60),
        3600..86_400 => format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60),
        _ => format!("{}d{:02}h", secs / 86_400, (secs % 86_400) / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::run;

    #[test]
    fn durations_are_compact() {
        assert_eq!(duration(SignedDuration::from_secs(32)), "32s");
        assert_eq!(duration(SignedDuration::from_secs(72)), "1m12s");
        assert_eq!(duration(SignedDuration::from_secs(3725)), "1h02m");
        assert_eq!(duration(SignedDuration::from_secs(90_000)), "1d01h");
        assert_eq!(duration(SignedDuration::from_secs(-5)), "0s");
    }

    #[test]
    fn short_ages_are_one_unit() {
        let s = SignedDuration::from_secs;
        assert_eq!(age_short(s(5)), "now");
        assert_eq!(age_short(s(150)), "2m");
        assert_eq!(age_short(s(4 * 3600 + 59)), "4h");
        assert_eq!(age_short(s(86_400)), "1d");
        assert_eq!(age_short(s(8 * 86_400)), "1w");
        assert_eq!(age_short(s(70 * 86_400)), "2M");
        assert_eq!(age_short(s(-30)), "now");
    }

    #[test]
    fn age_reads_naturally() {
        assert_eq!(age(std::time::Duration::from_millis(2500)), "2s ago");
        assert_eq!(age(std::time::Duration::from_secs(125)), "2m05s ago");
    }

    #[test]
    fn clock_uses_given_zone() {
        let ts = Timestamp::from_millisecond(1_788_170_893_271).unwrap();
        assert_eq!(clock(ts, &TimeZone::UTC, "%d.%m %H:%M"), "31.08 10:08");
        let oslo = TimeZone::get("Europe/Oslo").unwrap();
        assert_eq!(clock(ts, &oslo, "%Y-%m-%d %H:%M"), "2026-08-31 12:08");
        assert_eq!(clock(ts, &oslo, "%!"), "bad date_format");
    }

    #[test]
    fn glyphs_follow_state() {
        assert_eq!(run_glyph(&run(1, 1, 2, Some(ResultState::Success))).0, '✓');
        assert_eq!(run_glyph(&run(1, 1, 2, Some(ResultState::Canceled))).0, '✗');
        assert_eq!(run_glyph(&run(1, 1, 0, None)).0, '◐');
        assert_eq!(run_glyph(&run(1, 1, 2, None)).0, '?');
        assert_eq!(run_result(&run(1, 1, 0, None)), "RUNNING");
        assert_eq!(
            run_result(&run(1, 1, 2, Some(ResultState::Failed))),
            "FAILED"
        );
    }

    #[test]
    fn pipeline_glyph_prefers_the_latest_update() {
        let mut pipeline = crate::app::tests::pipeline("p", "x", "me@example.com");
        assert_eq!(pipeline_glyph(&pipeline).0, '✓');
        pipeline.latest_updates[0].state = UpdateState::Running;
        assert_eq!(pipeline_glyph(&pipeline).0, '◐');
        pipeline.latest_updates.clear();
        assert_eq!(pipeline_glyph(&pipeline).0, '·');
        pipeline.state = PipelineState::Failed;
        assert_eq!(pipeline_glyph(&pipeline).0, '✗');
    }

    #[test]
    fn running_run_has_no_duration() {
        assert_eq!(run_duration(&run(1, 1000, 0, None)), None);
        assert_eq!(
            run_duration(&run(1, 1000, 73_000, None)),
            Some(SignedDuration::from_secs(72))
        );
    }
}
