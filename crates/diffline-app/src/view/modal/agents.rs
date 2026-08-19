//! Choosing who the queue goes to.

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::style::Style;

use crate::app::App;
use crate::tui::theme;
use crate::tui::{
    AgentRow, agent_row, centered_over as centered, fill, frame, put, put_right, put_trunc, rule,
};

pub(crate) fn agents(buf: &mut Buffer, area: Rect, app: &App) {
    let kinds = app.agent_choices().len() - app.agents.len();
    let rows = app.agents.len() as u16 * 2 + kinds as u16 + 1;
    let h = (rows + 6).min(area.height.saturating_sub(4));
    let m = centered(area, Size::new(76, h.max(7)));
    frame(buf, m, theme::cyan());
    let base = Style::default().bg(theme::panel());

    put(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        "AGENTS ON THIS MACHINE",
        base.fg(theme::yellow()),
    );
    put_right(
        buf,
        m.right() - 2,
        m.y + 1,
        "via herdr",
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 2, theme::border());

    let mut y = m.y + 3;

    if app.agents.is_empty() {
        let msg = match app.agents_state.error() {
            Some(e) => e.to_string(),
            None if app.agents_state.is_loading() => "looking…".into(),
            None => "none running yet".into(),
        };
        put_trunc(
            buf,
            m.x + 3,
            y,
            m.right() - 2,
            &msg,
            base.fg(theme::dimmer()),
        );
        y += 2;
    }

    for (i, a) in app.agents.iter().enumerate() {
        if y + 1 >= m.bottom() - 1 {
            break;
        }
        // The mark is on the agent the queue would go to, which is not the
        // row the cursor is on: `sel` moves as you look, `agent_idx` is what
        // was chosen.
        let mark = (i == app.agent_idx && app.new_kind.is_none()).then(theme::yellow);
        // Why it cannot be used, where its directory would go: the directory
        // is what you read to tell agents apart, and the refusal is what you
        // read to find out why nothing is happening.
        let refusal = app.refusal(a);
        agent_row(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 2,
            },
            &AgentRow {
                kind: &a.kind,
                icon: &crate::shared::config::agent_icon(&a.kind),
                status: a.status.label(),
                title: "",
                detail: refusal.as_deref().unwrap_or(&a.cwd),
                trailing: "",
                selected: i == app.sel,
                mark,
                ground: theme::panel(),
            },
        );
        y += 2;
    }

    // Below the line, the ones that are not running: picking one of these
    // starts it in this repository when the queue is sent, so an agent is
    // never opened for a review that then does not happen.
    if y + 1 < m.bottom() - 1 {
        rule(buf, m, y, theme::border());
        put(
            buf,
            m.x + 3,
            y,
            m.right() - 2,
            " start a new one here ",
            base.fg(theme::dimmer()),
        );
        y += 1;
    }

    let running = app.agents.len();
    for (i, (kind, _)) in app.agent_choices().iter().enumerate().skip(running) {
        if y >= m.bottom() - 1 {
            break;
        }
        let bg = if i == app.sel {
            theme::sel()
        } else {
            theme::panel()
        };
        fill(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 1,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if app.new_kind.as_deref() == Some(kind.as_str()) {
            put(buf, m.x + 1, y, m.right(), "▌", s.fg(theme::yellow()));
        }
        put(buf, m.x + 3, y, m.right(), "+", s.fg(theme::green()));
        put(
            buf,
            m.x + 5,
            y,
            m.right(),
            &crate::shared::config::agent_icon(kind),
            s.fg(theme::purple()),
        );
        put_trunc(buf, m.x + 7, y, m.right() - 2, kind, s.fg(theme::bright()));
        y += 1;
    }
}
