//! The fuzzy finder.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::diffline::app::{App, FinderTab};
use crate::diffline::model::Kind;
use crate::tui::theme;
use crate::tui::{
    centered_over as centered, fill, frame, put, put_right, put_trunc, rule, scroll_into_view,
    vline,
};

pub(crate) fn finder(buf: &mut Buffer, area: Rect, app: &App) {
    let m = centered(
        area,
        area.width.saturating_sub(8).min(120),
        area.height * 3 / 4,
    );
    frame(buf, m, theme::yellow());
    let base = Style::default().bg(theme::panel());

    let mut x = m.x + 2;
    for t in FinderTab::ALL {
        let on = t == app.finder_tab;
        let style = if on {
            base.bg(theme::yellow()).fg(theme::panel())
        } else {
            base.fg(theme::dim())
        };
        x = put(
            buf,
            x,
            m.y + 1,
            m.right() - 2,
            &format!(" {} ", t.label()),
            style,
        );
        x += 1;
    }
    put_right(
        buf,
        m.right() - 2,
        m.y + 1,
        "⇥ scope",
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 2, theme::border());

    let hits = app.hits();
    crate::tui::query_line(
        buf,
        m,
        m.y + 3,
        &crate::tui::Query {
            text: &app.query,
            lead: "❯ ",
            placeholder: "fuzzy find…",
            caret: app.blink,
            accent: theme::yellow(),
        },
    );
    put_right(
        buf,
        m.right() - 2,
        m.y + 3,
        &format!("{} results", hits.len()),
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 4, theme::border());

    // Results on the left, what the highlighted one looks like on the right.
    let split = m.width * 44 / 100;
    let list = Rect {
        x: m.x + 1,
        y: m.y + 5,
        width: split,
        height: m.height.saturating_sub(7),
    };
    vline(buf, m.x + split + 1, list.y, list.height, theme::border());

    let mut scroll = app
        .sel
        .saturating_sub(list.height.saturating_sub(1) as usize);
    scroll_into_view(&mut scroll, app.sel, list.height as usize, hits.len());
    for (n, h) in hits.iter().enumerate().skip(scroll) {
        let y = list.y + (n - scroll) as u16;
        if y >= list.bottom() {
            break;
        }
        let sel = n == app.sel;
        let bg = if sel { theme::sel() } else { theme::panel() };
        fill(
            buf,
            Rect {
                x: list.x,
                y,
                width: list.width,
                height: 1,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if sel {
            put(buf, list.x, y, list.right(), "▌", s.fg(theme::yellow()));
        }
        put(
            buf,
            list.x + 2,
            y,
            list.right(),
            &h.icon,
            s.fg(theme::cyan()),
        );
        let mx = put_right(buf, list.right() - 1, y, &h.meta, s.fg(theme::dimmer()));
        put_trunc(
            buf,
            list.x + 4,
            y,
            mx.saturating_sub(1),
            &h.label,
            s.fg(if sel { theme::bright() } else { theme::fg() }),
        );
    }

    // the preview
    let pv = Rect {
        x: m.x + split + 2,
        y: m.y + 5,
        width: m.width.saturating_sub(split + 3),
        height: list.height,
    };
    if let Some(hit) = hits.get(app.sel) {
        let path = app
            .files
            .get(hit.file)
            .map(|f| f.path.as_str())
            .unwrap_or("");
        put_trunc(buf, pv.x, pv.y, pv.right(), path, base.fg(theme::dim()));
        // Borrowed: this shows four lines of a preview, and copying the
        // whole file's rows to do it is the same mistake the diff pane had.
        let empty = Vec::new();
        let rows = app.rows.get(path).unwrap_or(&empty);
        let centre = hit.row.unwrap_or(0);
        let start = centre.saturating_sub(4);
        for (n, r) in rows.iter().enumerate().skip(start) {
            let y = pv.y + 2 + (n - start) as u16;
            if y >= pv.bottom() {
                break;
            }
            let fg = match r.kind {
                Kind::Added => theme::green(),
                Kind::Deleted => theme::red(),
                Kind::Header => theme::cyan_soft(),
                Kind::Context => theme::dim(),
            };
            let bg = if Some(n) == hit.row {
                theme::sel()
            } else {
                theme::panel()
            };
            fill(
                buf,
                Rect {
                    x: pv.x,
                    y,
                    width: pv.width,
                    height: 1,
                },
                bg,
            );
            let s = Style::default().bg(bg);
            put_right(
                buf,
                pv.x + 5,
                y,
                &r.new.or(r.old).map(|v| v.to_string()).unwrap_or_default(),
                s.fg(theme::dimmer()),
            );
            put_trunc(
                buf,
                pv.x + 7,
                y,
                pv.right(),
                &format!("{}{}", r.sign(), r.text),
                s.fg(fg),
            );
        }
    }

    put(
        buf,
        m.x + 2,
        m.bottom() - 2,
        m.right() - 2,
        "↑↓ move · ↵ jump · ⇥ scope · esc close",
        base.fg(theme::dimmer()),
    );
}
