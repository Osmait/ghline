//! The review queue: what has been written, and where it is going.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::diffline::app::{App, Pane};
use crate::diffline::model::State;
use crate::tui::theme;
use crate::tui::{Section, fill, hline, put, put_right, put_trunc};

pub(super) fn queue(buf: &mut Buffer, area: Rect, app: &mut App) {
    Section::new("REVIEW QUEUE")
        .count(app.comments.len())
        .focused(app.pane == Pane::Queue)
        .open(buf, area);

    // The footer names the target and what sending would do.
    let foot_y = area.bottom() - 2;
    hline(buf, area.x, foot_y - 1, area.width, theme::border_soft());
    let fs = Style::default().bg(theme::panel_alt());
    // A pending new agent wins over the running one: it is what sending would
    // actually reach, and a footer naming the other would be a lie at exactly
    // the moment it is being read.
    let (who, dot) = match (&app.new_kind, app.agent()) {
        (Some(kind), _) => (format!("a new {kind} · here"), theme::green()),
        (None, Some(a)) => (
            format!("{} · {}", a.kind, a.where_short()),
            match a.status {
                crate::shared::mux::AgentStatus::Working => theme::yellow(),
                crate::shared::mux::AgentStatus::Blocked => theme::red(),
                crate::shared::mux::AgentStatus::Idle | crate::shared::mux::AgentStatus::Done => {
                    theme::green()
                }
                crate::shared::mux::AgentStatus::Unknown => theme::dimmer(),
            },
        ),
        (None, None) => ("no agent — press a".into(), theme::dimmer()),
    };
    put(buf, area.x + 1, foot_y, area.right(), "●", fs.fg(dot));
    put_trunc(
        buf,
        area.x + 3,
        foot_y,
        area.right() - 6,
        &who,
        fs.fg(theme::fg()),
    );
    put_right(buf, area.right() - 1, foot_y, "a", fs.fg(theme::dimmer()));

    let send = format!(" ⏎ S · send {} ", app.comments.len());
    let ready = !app.comments.is_empty();
    put(
        buf,
        area.x + 1,
        area.bottom() - 1,
        area.right(),
        &send,
        Style::default()
            .bg(if ready {
                theme::yellow()
            } else {
                theme::panel()
            })
            .fg(if ready {
                theme::panel()
            } else {
                theme::dimmer()
            }),
    );

    let list = Rect {
        y: area.y + 2,
        height: foot_y.saturating_sub(area.y + 3),
        ..area
    };

    if app.comments.is_empty() && app.replies.is_empty() {
        for (n, line) in [
            "No comments yet.",
            "",
            "Move to a line and press c.",
            "V first to take a range.",
        ]
        .iter()
        .enumerate()
        {
            put_trunc(
                buf,
                list.x + 2,
                list.y + n as u16,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::dimmer()),
            );
        }
        return;
    }

    let focused = app.pane == Pane::Queue;
    let mut y = list.y;
    for (i, c) in app.comments.iter().enumerate() {
        if y + 2 >= list.bottom() {
            break;
        }
        let sel = focused && i == app.queue_sel;
        let border = match c.state {
            State::Sending | State::Sent => theme::green(),
            State::Queued if sel => theme::yellow(),
            State::Queued => theme::border(),
        };
        let base = Style::default().bg(theme::bg());
        fill(
            buf,
            Rect {
                x: list.x,
                y,
                width: list.width,
                height: 3,
            },
            theme::bg(),
        );
        put(buf, list.x, y, area.right(), "▌", base.fg(border));

        put(
            buf,
            list.x + 2,
            y,
            area.right(),
            &format!("#{}", i + 1),
            base.fg(theme::yellow()),
        );
        let state = match c.state {
            State::Queued => "queued",
            State::Sending => "sending →",
            State::Sent => "sent",
        };
        let sx = put_right(buf, area.right() - 1, y, state, base.fg(border));
        put_trunc(
            buf,
            list.x + 6,
            y,
            sx.saturating_sub(1),
            &c.where_label(),
            base.fg(theme::dim()),
        );

        put_trunc(
            buf,
            list.x + 2,
            y + 1,
            area.right() - 1,
            &c.snippet,
            base.fg(theme::dimmer()),
        );
        put_trunc(
            buf,
            list.x + 2,
            y + 2,
            area.right() - 1,
            &c.body,
            base.fg(theme::fg()),
        );
        y += 4;
    }

    for reply in &app.replies {
        for line in reply.lines() {
            if y >= list.bottom() {
                return;
            }
            put_trunc(
                buf,
                list.x + 2,
                y,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::green()),
            );
            y += 1;
        }
        y += 1;
    }
}
