//! The line along the bottom: the mode, where you are, and the last thing
//! that happened.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::app::{App, Modal};
use crate::tui::theme;
use crate::tui::{fill, put, put_right, put_trunc};

pub(super) fn status_bar(buf: &mut Buffer, area: Rect, app: &App) {
    fill(buf, area, theme::panel());
    let base = Style::default().bg(theme::panel());

    let (mode, mode_bg) = match app.modal {
        Some(Modal::Comment) => ("INSERT", theme::green()),
        Some(_) => ("SEARCH", theme::purple()),
        None if app.visual() => ("VISUAL LINE", theme::cyan()),
        None => ("NORMAL", theme::yellow()),
    };
    // What has been typed and not yet resolved, shown where vim shows it.
    // A count or a half-finished prefix is a keystroke the reader is in the
    // middle of, and not showing it is how you end up pressing it twice.
    let pending = {
        let mut p = app.count.map(|n| n.to_string()).unwrap_or_default();
        p.push_str(match app.pending {
            crate::app::Pending::Leader => "␣",
            crate::app::Pending::G => "g",
            crate::app::Pending::Z => "z",
            crate::app::Pending::Bracket(crate::shared::nav::Dir::Prev) => "[",
            crate::app::Pending::Bracket(crate::shared::nav::Dir::Next) => "]",
            crate::app::Pending::None => "",
        });
        p
    };
    let mut x = put(
        buf,
        0,
        area.y,
        area.right(),
        &format!(" {mode} "),
        Style::default()
            .bg(mode_bg)
            .fg(theme::panel())
            .add_modifier(Modifier::BOLD),
    );

    x = put_trunc(
        buf,
        x + 1,
        area.y,
        area.right() / 2,
        app.path(),
        base.fg(theme::dim()),
    );

    let (lo, hi) = app.span();
    let pos = if app.visual() {
        format!("{} lines selected", hi - lo + 1)
    } else {
        format!("{}/{}", app.cursor + 1, app.diff_rows().len())
    };
    put(
        buf,
        x + 2,
        area.y,
        area.right(),
        &pos,
        base.fg(theme::dimmer()),
    );

    let hint = if app.visual() {
        "any motion extends · ␣n note on range · o other end · esc cancel"
    } else {
        "j/k move · }/]c hunk/change · ␣ leader · : commands · ␣? help"
    };
    let toast = format!(" {} ", app.toast);
    let tx = put_right(
        buf,
        area.right(),
        area.y,
        &toast,
        Style::default().bg(theme::sel()).fg(theme::yellow()),
    );
    // Where vim puts it: right of the hint, left of everything else.
    let hx = if pending.is_empty() {
        tx
    } else {
        put_right(
            buf,
            tx.saturating_sub(2),
            area.y,
            &pending,
            base.fg(theme::bright()),
        )
    };
    put_right(
        buf,
        hx.saturating_sub(2),
        area.y,
        hint,
        base.fg(theme::dimmer()),
    );
}
