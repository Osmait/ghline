//! Render. Each module reproduces one region of the design.

mod agents;
mod confirm;
mod detail;
mod diff;
mod dispatch;
mod explorer;
mod finder;
mod header;
mod list;
mod logs;
mod markdown;
pub mod overlay;
mod sidebar;
mod status;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::github::app::{App, View};
use crate::tui::theme;
use crate::tui::{fill, hline, put, vline};

// -------------------------------------------------------------------- layout

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    let buf = f.buffer_mut();
    fill(buf, area, theme::bg());

    // The mouse aims at what is on screen, so the regions are rebuilt from
    // scratch each frame: a pane not drawn this time is not there to click.
    // They are pushed in drawing order, which is what puts a modal in front.
    app.hits.clear();

    if area.height < 8 || area.width < 40 {
        put(
            buf,
            0,
            0,
            area.width,
            "terminal too small",
            Style::default().fg(theme::red()).bg(theme::bg()),
        );
        return;
    }

    // footer rows: border + status (+ border + command line)
    let footer_h: u16 = if app.cmd.is_some() { 4 } else { 2 };

    let header = Rect {
        x: 0,
        y: 0,
        width: area.width,
        height: 1,
    };
    let body = Rect {
        x: 0,
        y: 2,
        width: area.width,
        height: area.height - 2 - footer_h,
    };
    let footer = Rect {
        x: 0,
        y: area.height - footer_h,
        width: area.width,
        height: footer_h,
    };

    header::draw(buf, header, app);
    hline(buf, 0, 1, area.width, theme::border());

    let sidebar_w: u16 = 34;
    // logs and diff take the full width, as in the design; below 90 columns
    // there is not enough room for it whatever the reader asked for
    app.sidebar_shown =
        app.sidebar && !matches!(app.view, View::Logs | View::Diff) && area.width >= 90;
    if !app.sidebar_shown {
        draw_content(buf, body, app);
    } else {
        let side = Rect {
            x: 0,
            y: body.y,
            width: sidebar_w,
            height: body.height,
        };
        sidebar::draw(buf, side, app);
        vline(buf, sidebar_w, body.y, body.height, theme::border());
        let content = Rect {
            x: sidebar_w + 1,
            y: body.y,
            width: area.width - sidebar_w - 1,
            height: body.height,
        };
        draw_content(buf, content, app);
    }

    status::draw(buf, footer, app);

    if app.accounts_open {
        overlay::accounts(buf, area, app);
    }
    if app.finder_open {
        finder::draw(buf, area, app);
    }
    if app.dispatch_open {
        dispatch::draw(buf, area, app);
    }
    if app.themes_open {
        overlay::themes(buf, area, app);
    }
    if app.help_open {
        overlay::help(buf, area);
    }
    if let Some(prompt) = app.prompt.clone() {
        confirm::draw(buf, area, app, &prompt);
    }
}

fn draw_content(buf: &mut Buffer, area: Rect, app: &mut App) {
    if app.view == View::Logs {
        logs::draw(buf, area, app);
        return;
    }
    if app.view == View::Diff {
        diff::draw(buf, area, app);
        return;
    }

    // tab bar + its bottom border
    let tabs = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    list::tabs(buf, tabs, app);

    let inner = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(2),
    };

    match app.view {
        View::List if app.tab == crate::github::data::AGENTS_TAB => agents::draw(buf, inner, app),
        View::List if app.tab == crate::github::data::FILES_TAB => explorer::draw(buf, inner, app),
        View::List => list::draw(buf, inner, app),
        View::Detail => detail::draw(buf, inner, app),
        View::Diff | View::Logs => {}
    }
}

// --- what a state looks like -------------------------------------------
//
// Here rather than in `theme` because these are decisions about GitHub's
// vocabulary — a failing check is red, a merged pull request is purple —
// and `theme` is shared with a program that has neither. It kept them, and
// imported this program's `data` to do it.

use crate::github::data::{ReviewState, Status};
use ratatui::style::Color;

/// The design's `sc(status)`.
pub fn state_color(status: Status) -> Color {
    match status {
        Status::Success | Status::Open => theme::green(),
        Status::Failure => theme::red(),
        Status::Running => theme::yellow(),
        Status::Pending | Status::Skipped => theme::dimmer(),
        Status::Cancelled | Status::Draft => theme::dim(),
        Status::Closed | Status::Merged => theme::purple(),
        Status::Unknown => theme::fg(),
    }
}

/// The design's `si(status)`.
pub fn state_icon(status: Status) -> &'static str {
    match status {
        Status::Success => "✓",
        Status::Failure => "✗",
        Status::Running => "◐",
        Status::Pending => "○",
        Status::Skipped => "⊘",
        Status::Cancelled => "⊗",
        _ => "•",
    }
}

/// Colour and glyph for a review state. This is the view's decision, which is
/// why it lives here and not in the model.
pub fn review(state: ReviewState) -> (Color, &'static str) {
    match state {
        ReviewState::Approved => (theme::green(), "✓"),
        ReviewState::ChangesRequested => (theme::red(), "✗"),
        ReviewState::Dismissed => (theme::dim(), "⊘"),
        ReviewState::Commented => (theme::yellow(), "●"),
    }
}
