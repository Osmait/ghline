//! The `:` command list.

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};

use crate::diffline::app::App;
use crate::tui::theme;
use crate::tui::{Dialog, put_right, put_trunc};

pub(crate) fn palette(buf: &mut Buffer, area: Rect, app: &App) {
    let hits = app.palette_hits();
    let body = Dialog::new(":")
        .hint("⏎ run · esc")
        .accent(theme::purple())
        .size(Size::new(70, (hits.len() as u16 + 6).min(area.height - 2)))
        .over_content()
        .open(buf, area);

    let list = body.query(buf, &app.query, ": ", "command…", app.blink);
    for slot in list.rows(buf, hits.len(), 1, app.sel, 0) {
        let Some(label) = hits.get(slot.index) else {
            continue;
        };
        let key = crate::diffline::input::COMMANDS
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, k)| *k)
            .unwrap_or("");
        put_trunc(
            buf,
            slot.area.x + 2,
            slot.area.y,
            slot.area.right() - 10,
            label,
            slot.style.fg(theme::bright()),
        );
        put_right(
            buf,
            slot.area.right() - 1,
            slot.area.y,
            key,
            slot.style.fg(theme::dimmer()),
        );
    }
}
