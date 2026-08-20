//! Tab bar and the issue / PR / workflow run list.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::app::hit::{Region, Target};
use crate::app::{App, Pane};
use crate::data::{Detail, Item, Kind, Status, TABS};
use crate::tui::theme;
use crate::tui::{fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar};

/// The tab row (`area.height == 1`) plus its bottom border at `y + 1`.
pub(crate) fn tabs(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel());
    let y = area.y;
    let repo = app.repo().cloned().unwrap_or_else(crate::data::Repo::empty);

    let mut x = area.x;
    let mut active_span = (0u16, 0u16);

    for (i, t) in TABS.iter().enumerate() {
        let active = i == app.tab;
        let count = match t.id {
            "issues" => repo.issues.to_string(),
            "prs" => repo.prs.to_string(),
            _ => String::new(),
        };
        let bg = if active {
            theme::tab_active_bg()
        } else {
            theme::panel()
        };
        let label_w = 2
            + 1
            + 1
            + t.label.width() as u16
            + if count.is_empty() {
                0
            } else {
                count.width() as u16 + 1
            }
            + 2;
        let cell = Rect {
            x,
            y,
            width: label_w.min(area.right().saturating_sub(x)),
            height: 1,
        };
        fill(buf, cell, bg);

        let base = Style::default().bg(bg);
        let mut cx = x + 2;
        cx = put(buf, cx, y, area.right(), t.key, base.fg(theme::dimmer()));
        cx = put(buf, cx, y, area.right(), " ", base);
        cx = put(
            buf,
            cx,
            y,
            area.right(),
            t.label,
            base.fg(if active {
                theme::bright()
            } else {
                theme::dim()
            }),
        );
        if !count.is_empty() {
            cx = put(buf, cx, y, area.right(), " ", base);
            put(
                buf,
                cx,
                y,
                area.right(),
                &count,
                base.fg(if active {
                    theme::cyan()
                } else {
                    theme::dimmer()
                }),
            );
        }

        if active {
            active_span = (x, cell.width);
        }
        app.hits.push(Region::plain(Target::Tab(i), cell));
        x += cell.width;
    }

    // filter label on the right, counting whatever the active tab is showing
    let count = match app.tab {
        crate::data::AGENTS_TAB => app.agents_visible().len(),
        crate::data::FILES_TAB => app.fs_rows().len(),
        _ => app.visible().len(),
    };
    let filter_label = if app.filter.is_empty() {
        format!("{count} items · / to filter")
    } else {
        format!("/{}  {count} match", app.filter)
    };
    // omitted if it would collide with the last tab
    if filter_label.width() as u16 + 2 <= area.right().saturating_sub(x + 2) {
        put_right(
            buf,
            area.right() - 2,
            y,
            &filter_label,
            Style::default().bg(theme::panel()).fg(theme::dimmer()),
        );
    }

    // bottom border, with a cyan underline beneath the active tab
    hline(buf, area.x, y + 1, area.width, theme::border());
    if active_span.1 > 0 {
        let s = "─".repeat(active_span.1 as usize);
        put(
            buf,
            active_span.0,
            y + 1,
            active_span.0 + active_span.1,
            &s,
            Style::default().fg(theme::cyan()).bg(theme::bg()),
        );
    }
}

fn icon_for(it: &Item) -> &'static str {
    match it.kind() {
        Kind::Issue => {
            if it.state == Status::Open {
                "◉"
            } else {
                "⊙"
            }
        }
        Kind::Pr => match it.state {
            Status::Merged => "⑃",
            Status::Draft => "⑂",
            _ => "⇅",
        },
        Kind::Run => super::state_icon(it.state),
    }
}

fn sub_for(it: &Item) -> String {
    // In a list that spans repositories, which one a row came from is the
    // first thing you need; elsewhere it is the one thing you already know.
    let where_from = match it.repo.rsplit('/').next() {
        Some(r) if !it.repo.is_empty() => format!("{r} · "),
        _ => String::new(),
    };
    let rest = match &it.detail {
        Detail::Run(run) => {
            format!("{} · {} · {} · {}", run.event, it.author, run.dur, it.when)
        }
        Detail::Pr(pr) => format!(
            "{}{} · {} · {}/{} · {} files · {}",
            pr.branch,
            if pr.branch_deleted { " ⊘" } else { "" },
            it.author,
            pr.add,
            pr.del,
            pr.files,
            it.when
        ),
        Detail::Issue(issue) => {
            format!("{} · {} comments · {}", it.author, issue.comments, it.when)
        }
    };
    format!("{where_from}{rest}")
}

pub(crate) fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    // the scroll is settled before the list is borrowed
    let visible = app.visible();
    let sel_pos = app.item_idx(visible.len());
    let rows = (area.height / 2) as usize;
    scroll_into_view(&mut app.item_scroll, sel_pos, rows, visible.len());
    let scroll = app.item_scroll;
    // registered even when the list is empty, so clicking a loading or empty
    // pane still moves the focus there
    app.hits.push(Region::rows(
        Target::Pane(Pane::List),
        area,
        2,
        scroll,
        visible.len(),
    ));
    let items: Vec<&Item> = visible.iter().map(|&i| &app.list()[i]).collect();

    if items.is_empty() {
        let st = app.list_state();
        if st.is_loading() {
            // the outline of the rows that are coming, so the pane does not
            // jump when they arrive
            let avail = area.width.saturating_sub(24);
            let titles = [44, 58, 32, 50, 38, 54];
            let subs = [26, 20, 30, 23, 28, 18];
            // the design says how many rows to hint at: `hint-placeholder-count`
            let rows = ((area.height / 2) as usize).min(10);
            for row in 0..rows {
                let y = area.y + (row as u16) * 2;
                skel_bar(buf, area.x + 2, y, 1, row, app.anim); // icon
                skel_bar(buf, area.x + 4, y, 5, row, app.anim); // number
                let tw = pct(avail, titles[row % titles.len()]);
                skel_bar(buf, area.x + 11, y, tw, row, app.anim);
                skel_bar(buf, area.right().saturating_sub(12), y, 9, row, app.anim);
                let sw = pct(avail, subs[row % subs.len()]);
                skel_bar(buf, area.x + 11, y + 1, sw, row, app.anim);
            }
            return;
        }
        let (msg, color) = if let Some(e) = st.error() {
            (e.to_string(), theme::red())
        } else if !app.filter.is_empty() {
            ("no matches".to_string(), theme::dimmer())
        } else {
            ("nothing here".to_string(), theme::dimmer())
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

    // Two lines per entry, and every x below is one of these columns:
    //
    //     ▌ ⇅ #217   fix(layout): clamp the sidebar [bug]     open  ✗ checks
    //     ▌          fix/sidebar-clamp · ada-example · +128/-34 · 26m ago
    //     │ │ │      │                                        │
    //     0 2 4      11 = text_x                              state_x
    //
    // `state_x` is not a column but a result: the state is laid out from
    // `right() - 2` leftwards, and where it ends is where the title has to
    // stop. So it is worked out before the title, which is why it is not in
    // the order it is read in.
    for (row, i) in (scroll..items.len()).enumerate() {
        if row >= rows {
            break;
        }
        let y = area.y + (row as u16) * 2;
        let it = items[i];
        let selected = i == sel_pos;
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

        let base = Style::default().bg(bg);
        if selected {
            let mark = if app.pane == Pane::List {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
            put(buf, area.x, y + 1, area.right(), "▌", base.fg(mark));
        }

        let status = it.state;
        put(
            buf,
            area.x + 2,
            y,
            area.right(),
            icon_for(it),
            base.fg(super::state_color(status)),
        );
        let text_x = area.x + 11;
        let num = format!("#{}", it.num);
        put_trunc(
            buf,
            area.x + 4,
            y,
            text_x - 1,
            &num,
            base.fg(theme::dimmer()),
        );

        let state_text = if it.kind() == Kind::Pr {
            format!("{}  {} checks", it.state, super::state_icon(it.checks()))
        } else {
            it.state.to_string()
        };
        let state_color = super::state_color(if it.kind() == Kind::Pr {
            it.checks()
        } else {
            status
        });
        let state_x = put_right(buf, area.right() - 2, y, &state_text, base.fg(state_color));

        let title_max = state_x.saturating_sub(2);
        let labels_w: u16 = it.labels.iter().map(|l| l.name.width() as u16 + 3).sum();
        let title_room = title_max.saturating_sub(text_x).saturating_sub(labels_w);
        let fg = if selected {
            theme::bright()
        } else {
            theme::fg()
        };
        let mut lx = put_trunc(buf, text_x, y, text_x + title_room, &it.title, base.fg(fg));
        for l in &it.labels {
            lx = put(buf, lx, y, title_max, " ", base);
            let chip = format!("[{}]", l.name);
            lx = put(buf, lx, y, title_max, &chip, base.fg(theme::label(l.rgb)));
        }

        put_trunc(
            buf,
            text_x,
            y + 1,
            area.right() - 2,
            &sub_for(it),
            base.fg(theme::dimmer()),
        );
    }
}
