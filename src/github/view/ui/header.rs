//! The top bar.
//!
//! ```text
//!  gh │ ● marasanz (personal) [a] │ marasanz / tuikit › Pull Requests   synced 3s ago · idle  ? help
//!  ─┬─  ─────────────┬───────────   ──────────────┬───────────────────   ──────────┬─────────  ──┬───
//!   │                │                            │                                │             │
//!   │                account, clickable           where you are, one crumb per     │             the
//!   the mark                                      level and each one clickable     │             help
//!                                                                                  what the worker
//!                                                                                  is doing, right
//!                                                                                  aligned
//! ```
//!
//! The crumbs are laid out left to right and the status right to left; they
//! meet in the middle, and the crumbs are what gives way when there is not
//! enough room for both.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::github::app::{App, View};
use crate::github::data::TABS;
use crate::tui::theme;
use crate::tui::{bold, fill, put, put_right};

pub(crate) struct Crumb {
    pub label: String,
    pub color: Color,
    pub sep: &'static str,
}

pub(crate) fn crumbs(app: &App) -> Vec<Crumb> {
    let mut out = vec![
        Crumb {
            label: app.login().to_string(),
            color: theme::dim(),
            sep: "/",
        },
        Crumb {
            label: app.repo_label().to_string(),
            color: theme::bright(),
            sep: "›",
        },
        Crumb {
            label: TABS[app.tab].label.to_string(),
            color: if app.view == View::List {
                theme::cyan()
            } else {
                theme::dim()
            },
            sep: if app.view == View::List { "" } else { "›" },
        },
    ];

    if app.view != View::List
        && let Some(cur) = app.current()
    {
        out.push(Crumb {
            label: format!("#{}", cur.num),
            color: if app.view == View::Detail {
                theme::cyan()
            } else {
                theme::dim()
            },
            sep: if app.view == View::Detail { "" } else { "›" },
        });
    }
    if app.view == View::Diff {
        out.push(Crumb {
            label: "diff".to_string(),
            color: theme::cyan(),
            sep: "",
        });
    }
    if app.view == View::Logs {
        let tree = app.flat_tree();
        let name = tree
            .get(app.tree_sel_idx(tree.len()))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "—".into());
        out.push(Crumb {
            label: name,
            color: theme::cyan(),
            sep: "",
        });
    }
    out
}

pub(crate) fn draw(buf: &mut Buffer, area: Rect, app: &App) {
    fill(buf, area, theme::panel());
    let y = area.y;
    let max = area.right();
    let base = Style::default().bg(theme::panel());

    let mut x = area.x + 1;
    x = put(buf, x, y, max, "gh", bold(base.fg(theme::yellow())));
    x = put(buf, x, y, max, "  │  ", base.fg(theme::dim()));

    // the active account
    x = put(buf, x, y, max, "●", base.fg(theme::green()));
    x = put(buf, x, y, max, " ", base);
    x = put(buf, x, y, max, app.login(), base.fg(theme::fg()));
    x = put(buf, x, y, max, " ", base);
    x = put(
        buf,
        x,
        y,
        max,
        app.account().map(|a| a.kind.as_str()).unwrap_or(""),
        base.fg(theme::dim()),
    );
    x = put(buf, x, y, max, " [a]", base.fg(theme::dim()));
    x = put(buf, x, y, max, "  │  ", base.fg(theme::dim()));

    // breadcrumbs; the right-hand side is reserved
    let sync = if app.waiting() {
        "gh · syncing…"
    } else {
        "gh · live"
    };
    let right_w = sync.chars().count() as u16 + 10;
    let crumb_max = max.saturating_sub(right_w).max(x);

    for c in crumbs(app) {
        x = put(buf, x, y, crumb_max, &c.label, base.fg(c.color));
        if !c.sep.is_empty() {
            x = put(buf, x, y, crumb_max, " ", base);
            x = put(buf, x, y, crumb_max, c.sep, base.fg(theme::gutter()));
            x = put(buf, x, y, crumb_max, " ", base);
        }
    }

    let help_x = put_right(buf, max - 1, y, "?  help", base.fg(theme::dimmest()));
    put_right(buf, help_x - 2, y, sync, base.fg(theme::dim()));
}
