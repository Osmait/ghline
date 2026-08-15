//! Choosing a palette.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::diffline::app::App;
use crate::diffline::hit::{Region, Target};
use crate::tui::theme;
use crate::tui::{Dialog, put, put_trunc};

/// The theme picker. Small on purpose — it sits over the diff, and the diff
/// is what you are actually judging the colours against.
pub(crate) fn themes(buf: &mut Buffer, area: Rect, app: &mut App) {
    let all = crate::tui::theme::Theme::all();
    let body = Dialog::new("THEME")
        .hint("⏎ keep · esc undo")
        .accent(theme::cyan())
        .size(60, (all.len() as u16 * 2 + 5).min(area.height - 2))
        .over_content()
        .open(buf, area);

    app.hits.push(Region::plain(Target::Modal, body.outer));
    app.hits
        .push(Region::rows(Target::Modal, body.inner, 2, 0, all.len()));

    for slot in body.rows(buf, all.len(), 2, app.sel, 0) {
        let Some(t) = all.get(slot.index) else {
            continue;
        };
        let s = slot.style;
        put_trunc(
            buf,
            slot.area.x + 2,
            slot.area.y,
            slot.area.right() - 12,
            t.name(),
            s.fg(theme::bright()),
        );
        put_trunc(
            buf,
            slot.area.x + 2,
            slot.area.y + 1,
            slot.area.right() - 1,
            t.about(),
            s.fg(theme::dimmer()),
        );
        // The accents themselves, so the list shows a theme rather than
        // naming one — the picker is already painted in whichever is chosen.
        let mut x = slot.area.right() - 11;
        for c in [
            theme::green(),
            theme::yellow(),
            theme::red(),
            theme::cyan(),
            theme::purple(),
        ] {
            x = put(buf, x, slot.area.y, slot.area.right() - 1, "██", s.fg(c));
        }
    }
}
