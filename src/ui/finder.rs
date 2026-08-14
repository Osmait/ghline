//! The finder modal: a query line, a row of sources, and the results.
//!
//! It takes most of the screen on purpose. Unlike the other modals, what you
//! are reading is the result list itself, so the interface behind it is not
//! worth preserving.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::overlay::{centered, frame, rule, scrim};
use super::{fill, put, put_right, put_trunc, scroll_into_view};
use crate::app::App;
use crate::finder::{HitKind, Source};
use crate::fuzzy;
use crate::theme;

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    scrim(buf, area);

    let width = area.width.saturating_sub(8).min(96);
    let height = area.height.saturating_sub(4).min(28);
    let modal = centered(area, width, height);
    frame(buf, modal, theme::cyan());

    let base = Style::default().bg(theme::panel());
    let inner_right = modal.right() - 2;

    // --- the query line
    let y = modal.y + 1;
    let mut x = put(
        buf,
        modal.x + 2,
        y,
        inner_right,
        "❯ ",
        base.fg(theme::cyan()),
    );
    if app.finder_query.is_empty() {
        put_trunc(
            buf,
            x,
            y,
            inner_right,
            app.finder_source.placeholder(),
            base.fg(theme::dimmer()),
        );
    } else {
        x = put_trunc(
            buf,
            x,
            y,
            inner_right,
            &app.finder_query,
            base.fg(theme::bright()),
        );
        if app.blink {
            put(buf, x, y, inner_right, "█", base.fg(theme::cyan()));
        }
    }
    rule(buf, modal, modal.y + 2, theme::cyan());

    // --- the sources
    let y = modal.y + 3;
    let mut x = modal.x + 2;
    for s in Source::ALL {
        let active = s == app.finder_source;
        let style = if active {
            base.fg(theme::bright()).add_modifier(Modifier::BOLD)
        } else {
            base.fg(theme::dimmer())
        };
        if active {
            x = put(buf, x, y, inner_right, "▸ ", base.fg(theme::cyan()));
        }
        x = put(buf, x, y, inner_right, s.label(), style);
        x = put(buf, x, y, inner_right, "   ", base);
    }
    put_right(buf, inner_right, y, "tab", base.fg(theme::dimmer()));

    // --- the results
    let list = Rect {
        x: modal.x + 1,
        y: modal.y + 5,
        width: modal.width - 2,
        height: modal.bottom().saturating_sub(modal.y + 7),
    };
    let hits = app.finder_results();
    let rows = (list.height / 2) as usize;

    if hits.is_empty() {
        let msg = if app.finder_state.is_loading() {
            "searching…".to_string()
        } else if let Some(e) = app.finder_state.error() {
            e.to_string()
        } else if app.finder_source.needs_query() && app.finder_query.trim().is_empty() {
            app.finder_source.placeholder().to_string()
        } else {
            "nothing matches".to_string()
        };
        let color = if app.finder_state.error().is_some() {
            theme::red()
        } else {
            theme::dimmer()
        };
        put_trunc(buf, list.x + 2, list.y, inner_right, &msg, base.fg(color));
    } else {
        scroll_into_view(&mut app.finder_scroll, app.finder_sel, rows, hits.len());
        for (row, i) in (app.finder_scroll..hits.len()).enumerate() {
            if row >= rows {
                break;
            }
            let y = list.y + (row as u16) * 2;
            let hit = &hits[i];
            let selected = i == app.finder_sel;
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
                put(buf, list.x, y, inner_right, "▌", s.fg(theme::cyan()));
                put(buf, list.x, y + 1, inner_right, "▌", s.fg(theme::cyan()));
            }

            // the kind, so a mixed list stays readable
            let (mark, mark_color) = match hit.kind {
                HitKind::Repo => ("▪", theme::cyan_soft()),
                HitKind::Issue => ("◉", theme::state_color(hit.state)),
                HitKind::Pr => ("⇅", theme::state_color(hit.state)),
                HitKind::Commit => ("◇", theme::purple()),
            };
            put(buf, list.x + 2, y, inner_right, mark, s.fg(mark_color));

            let fg = if selected {
                theme::bright()
            } else {
                theme::fg()
            };
            // only a local match knows which letters were hit
            let positions = if app.finder_source.is_local() {
                fuzzy::score(&app.finder_query, &hit.label)
                    .map(|(_, p)| p)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            put_matched(
                buf,
                (list.x + 4, y, inner_right),
                &hit.label,
                &positions,
                s.fg(fg),
            );
            put_trunc(
                buf,
                list.x + 4,
                y + 1,
                inner_right,
                &hit.detail,
                s.fg(theme::dimmer()),
            );
        }
    }

    // --- the footer
    let foot_y = modal.bottom() - 2;
    rule(buf, modal, foot_y - 1, theme::cyan());
    let count = if app.finder_state.is_loading() {
        "…".to_string()
    } else {
        format!("{}", hits.len())
    };
    put(
        buf,
        modal.x + 2,
        foot_y,
        inner_right,
        &format!("{count} results"),
        base.fg(theme::dimmer()),
    );
    put_right(
        buf,
        inner_right,
        foot_y,
        "↑↓ or ^n/^p · enter opens · esc",
        base.fg(theme::dimmer()),
    );
}

/// Writes `text`, brightening the characters the query matched. `at` is the
/// x, y and right edge to write between.
fn put_matched(
    buf: &mut Buffer,
    at: (u16, u16, u16),
    text: &str,
    positions: &[usize],
    base: Style,
) {
    let (x, y, max) = at;
    if positions.is_empty() {
        put_trunc(buf, x, y, max, text, base);
        return;
    }
    let hit = base.fg(theme::cyan()).add_modifier(Modifier::BOLD);
    let mut cx = x;
    for (i, c) in text.chars().enumerate() {
        let style = if positions.contains(&i) { hit } else { base };
        cx = put(buf, cx, y, max, &c.to_string(), style);
        if cx >= max {
            break;
        }
    }
}
