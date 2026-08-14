//! Log view: the job/step tree on the left, streaming output on the right.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar, vline, wrap};
use crate::app::hit::{Region, Target};
use crate::app::{App, NodeKind, Pane};
use crate::data::Status;
use crate::theme;

const TREE_W: u16 = 38;

fn log_color(kind: &str) -> Color {
    match kind {
        "green" => theme::green(),
        "red" => theme::red(),
        "yellow" => theme::yellow(),
        "dim" => theme::dim(),
        "group" => theme::purple(),
        "fg" => theme::log_fg(),
        _ => theme::fg(),
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let tree_w = TREE_W.min(area.width / 2);
    let tree = Rect {
        x: area.x,
        y: area.y,
        width: tree_w,
        height: area.height,
    };
    let pane = Rect {
        x: area.x + tree_w + 1,
        y: area.y,
        width: area.width - tree_w - 1,
        height: area.height,
    };
    draw_tree(buf, tree, app);
    vline(buf, area.x + tree_w, area.y, area.height, theme::border());
    draw_pane(buf, pane, app);
}

fn draw_tree(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel_alt());
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::panel());
    put(
        buf,
        area.x + 1,
        area.y,
        area.right(),
        "JOBS & STEPS",
        Style::default().bg(theme::panel()).fg(theme::dim()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    let nodes = app.flat_tree();
    let sel = app.tree_sel_idx(nodes.len());
    let list_h = area.height.saturating_sub(2) as usize;
    scroll_into_view(&mut app.tree_scroll, sel, list_h, nodes.len());
    app.hits.push(Region::rows(
        Target::Pane(Pane::Tree),
        Rect {
            x: area.x,
            y: area.y + 2,
            width: area.width,
            height: area.height.saturating_sub(2),
        },
        1,
        app.tree_scroll,
        nodes.len(),
    ));

    for (row, i) in (app.tree_scroll..nodes.len()).enumerate() {
        if row >= list_h {
            break;
        }
        let y = area.y + 2 + row as u16;
        let n = &nodes[i];
        let selected = i == sel;
        let bg = if selected {
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
        if selected {
            let mark = if app.pane == Pane::Tree {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }

        // steps are indented 2 columns past their job (10px vs 26px)
        let indent = if n.kind == NodeKind::Job {
            area.x + 1
        } else {
            area.x + 5
        };
        let mut cx = indent;
        if n.kind == NodeKind::Job {
            let caret = if app.collapsed.contains(&n.ji) {
                "▸"
            } else {
                "▾"
            };
            cx = put(buf, cx, y, area.right(), caret, base.fg(theme::dimmer()));
            cx = put(buf, cx, y, area.right(), " ", base);
        }
        cx = put(
            buf,
            cx,
            y,
            area.right(),
            theme::state_icon(n.status),
            base.fg(theme::state_color(n.status)),
        );
        cx = put(buf, cx, y, area.right(), " ", base);

        let dur_color = if n.status == Status::Running {
            theme::yellow()
        } else {
            theme::dimmer()
        };
        let dur_x = put_right(buf, area.right() - 1, y, &n.dur, base.fg(dur_color));
        let fg = if selected {
            theme::bright()
        } else if n.kind == NodeKind::Job {
            theme::fg()
        } else {
            theme::step_fg()
        };
        put_trunc(buf, cx, y, dur_x.saturating_sub(1), &n.name, base.fg(fg));
    }
}

fn draw_pane(buf: &mut Buffer, area: Rect, app: &mut App) {
    // the output itself, below its one-row header: scrolled rather than
    // selected, so only the pane matters and not the row
    app.hits.push(Region::plain(
        Target::Pane(Pane::Log),
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        },
    ));
    let nodes = app.flat_tree();
    let idx = app.tree_sel_idx(nodes.len());
    let (name, status, dur) = match nodes.get(idx) {
        Some(n) => (n.name.clone(), n.status, n.dur.clone()),
        None => ("—".to_string(), Status::Pending, "—".to_string()),
    };

    // ---- header
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::panel());
    let base = Style::default().bg(theme::panel());
    let mut cx = put(
        buf,
        area.x + 1,
        area.y,
        area.right(),
        theme::state_icon(status),
        base.fg(theme::state_color(status)),
    );
    cx = put(buf, cx, area.y, area.right(), " ", base);
    // the rightmost ~32 columns are reserved (follow + shortcut)
    let head_max = area.right().saturating_sub(32).max(cx);
    cx = put_trunc(
        buf,
        cx,
        area.y,
        head_max,
        &name,
        base.fg(theme::state_color(status)),
    );
    cx = put(buf, cx, area.y, head_max, "  ", base);
    put_trunc(
        buf,
        cx,
        area.y,
        head_max,
        &format!("{dur} · attempt 1"),
        base.fg(theme::dimmer()),
    );

    let err_x = put_right(
        buf,
        area.right() - 1,
        area.y,
        "e → first error",
        base.fg(theme::dimmer()),
    );
    let (follow_label, follow_color) = if app.follow {
        ("● following [f]", theme::green())
    } else {
        ("○ paused [f]", theme::dimmer())
    };
    put_right(buf, err_x - 2, area.y, follow_label, base.fg(follow_color));
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    // ---- footer with the stats
    let foot_y = area.bottom() - 1;
    hline(buf, area.x, foot_y - 1, area.width, theme::border());
    fill(
        buf,
        Rect {
            x: area.x,
            y: foot_y,
            width: area.width,
            height: 1,
        },
        theme::panel(),
    );

    let rows = app.log_lines();
    let err_count = rows.iter().filter(|l| l.kind == "red").count();
    let stats = format!(
        "{} lines{} · {}",
        rows.len(),
        if app.log_filter.is_empty() {
            ""
        } else {
            " (filtered)"
        },
        dur
    );
    let fs = Style::default().bg(theme::panel());
    let sx = put(
        buf,
        area.x + 1,
        foot_y,
        area.right(),
        &stats,
        fs.fg(theme::dimmer()),
    );
    if err_count > 0 {
        put(
            buf,
            sx + 2,
            foot_y,
            area.right(),
            &format!("{err_count} errors · press e"),
            fs.fg(theme::red()),
        );
    }

    // ---- lines
    let view = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: foot_y.saturating_sub(area.y + 3),
    };
    if view.height == 0 {
        return;
    }

    let n_x = area.x + 1;
    let time_x = area.x + 8;
    let text_x = area.x + 17;
    let text_w = area.right().saturating_sub(text_x + 1) as usize;

    // flatten by wrapping each log line; only the first carries number and time
    struct Disp {
        n: Option<usize>,
        time: String,
        text: String,
        kind: &'static str,
    }
    let mut disp: Vec<Disp> = Vec::new();
    for l in &rows {
        let text = if l.text.is_empty() {
            " ".to_string()
        } else {
            l.text.clone()
        };
        let mut first = true;
        for part in wrap(&text, text_w.max(8)) {
            disp.push(Disp {
                n: if first { Some(l.n) } else { None },
                time: if first { l.time.clone() } else { String::new() },
                text: part,
                kind: l.kind,
            });
            first = false;
        }
    }

    if disp.is_empty() {
        let st = app.logs_status();
        if st.is_loading() {
            let avail = area.width.saturating_sub(20);
            let widths = [50, 30, 58, 40, 35, 64, 25, 45];
            for row in 0..(view.height as usize).min(14) {
                let ry = view.y + row as u16;
                skel_bar(buf, n_x + 2, ry, 3, row, app.anim);
                skel_bar(
                    buf,
                    text_x,
                    ry,
                    pct(avail, widths[row % widths.len()]),
                    row,
                    app.anim,
                );
            }
            return;
        }
        let (msg, color) = match st.error() {
            Some(e) => (e.to_string(), theme::red()),
            None => ("no output for this step".to_string(), theme::dimmer()),
        };
        put_trunc(
            buf,
            text_x,
            view.y,
            area.right() - 1,
            &msg,
            Style::default().bg(theme::bg()).fg(color),
        );
        return;
    }

    let tail = if status == Status::Running {
        "▌ streaming…".to_string()
    } else if !app.log_filter.is_empty() {
        format!("filter: /{}", app.log_filter)
    } else {
        "— end of log —".to_string()
    };
    disp.push(Disp {
        n: None,
        time: String::new(),
        text: tail,
        kind: "tail",
    });

    let h = view.height as usize;
    if app.follow {
        app.log_scroll = disp.len().saturating_sub(h);
    } else {
        app.log_scroll = app.log_scroll.min(disp.len().saturating_sub(h));
    }
    // the log height also feeds ^d/^u
    app.detail_height = view.height;

    let focused = app.pane == Pane::Log;
    for (row, i) in (app.log_scroll..disp.len()).enumerate() {
        if row >= h {
            break;
        }
        let y = view.y + row as u16;
        if focused {
            put(
                buf,
                view.x,
                y,
                view.x + 1,
                "▌",
                Style::default().bg(theme::bg()).fg(theme::cyan()),
            );
        }
        let d = &disp[i];
        let bg = if d.kind == "red" {
            theme::err_bg()
        } else {
            theme::bg()
        };
        fill(
            buf,
            Rect {
                x: view.x,
                y,
                width: view.width,
                height: 1,
            },
            bg,
        );
        let base = Style::default().bg(bg);

        if d.kind == "tail" {
            put(buf, text_x, y, area.right(), &d.text, base.fg(theme::dim()));
            continue;
        }
        if let Some(n) = d.n {
            put_right(buf, n_x + 5, y, &n.to_string(), base.fg(theme::gutter()));
            put(
                buf,
                time_x,
                y,
                area.right(),
                &d.time,
                base.fg(theme::dimmer()),
            );
        }
        put(
            buf,
            text_x,
            y,
            area.right() - 1,
            &d.text,
            base.fg(log_color(d.kind)),
        );
    }
}
