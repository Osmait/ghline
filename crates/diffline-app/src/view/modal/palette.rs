//! The `:` command list.

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};

use crate::app::App;
use crate::hit::{Region, Target};
use crate::tui::theme;
use crate::tui::{Dialog, put_right, put_trunc, scroll_into_view};

pub(crate) fn palette(buf: &mut Buffer, area: Rect, app: &mut App) {
    let hits = app.palette_hits();
    let body = Dialog::new(":")
        .hint("⏎ run · esc")
        .accent(theme::purple())
        .size(Size::new(70, (hits.len() as u16 + 6).min(area.height - 2)))
        .over_content()
        .open(buf, area);

    let list = body.query(buf, &app.query, ": ", "command…", app.blink);
    // The box under the rows, so a click inside stays inside and a click on
    // an entry means that entry.
    app.hits.push(Region::plain(Target::Modal, body.outer));
    // scrolled to the selection: the list outgrows a short terminal, and its
    // last entries — help, quit — were off its bottom with no way down
    let mut scroll = 0;
    scroll_into_view(&mut scroll, app.sel, list.inner.height as usize, hits.len());
    app.hits.push(Region::rows(
        Target::Modal,
        list.inner,
        1,
        scroll,
        hits.len(),
    ));
    for slot in list.rows(buf, hits.len(), 1, app.sel, scroll) {
        let Some(label) = hits.get(slot.index) else {
            continue;
        };
        // the key as the keymap has it now, not as it shipped
        let key = crate::input::COMMANDS
            .iter()
            .find(|(l, _)| l == label)
            .and_then(|(_, a)| *a)
            .and_then(|a| app.keys.key_for(a))
            .unwrap_or_default();
        let key = key.as_str();
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
