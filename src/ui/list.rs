//! Tab bar and the issue / PR / workflow run list.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use super::{fill, hline, put, put_right, put_trunc, scroll_into_view};
use crate::app::{App, Pane};
use crate::data::{Detail, Item, Kind, Status, TABS};
use crate::theme;

/// The tab row (`area.height == 1`) plus its bottom border at `y + 1`.
pub fn tabs(buf: &mut Buffer, area: Rect, app: &App) {
    fill(buf, area, theme::PANEL);
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
            theme::TAB_ACTIVE_BG
        } else {
            theme::PANEL
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
        cx = put(buf, cx, y, area.right(), t.key, base.fg(theme::DIMMER));
        cx = put(buf, cx, y, area.right(), " ", base);
        cx = put(
            buf,
            cx,
            y,
            area.right(),
            t.label,
            base.fg(if active { theme::BRIGHT } else { theme::DIM }),
        );
        if !count.is_empty() {
            cx = put(buf, cx, y, area.right(), " ", base);
            put(
                buf,
                cx,
                y,
                area.right(),
                &count,
                base.fg(if active { theme::CYAN } else { theme::DIMMER }),
            );
        }

        if active {
            active_span = (x, cell.width);
        }
        x += cell.width;
    }

    // filter label on the right
    let items = app.visible();
    let filter_label = if app.filter.is_empty() {
        format!("{} items · / to filter", items.len())
    } else {
        format!("/{}  {} match", app.filter, items.len())
    };
    // omitted if it would collide with the last tab
    if filter_label.width() as u16 + 2 <= area.right().saturating_sub(x + 2) {
        put_right(
            buf,
            area.right() - 2,
            y,
            &filter_label,
            Style::default().bg(theme::PANEL).fg(theme::DIMMER),
        );
    }

    // bottom border, with a cyan underline beneath the active tab
    hline(buf, area.x, y + 1, area.width, theme::BORDER);
    if active_span.1 > 0 {
        let s = "─".repeat(active_span.1 as usize);
        put(
            buf,
            active_span.0,
            y + 1,
            active_span.0 + active_span.1,
            &s,
            Style::default().fg(theme::CYAN).bg(theme::BG),
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
        Kind::Run => theme::state_icon(it.state),
    }
}

fn sub_for(it: &Item) -> String {
    match &it.detail {
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
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    // the scroll is settled before the list is borrowed
    let visible = app.visible();
    let sel_pos = app.item_idx(visible.len());
    let rows = (area.height / 2) as usize;
    scroll_into_view(&mut app.item_scroll, sel_pos, rows, visible.len());
    let scroll = app.item_scroll;
    let items: Vec<&Item> = visible.iter().map(|&i| &app.list()[i]).collect();

    if items.is_empty() {
        let st = app.list_state();
        let (msg, color) = if st.is_loading() {
            ("loading…".to_string(), theme::DIMMER)
        } else if let Some(e) = st.error() {
            (e.to_string(), theme::RED)
        } else if !app.filter.is_empty() {
            ("no matches".to_string(), theme::DIMMER)
        } else {
            ("nothing here".to_string(), theme::DIMMER)
        };
        put_trunc(
            buf,
            area.x + 3,
            area.y + 1,
            area.right() - 2,
            &msg,
            Style::default().bg(theme::BG).fg(color),
        );
        return;
    }

    for (row, i) in (scroll..items.len()).enumerate() {
        if row >= rows {
            break;
        }
        let y = area.y + (row as u16) * 2;
        let it = items[i];
        let selected = i == sel_pos;
        let bg = if selected { theme::SEL } else { theme::BG };
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
                theme::CYAN
            } else {
                theme::SEL_MARK_IDLE
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
            put(buf, area.x, y + 1, area.right(), "▌", base.fg(mark));
        }

        // icon + number
        let status = it.state;
        put(
            buf,
            area.x + 2,
            y,
            area.right(),
            icon_for(it),
            base.fg(theme::state_color(status)),
        );
        let text_x = area.x + 11;
        let num = format!("#{}", it.num);
        put_trunc(buf, area.x + 4, y, text_x - 1, &num, base.fg(theme::DIMMER));

        // estado (derecha)
        let state_text = if it.kind() == Kind::Pr {
            format!("{}  {} checks", it.state, theme::state_icon(it.checks()))
        } else {
            it.state.to_string()
        };
        let state_color = theme::state_color(if it.kind() == Kind::Pr {
            it.checks()
        } else {
            status
        });
        let state_x = put_right(buf, area.right() - 2, y, &state_text, base.fg(state_color));

        // title + labels
        let title_max = state_x.saturating_sub(2);
        let labels_w: u16 = it.labels.iter().map(|l| l.name.width() as u16 + 3).sum();
        let title_room = title_max.saturating_sub(text_x).saturating_sub(labels_w);
        let fg = if selected { theme::BRIGHT } else { theme::FG };
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
            base.fg(theme::DIMMER),
        );
    }
}
