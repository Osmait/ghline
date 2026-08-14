//! Left pane: the active account's repository list.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar};
use crate::app::hit::{Region, Target};
use crate::app::{App, Pane};
use crate::theme;

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel_alt());

    let top = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, top, theme::panel());
    let head = Style::default().bg(theme::panel()).fg(theme::dim());
    put(buf, area.x + 1, area.y, area.right(), "REPOSITORIES", head);

    let repos_len = app.repos().len();
    let idx = app.repo_idx();
    let num = format!("{}/{}", idx + 1, repos_len);
    let mark_color = if app.pane == Pane::Repos {
        theme::cyan()
    } else {
        theme::dimmer()
    };
    put_right(buf, area.right() - 1, area.y, &num, head.fg(mark_color));

    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    // footer with the totals
    let foot_y = area.bottom() - 1;
    hline(buf, area.x, foot_y - 1, area.width, theme::border_soft());
    let foot = Rect {
        x: area.x,
        y: foot_y,
        width: area.width,
        height: 1,
    };
    fill(buf, foot, theme::panel());
    let totals = format!(
        "{} open issues · {} open PRs",
        app.repos().iter().map(|r| r.issues).sum::<u32>(),
        app.repos().iter().map(|r| r.prs).sum::<u32>()
    );
    put_trunc(
        buf,
        area.x + 1,
        foot_y,
        area.right() - 1,
        &totals,
        Style::default().bg(theme::panel()).fg(theme::dimmer()),
    );

    // rows
    let list = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: foot_y.saturating_sub(area.y + 3),
    };
    scroll_into_view(&mut app.repo_scroll, idx, list.height as usize, repos_len);
    app.hits.push(Region::rows(
        Target::Pane(Pane::Repos),
        list,
        1,
        app.repo_scroll,
        repos_len,
    ));

    if repos_len == 0 {
        let loading = app
            .repos_state
            .get(app.login())
            .map(super::super::app::Load::is_loading)
            .unwrap_or(true);
        if loading {
            let avail = area.width.saturating_sub(12);
            let names = [56, 40, 68, 47, 34, 60, 44, 52];
            for row in 0..(list.height as usize).min(8) {
                let y = list.y + row as u16;
                let w = pct(avail, names[row % names.len()]);
                skel_bar(buf, area.x + 2, y, w, row, app.anim);
                skel_bar(buf, area.right().saturating_sub(9), y, 7, row, app.anim);
            }
            return;
        }
        let (msg, color) = match app.repos_state.get(app.login()).and_then(|s| s.error()) {
            Some(e) => (e.to_string(), theme::red()),
            None => ("no repositories".to_string(), theme::dimmer()),
        };
        put_trunc(
            buf,
            area.x + 2,
            list.y,
            area.right() - 1,
            &msg,
            Style::default().bg(theme::panel_alt()).fg(color),
        );
        return;
    }

    for (row, i) in (app.repo_scroll..repos_len).enumerate() {
        if row as u16 >= list.height {
            break;
        }
        let y = list.y + row as u16;
        let r = &app.repos()[i];
        let sel = i == idx;
        let bg = if sel {
            theme::sel()
        } else {
            theme::panel_alt()
        };
        fill(
            buf,
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            bg,
        );

        let base = Style::default().bg(bg);
        if sel {
            let mark = if app.pane == Pane::Repos {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }

        let priv_mark = if r.private {
            theme::PRIVATE_MARK
        } else {
            theme::PUBLIC_MARK
        };
        let priv_color = if r.private {
            theme::yellow()
        } else {
            theme::dimmer()
        };
        put(
            buf,
            area.x + 2,
            y,
            area.x + 3,
            priv_mark,
            base.fg(priv_color),
        );

        let counts = format!("{}i {}p", r.issues, r.prs);
        let counts_x = put_right(buf, area.right() - 1, y, &counts, base.fg(theme::dimmer()));
        let dot_x = put_right(buf, counts_x - 1, y, "●", base.fg(theme::lang(&r.lang)));

        let fg = if sel { theme::bright() } else { theme::fg() };
        put_trunc(
            buf,
            area.x + 4,
            y,
            dot_x.saturating_sub(1),
            r.label(),
            base.fg(fg),
        );
    }
}
