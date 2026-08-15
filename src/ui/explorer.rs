//! The repository's files: a tree on the left, the file on the right.
//!
//! The same two-pane shape as the logs and the diff, because it is the same
//! question — pick a thing, read the thing. The tree comes down whole in one
//! request, so opening a directory is instant and nothing waits on the network
//! except the file you actually asked to read.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar, vline};
use crate::app::hit::{Region, Target};
use crate::app::{App, Load, Pane};
use crate::icons::{file as icon_file, folder};
use crate::theme;

/// Width of the tree pane. Wide enough for a nested path, narrow enough that
/// the file still gets most of the screen — which is what you came to read.
const TREE_W: u16 = 42;

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    // The gathering row has no tree of its own: there is no such thing as the
    // files of a hundred and forty repositories at once.
    if app.is_all() {
        put_trunc(
            buf,
            area.x + 3,
            area.y + 1,
            area.right() - 2,
            "pick a repository first — [ and ] step through them, or p finds one by name",
            Style::default().bg(theme::bg()).fg(theme::dimmer()),
        );
        return;
    }

    let tree_w = TREE_W.min(area.width / 2);
    let tree = Rect {
        x: area.x,
        y: area.y,
        width: tree_w,
        height: area.height,
    };
    let view = Rect {
        x: area.x + tree_w + 1,
        y: area.y,
        width: area.width - tree_w - 1,
        height: area.height,
    };
    draw_tree(buf, tree, app);
    vline(buf, area.x + tree_w, area.y, area.height, theme::border());
    draw_file(buf, view, app);
}

/// What each kind of token looks like. Borrowed from the diff and the logs,
/// so a comment here is the same colour as a comment there.
fn kind_color(kind: crate::syntax::Kind) -> ratatui::style::Color {
    use crate::syntax::Kind;
    match kind {
        Kind::Comment => theme::dimmer(),
        Kind::Str => theme::green(),
        Kind::Number => theme::orange(),
        Kind::Keyword => theme::purple(),
        Kind::Type => theme::cyan_soft(),
    }
}

/// A file's size, short enough for the right-hand column.
fn human(bytes: u64) -> String {
    match bytes {
        0 => String::new(),
        b if b < 1024 => format!("{b}"),
        b if b < 1024 * 1024 => format!("{}K", b / 1024),
        b => format!("{}M", b / (1024 * 1024)),
    }
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
    let hs = Style::default().bg(theme::panel()).fg(theme::dim());
    put(buf, area.x + 1, area.y, area.right(), "FILES", hs);

    let state = app.tree_state();
    let rows = app.fs_rows().len();
    put_right(
        buf,
        area.right() - 1,
        area.y,
        &format!("{rows}"),
        hs.fg(theme::dimmer()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    let list_h = area.height.saturating_sub(2) as usize;
    let sel = app.fs_idx();
    scroll_into_view(&mut app.fs_scroll, sel, list_h, rows);
    let list = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(2),
    };
    app.hits.push(Region::rows(
        Target::Pane(Pane::FileTree),
        list,
        1,
        app.fs_scroll,
        rows,
    ));

    if rows == 0 {
        if state.is_loading() {
            let avail = area.width.saturating_sub(10);
            let widths = [62, 40, 74, 51, 36, 68, 45];
            for row in 0..list_h.min(7) {
                let y = list.y + row as u16;
                let indent = u16::from(row % 3 == 1) * 2;
                skel_bar(
                    buf,
                    area.x + 2 + indent,
                    y,
                    pct(avail, widths[row % widths.len()]),
                    row,
                    app.anim,
                );
            }
            return;
        }
        let (msg, color) = match state.error() {
            Some(e) => (e.to_string(), theme::red()),
            None if !app.filter.is_empty() => ("no file matches".to_string(), theme::dimmer()),
            None => ("no files".to_string(), theme::dimmer()),
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

    // A filter flattens the tree, so the indent would be a lie about where the
    // rows sit relative to each other.
    let flat = !app.filter.trim().is_empty();
    let icon_style = crate::config::file_icons();
    let entries: Vec<crate::data::TreeEntry> = app.fs_rows().into_iter().cloned().collect();
    let focused = app.pane == Pane::FileTree;

    for (row, i) in (app.fs_scroll..entries.len()).enumerate() {
        if row >= list_h {
            break;
        }
        let y = list.y + row as u16;
        let e = &entries[i];
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
            let mark = if focused {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }

        let indent = if flat { 0 } else { e.depth() as u16 * 2 };
        let x = area.x + 2 + indent.min(area.width.saturating_sub(12));

        let size = human(e.size);
        let size_x = put_right(buf, area.right() - 1, y, &size, base.fg(theme::dimmer()));

        // An open directory points down, a closed one points right: the same
        // convention the jobs tree uses two views away. The arrow says what
        // will happen; the icon beside it says what the thing is.
        let (arrow, color) = if e.is_dir {
            let open = app.fs_open.contains(&e.path);
            (if open { "▾" } else { "▸" }, theme::cyan_soft())
        } else {
            (" ", theme::dimmer())
        };
        let mut mx = put(buf, x, y, area.right(), arrow, base.fg(color));

        let icon = if e.is_dir {
            folder(icon_style, app.fs_open.contains(&e.path))
        } else {
            icon_file(icon_style, &e.path)
        };
        if !icon.is_empty() {
            let icolor = if e.is_dir {
                theme::cyan_soft()
            } else {
                theme::lang(crate::icons::language(&e.path))
            };
            mx = put(buf, mx, y, area.right(), icon, base.fg(icolor));
            mx = put(buf, mx, y, area.right(), " ", base);
        }

        let fg = match (e.is_dir, selected) {
            (true, _) => theme::cyan_soft(),
            (false, true) => theme::bright(),
            (false, false) => theme::fg(),
        };
        let label = if flat { &e.path } else { e.name() };
        put_trunc(buf, mx, y, size_x.saturating_sub(1), label, base.fg(fg));
    }
}

fn draw_file(buf: &mut Buffer, area: Rect, app: &mut App) {
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::panel());
    let base = Style::default().bg(theme::panel());

    let path = app.fs_current().map(|e| e.path.clone()).unwrap_or_default();
    put_trunc(
        buf,
        area.x + 1,
        area.y,
        area.right().saturating_sub(14),
        &path,
        base.fg(theme::dim()),
    );
    put_right(
        buf,
        area.right() - 1,
        area.y,
        "x → agent",
        base.fg(theme::dimmer()),
    );

    let body = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(1),
    };
    app.hits
        .push(Region::plain(Target::Pane(Pane::FileView), body));

    let text = match app.file_body() {
        Ok(text) => text.to_string(),
        Err(Load::Loading) => {
            let avail = body.width.saturating_sub(10);
            let widths = [72, 48, 84, 30, 66, 54, 78, 41, 60];
            for row in 0..(body.height as usize).min(widths.len() * 2) {
                let y = body.y + row as u16;
                if row % 4 == 3 {
                    continue; // a blank line, so it reads as code rather than a wall
                }
                skel_bar(
                    buf,
                    body.x + 2,
                    y,
                    pct(avail, widths[row % widths.len()]),
                    row,
                    app.anim,
                );
            }
            return;
        }
        Err(state) => {
            let (msg, color) = match state.error() {
                Some(e) => (e.to_string(), theme::red()),
                None => ("select a file to read it".to_string(), theme::dimmer()),
            };
            put_trunc(
                buf,
                body.x + 2,
                body.y,
                area.right() - 2,
                &msg,
                Style::default().bg(theme::bg()).fg(color),
            );
            return;
        }
    };

    // Wrapped rather than cut: a long line in a config file is still worth
    // reading, and there is no horizontal scroll in this program. Wrapped by
    // byte range rather than by text, so a colour span survives the wrap.
    let gutter = 6u16;
    let width = body.width.saturating_sub(gutter + 2) as usize;
    let spans = app.file_spans().cloned().unwrap_or_default();
    let source: Vec<&str> = text.lines().collect();

    let mut rendered: Vec<(usize, usize, usize)> = Vec::new(); // line, from, to
    for (n, line) in source.iter().enumerate() {
        for (i, (from, to)) in crate::syntax::wrap_ranges(line, width.max(1))
            .into_iter()
            .enumerate()
        {
            rendered.push((if i == 0 { n + 1 } else { 0 }, from, to));
        }
    }

    // The cursor is on a source line; the scroll is in rendered rows, which a
    // wrapped line spans several of. Finding the first row of the selected
    // line is what keeps the two in step.
    let height = body.height as usize;
    let sel_line = app.file_sel.min(source.len().saturating_sub(1));
    let sel_row = rendered
        .iter()
        .position(|(n, _, _)| *n == sel_line + 1)
        .unwrap_or(0);
    scroll_into_view(&mut app.file_scroll, sel_row, height, rendered.len());
    let focused = app.pane == Pane::FileView;

    for (row, i) in (app.file_scroll..rendered.len()).enumerate() {
        if row >= height {
            break;
        }
        let y = body.y + row as u16;
        let (n, from, to) = rendered[i];
        // a continuation row belongs to the last numbered one above it
        let owner = if n > 0 {
            n
        } else {
            rendered[..i]
                .iter()
                .rev()
                .find(|(m, _, _)| *m > 0)
                .map_or(1, |(m, _, _)| *m)
        };
        let line = source.get(owner - 1).map_or("", |l| &l[from..to]);

        // Every rendered row of the selected line is marked, not just the
        // first: a wrapped line is one line, and highlighting a third of it
        // would read as a different one.
        let on_cursor = owner == sel_line + 1;
        let bg = if on_cursor { theme::sel() } else { theme::bg() };
        fill(
            buf,
            Rect {
                x: body.x,
                y,
                width: body.width,
                height: 1,
            },
            bg,
        );
        let base = Style::default().bg(bg);

        if on_cursor {
            let mark = if focused {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, body.x, y, body.x + 1, "▌", base.fg(mark));
        }
        if n > 0 {
            let num = if on_cursor {
                base.fg(theme::cyan())
            } else {
                base.fg(theme::dimmer())
            };
            put_right(buf, body.x + gutter, y, &n.to_string(), num);
        }
        // Colour is per span; a stretch with none over it is ordinary text.
        let plain = if on_cursor {
            theme::bright()
        } else {
            theme::fg()
        };
        let mut cx = body.x + gutter + 2;
        let mut at = from;
        let empty = Vec::new();
        let on_line = spans.get(owner - 1).unwrap_or(&empty);

        for sp in on_line.iter().filter(|s| s.to > from && s.from < to) {
            let (sf, st) = (sp.from.max(from), sp.to.min(to));
            if sf > at {
                cx = put(
                    buf,
                    cx,
                    y,
                    area.right(),
                    &line[at - from..sf - from],
                    base.fg(plain),
                );
            }
            cx = put(
                buf,
                cx,
                y,
                area.right(),
                &line[sf - from..st - from],
                base.fg(kind_color(sp.kind)),
            );
            at = st;
        }
        if at < to {
            put(buf, cx, y, area.right(), &line[at - from..], base.fg(plain));
        }
    }

    if rendered.len() > height {
        let label = format!("{}/{}", sel_line + 1, source.len());
        put_right(
            buf,
            area.right() - 1,
            body.bottom() - 1,
            &label,
            Style::default()
                .bg(theme::bg())
                .fg(theme::dimmer())
                .add_modifier(Modifier::DIM),
        );
    }
}
