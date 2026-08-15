//! The keymap, read off the keymap.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::super::parts::KEY_W;
use crate::diffline::app::App;
use crate::shared::theme;
use crate::tui::{centered_over as centered, frame, put, put_right, put_trunc, rule};

/// The keymap, read off the keymap.
///
/// Generated rather than written down: a help that is a second list of the
/// bindings is a help that is wrong the first time somebody rebinds a key,
/// and being wrong about that is worse than not being there.
pub(crate) fn help(buf: &mut Buffer, area: Rect, app: &App) {
    let rows = app.keys.listing();
    let m = centered(area, 86, (rows.len() as u16).div_ceil(2) + 6);
    frame(buf, m, theme::yellow());
    let base = Style::default().bg(theme::panel());
    put(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        "KEYMAP",
        base.fg(theme::yellow()),
    );
    put_right(
        buf,
        m.right() - 2,
        m.y + 1,
        &match crate::diffline::keys::path() {
            Some(p) => format!("{}", p.display()),
            None => "no config directory".into(),
        },
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 2, theme::border());

    // Anything the reader's file got wrong, before the bindings — a key that
    // does nothing because of a typo three lines up is worth interrupting for.
    let mut top = m.y + 3;
    for problem in app.keys.problems.iter().take(3) {
        if top + 1 >= m.bottom() {
            break;
        }
        put_trunc(
            buf,
            m.x + 2,
            top,
            m.right() - 2,
            &format!("keys: {problem}"),
            base.fg(theme::red()),
        );
        top += 1;
    }

    let half = m.width / 2;
    for (i, (spec, action)) in rows.iter().enumerate() {
        let col = i % 2;
        let y = top + (i / 2) as u16;
        if y >= m.bottom() - 1 {
            break;
        }
        let x = m.x + 2 + col as u16 * half;
        put(buf, x, y, x + KEY_W, spec, base.fg(theme::yellow()));
        put_trunc(
            buf,
            x + KEY_W,
            y,
            x + half - 1,
            action.about(),
            base.fg(theme::dim()),
        );
    }
}
