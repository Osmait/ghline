//! The dispatch picker: where an issue goes.
//!
//! Two kinds of destination in one list — an agent already running, or a fresh
//! worktree with a new one. They are one list rather than two panes because
//! the choice is a single question, and because the answer is usually "that
//! one, the idle one".

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::github::app::hit::{Region, Target};
use crate::github::app::{App, Dest};
use crate::tui::scrim;
use crate::tui::theme;
use crate::tui::{centered, frame, rule};
use crate::tui::{fill, put, put_right, put_trunc, scroll_into_view};

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    scrim(buf, area);

    let dests = app.dispatch_dests();
    let width = area.width.saturating_sub(8).min(84);
    // Where the 2 and the 11 come from. Two lines per destination, and eight
    // of fixed furniture around them:
    //
    //     ┌────────────────────────┐  y      border
    //     │ SEND issue #217 …      │  y+1    header
    //     ├────────────────────────┤  y+2    rule
    //     │ ❯ the instruction      │  y+3    the note
    //     ├────────────────────────┤  y+4    rule
    //     │ ▌ claude   ~/work/tui  │ ─┐ the list: y+5 to bottom-4, and the
    //     │            2 comments  │ ─┘ only part that grows
    //     ├────────────────────────┤  bottom-3  rule
    //     │ type to add … · ⌂ …    │  bottom-2  footer
    //     └────────────────────────┘  bottom-1  border
    //
    // Which makes the list `height - 8`, so eight would fit exactly. The
    // three above that are slack: it leaves the list one row more than there
    // are destinations, and a list that cannot fill itself never scrolls. The
    // `min` is what takes it away again on a screen too short to hold it, and
    // that is the one case the scrolling below is for.
    let height = (dests.len() as u16 * 2 + 11).min(area.height.saturating_sub(4));
    let modal = centered(area, width, height);
    frame(buf, modal, theme::cyan());

    let base = Style::default().bg(theme::panel());
    let max = modal.right() - 2;

    // What is being sent, not just which row is selected: standing in a log
    // and standing in the list of runs send very different things.
    let kind = app
        .dispatch_subject()
        .map(crate::github::subject::Subject::label)
        .unwrap_or("nothing");
    let what = app
        .current()
        .map(|c| format!("#{}  {}", c.num, c.title))
        .unwrap_or_default();

    let mut cx = put(
        buf,
        modal.x + 2,
        modal.y + 1,
        max,
        "SEND",
        base.fg(theme::cyan()),
    );
    cx = put(
        buf,
        cx + 1,
        modal.y + 1,
        max,
        kind,
        base.fg(theme::yellow()),
    );
    put_trunc(
        buf,
        cx + 2,
        modal.y + 1,
        // enough for the hint on the right, which grew when the arrows
        // took over moving
        max.saturating_sub(22),
        &what,
        base.fg(theme::bright()),
    );
    put_right(
        buf,
        max,
        modal.y + 1,
        "↑↓ or ^n/^p · enter",
        base.fg(theme::dimmer()),
    );
    rule(buf, modal, modal.y + 2, theme::cyan());

    // The instruction line. Typing lands here rather than moving the
    // selection, which is why the arrows do the moving — the same bargain the
    // finder makes, and for the same reason.
    let ny = modal.y + 3;
    let nx = put(buf, modal.x + 2, ny, max, "❯ ", base.fg(theme::cyan()));
    if app.dispatch_note.is_empty() {
        put_trunc(
            buf,
            nx,
            ny,
            max,
            "say something specific, or leave it blank for the template",
            base.fg(theme::dimmer()),
        );
    } else {
        let end = put_trunc(
            buf,
            nx,
            ny,
            max,
            &app.dispatch_note,
            base.fg(theme::bright()),
        );
        if app.blink {
            put(buf, end, ny, max, "█", base.fg(theme::cyan()));
        }
    }
    rule(buf, modal, modal.y + 4, theme::cyan());

    let list = Rect {
        x: modal.x + 1,
        y: modal.y + 5,
        width: modal.width - 2,
        height: modal.bottom().saturating_sub(modal.y + 8),
    };
    let rows = (list.height / 2) as usize;

    if dests.is_empty() {
        put_trunc(
            buf,
            list.x + 2,
            list.y,
            max,
            "nowhere to send it — no agents running, and no local clone to branch from",
            base.fg(theme::dimmer()),
        );
    } else {
        scroll_into_view(
            &mut app.dispatch_scroll,
            app.dispatch_sel,
            rows,
            dests.len(),
        );
        app.hits.push(Region::rows(
            Target::Dispatch,
            list,
            2,
            app.dispatch_scroll,
            dests.len(),
        ));

        for (row, i) in (app.dispatch_scroll..dests.len()).enumerate() {
            if row >= rows {
                break;
            }
            let y = list.y + (row as u16) * 2;
            let d = &dests[i];
            let selected = i == app.dispatch_sel;
            let bg = if selected {
                theme::sel()
            } else {
                theme::panel()
            };
            fill(
                buf,
                Rect {
                    x: list.x,
                    y,
                    width: list.width,
                    height: 2,
                },
                bg,
            );
            let s = Style::default().bg(bg);
            if selected {
                put(buf, list.x, y, max, "▌", s.fg(theme::cyan()));
                put(buf, list.x, y + 1, max, "▌", s.fg(theme::cyan()));
            }

            // A destination that cannot take the issue is still shown, with
            // the reason: knowing an agent is busy is more useful than a list
            // that quietly omits it.
            let refusal = d.refusal();
            let (mark, mark_color) = match d {
                Dest::Running { .. } if refusal.is_some() => ("○", theme::dimmer()),
                Dest::Running { .. } => ("◉", theme::green()),
                // a different mark, because the two do different things to
                // the reader's disk
                Dest::Fresh {
                    in_place: Some(_), ..
                } => ("⌂", theme::yellow()),
                Dest::Fresh { .. } => ("+", theme::cyan_soft()),
                Dest::NotCloned(_) => ("⊘", theme::dimmer()),
            };
            put(buf, list.x + 2, y, max, mark, s.fg(mark_color));

            let fg = match (&refusal, selected) {
                (Some(_), _) => theme::dimmer(),
                (None, true) => theme::bright(),
                (None, false) => theme::fg(),
            };
            put_trunc(
                buf,
                list.x + 4,
                y,
                max,
                &d.title(),
                s.fg(fg).add_modifier(Modifier::BOLD),
            );
            let (detail, color) = match &refusal {
                Some(why) => (why.clone(), theme::yellow()),
                None => (d.detail(), theme::dimmer()),
            };
            put_trunc(buf, list.x + 4, y + 1, max, &detail, s.fg(color));
        }
    }

    // the frame's own border sits on `bottom() - 1`, so the footer goes above
    // it and its rule above that
    let foot_y = modal.bottom() - 2;
    rule(buf, modal, foot_y - 1, theme::cyan());
    put(
        buf,
        modal.x + 2,
        foot_y,
        max,
        "type to add an instruction · + worktree · ⌂ the checkout you have",
        base.fg(theme::dimmer()),
    );
}
