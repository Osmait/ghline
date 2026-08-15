//! The Agents tab: what herdr is running right now.
//!
//! Two rows per agent, like the issue and pull request lists, because the same
//! two things are worth knowing — what it is, and what it is doing. Unlike
//! those, this list is about the machine rather than about a repository, so it
//! stays populated when the repository selection moves.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::github::app::hit::{Region, Target};
use crate::github::app::{App, Pane};
use crate::tui::theme;
use crate::tui::{AgentRow, agent_row, fill, pct, put_trunc, scroll_into_view, skel_bar};

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let rows = (area.height / 2) as usize;
    let sel = app.agent_idx();
    let len = app.agents_visible().len();
    scroll_into_view(&mut app.agent_scroll, sel, rows, len);
    let scroll = app.agent_scroll;
    app.hits.push(Region::rows(
        Target::Pane(Pane::Agents),
        area,
        2,
        scroll,
        len,
    ));

    if len == 0 {
        let state = app.agents_state.clone();
        if state.is_loading() {
            let avail = area.width.saturating_sub(24);
            let names = [38, 52, 30];
            for row in 0..rows.min(3) {
                let y = area.y + (row as u16) * 2;
                skel_bar(buf, area.x + 2, y, 1, row, app.anim);
                skel_bar(buf, area.x + 4, y, 8, row, app.anim);
                skel_bar(
                    buf,
                    area.x + 14,
                    y,
                    pct(avail, names[row % names.len()]),
                    row,
                    app.anim,
                );
                skel_bar(buf, area.x + 14, y + 1, pct(avail, 44), row, app.anim);
            }
            return;
        }
        let (msg, color) = match state.error() {
            Some(e) => (e.to_string(), theme::red()),
            None if !app.live() => (
                "agents are a live-mode feature; herdr is not consulted in demo".to_string(),
                theme::dimmer(),
            ),
            None if !app.filter.is_empty() => ("no matches".to_string(), theme::dimmer()),
            None => (
                "no agents running · start one in herdr, or dispatch an issue with x".to_string(),
                theme::dimmer(),
            ),
        };
        put_trunc(
            buf,
            area.x + 3,
            area.y + 1,
            area.right() - 2,
            &msg,
            Style::default().bg(theme::bg()).fg(color),
        );
        return;
    }

    let agents: Vec<crate::shared::mux::Agent> =
        app.agents_visible().into_iter().cloned().collect();

    for (row, i) in (scroll..agents.len()).enumerate() {
        if row >= rows {
            break;
        }
        let y = area.y + (row as u16) * 2;
        let a = &agents[i];
        let selected = i == sel;
        let bg = if selected { theme::sel() } else { theme::bg() };
        fill(
            buf,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 2,
            },
            bg,
        );

        // Two marks, two meanings: the one this passes as `mark` is where
        // the cursor is, and the row's own status glyph is what the agent is
        // doing. The mark dims when the pane is not focused, so a selection
        // you left behind does not read as the one you are moving.
        let mark = selected.then(|| {
            if app.pane == Pane::Agents {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            }
        });
        // This program is in the list like anything else. Saying so is the
        // cheapest way to stop someone dispatching an issue to the agent that
        // is drawing the row they clicked.
        let title = if a.focused {
            format!("{}  (this window)", a.title)
        } else {
            a.title.clone()
        };
        agent_row(
            buf,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 2,
            },
            &AgentRow {
                kind: &a.kind,
                icon: &crate::shared::config::agent_icon(&a.kind),
                status: a.status,
                title: &title,
                detail: &a.cwd,
                trailing: &a.pane,
                selected,
                mark,
                ground: theme::bg(),
            },
        );
    }
}
