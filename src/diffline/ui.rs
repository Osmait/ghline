//! Drawing Diffline.
//!
//! Cell painting rather than widgets, and the same primitives the GitHub
//! browser uses — the two look alike because they are drawn with the same
//! hands. The palette is Catppuccin Mocha, which is what the design specified
//! and what `theme.rs` already held.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::app::{App, FinderTab, Modal, Pane};
use super::model::{Kind, State};
use crate::theme;
use crate::tui::{
    centered_over as centered, clear, fill, frame, hline, put, put_right, put_trunc, rule,
    scroll_into_view, skel_bar, vline,
};

/// The file tree's width, and the queue's. Both fixed: the diff is what the
/// screen is for, and it takes whatever is left.
const TREE_W: u16 = 32;
const QUEUE_W: u16 = 44;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    let buf = f.buffer_mut();
    clear(buf, area, theme::bg());

    if area.height < 10 || area.width < 60 {
        put(
            buf,
            0,
            0,
            area.width,
            "terminal too small",
            Style::default().fg(theme::red()).bg(theme::bg()),
        );
        return;
    }

    let header = Rect {
        x: 0,
        y: 0,
        width: area.width,
        height: 1,
    };
    let status = Rect {
        x: 0,
        y: area.height - 1,
        width: area.width,
        height: 1,
    };
    let body = Rect {
        x: 0,
        y: 1,
        width: area.width,
        height: area.height - 2,
    };

    header_bar(buf, header, app);
    hline(buf, 0, 1, area.width, theme::border());

    // The side panes give way on a narrow terminal rather than squeezing the
    // diff into nothing: reading the change is the job.
    let tree_w = if app.tree_shown && area.width >= 110 {
        TREE_W
    } else {
        0
    };
    let queue_w = if app.queue_shown && area.width >= 150 {
        QUEUE_W
    } else {
        0
    };
    let body = Rect {
        y: body.y + 1,
        height: body.height - 1,
        ..body
    };

    if tree_w > 0 {
        let r = Rect {
            width: tree_w,
            ..body
        };
        tree(buf, r, app);
        vline(buf, tree_w, body.y, body.height, theme::border());
    }
    if queue_w > 0 {
        let x = area.width - queue_w;
        vline(buf, x - 1, body.y, body.height, theme::border());
        queue(
            buf,
            Rect {
                x,
                width: queue_w,
                ..body
            },
            app,
        );
    }
    let mid_x = tree_w + u16::from(tree_w > 0);
    let mid_w = area
        .width
        .saturating_sub(mid_x)
        .saturating_sub(queue_w + u16::from(queue_w > 0));
    diff(
        buf,
        Rect {
            x: mid_x,
            width: mid_w,
            ..body
        },
        app,
    );

    // Drawn last of the body, over the diff's top edge: while the queue is
    // away this is the only thing saying how much is in it.
    if queue_w == 0 {
        queue_tab(buf, area, app);
    }

    status_bar(buf, status, app);

    match app.modal {
        Some(Modal::Finder) => finder(buf, area, app),
        Some(Modal::Palette) => palette(buf, area, app),
        Some(Modal::Comment) => comment(buf, area, app),
        Some(Modal::Agents) => agents(buf, area, app),
        Some(Modal::Themes) => themes(buf, area, app),
        Some(Modal::Deps) => deps(buf, area, app),
        Some(Modal::Help) => help(buf, area, app),
        None => {}
    }
}

/// Right-aligns a run of pieces that are not all the same colour.
///
/// `put_right` takes one style for the whole string, which is why the counts
/// were a flat grey: `+120 −80` is two facts and they are not the same fact.
/// Returns the left edge, as `put_right` does.
fn put_right_parts(buf: &mut Buffer, right_x: u16, y: u16, parts: &[(&str, Style)]) -> u16 {
    let total: u16 = parts.iter().map(|(t, _)| t.width() as u16).sum();
    let mut x = right_x.saturating_sub(total);
    let left = x;
    for (text, style) in parts {
        x = put(buf, x, y, right_x, text, *style);
    }
    left
}

/// Green for what arrived and red for what went, unless it is zero — a file
/// that added nothing should not be shouting green about it.
fn count_style(base: Style, n: u32, added: bool) -> Style {
    if n == 0 {
        return base.fg(theme::dimmer());
    }
    base.fg(if added { theme::green() } else { theme::red() })
}

/// The count of queued comments, as a tab hanging off the top edge.
///
/// Only while the queue itself is hidden. It names its own key, because a
/// pane you cannot see is a pane you have to be told how to open.
fn queue_tab(buf: &mut Buffer, area: Rect, app: &App) {
    let n = app.comments.len();
    let label = if n == 0 {
        " no comments · ␣c ".to_string()
    } else {
        format!(" ● {n} queued · ␣c ")
    };
    let w = label.width() as u16;
    if w + 4 > area.width {
        return;
    }
    // On the rule under the header, right-aligned: out of the way of the code
    // and of the file names, and on the one row that is already a border.
    let style = Style::default().fg(theme::panel()).bg(if n == 0 {
        theme::dimmer()
    } else {
        theme::yellow()
    });
    put(buf, area.width - w - 2, 1, area.width, &label, style);
}

// ------------------------------------------------------------------ header

fn header_bar(buf: &mut Buffer, area: Rect, app: &App) {
    fill(buf, area, theme::panel());
    let base = Style::default().bg(theme::panel());

    let mut x = put(
        buf,
        0,
        0,
        area.right(),
        " DIFFLINE ",
        Style::default()
            .bg(theme::yellow())
            .fg(theme::panel())
            .add_modifier(Modifier::BOLD),
    );

    x = put(buf, x + 1, 0, area.right(), "⎇ ", base.fg(theme::purple()));
    x = put_trunc(
        buf,
        x,
        0,
        area.right() / 2,
        &app.scope.to_string(),
        base.fg(theme::bright()),
    );

    // The scopes, as tabs. The one in force is inverted.
    x += 2;
    for s in &app.scopes {
        let on = *s == app.scope;
        let style = if on {
            base.bg(theme::fg()).fg(theme::panel())
        } else {
            base.fg(theme::dim())
        };
        x = put(buf, x, 0, area.right(), &format!(" {s} "), style);
        x += 1;
    }

    let (add, del) = app
        .files
        .iter()
        .fold((0u32, 0u32), |(a, d), f| (a + f.add, d + f.del));
    let dim = base.fg(theme::dimmer());
    let rest = format!(
        "  │  {} files  │  {} queued ",
        app.files.len(),
        app.comments.len()
    );
    put_right_parts(
        buf,
        area.right(),
        0,
        &[
            (&format!("+{add}"), count_style(base, add, true)),
            ("  ", dim),
            (&format!("−{del}"), count_style(base, del, false)),
            (&rest, dim),
        ],
    );
}

// -------------------------------------------------------------------- tree

fn tree(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel_alt());
    let head = Rect { height: 1, ..area };
    fill(buf, head, theme::panel());
    let hs = Style::default().bg(theme::panel()).fg(theme::dim());
    put(buf, area.x + 1, area.y, area.right(), "CHANGES", hs);
    put_right(
        buf,
        area.right() - 1,
        area.y,
        &app.files.len().to_string(),
        hs.fg(theme::dimmer()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    let list = Rect {
        y: area.y + 2,
        height: area.height.saturating_sub(2),
        ..area
    };
    let rows = list.height as usize;

    if app.files.is_empty() {
        let state = app.files_state.clone();
        if state.is_loading() {
            for row in 0..rows.min(6) {
                skel_bar(buf, list.x + 2, list.y + row as u16, 20, row, app.anim);
            }
            return;
        }
        let (msg, color) = match state.error() {
            Some(e) => (e.to_string(), theme::red()),
            None => ("nothing changed".into(), theme::dimmer()),
        };
        put_trunc(
            buf,
            list.x + 2,
            list.y,
            area.right() - 1,
            &msg,
            Style::default().bg(theme::panel_alt()).fg(color),
        );
        return;
    }

    scroll_into_view(&mut app.tree_scroll, app.file_idx, rows, app.files.len());
    let focused = app.pane == Pane::Tree;
    let mut last_dir = String::new();

    // Directories are printed as separators rather than as a real tree: a
    // diff touches few enough directories that indentation would cost a
    // column and buy nothing.
    let mut y = list.y;
    for (i, f) in app.files.iter().enumerate().skip(app.tree_scroll) {
        if y >= list.bottom() {
            break;
        }
        if f.dir() != last_dir {
            last_dir = f.dir().to_string();
            if y < list.bottom() {
                put_trunc(
                    buf,
                    list.x + 1,
                    y,
                    area.right() - 1,
                    &format!("{last_dir}/"),
                    Style::default().bg(theme::panel_alt()).fg(theme::dimmer()),
                );
                y += 1;
            }
            if y >= list.bottom() {
                break;
            }
        }

        let sel = i == app.file_idx;
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
            let mark = if focused {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }

        let status_fg = match f.status {
            super::model::Status::Added => theme::green(),
            super::model::Status::Deleted => theme::red(),
            _ => theme::cyan(),
        };
        put(
            buf,
            area.x + 2,
            y,
            area.right(),
            f.status.mark(),
            base.fg(status_fg),
        );

        // A file with notes on it carries a dot, so the tree says where the
        // work is without opening anything.
        let noted = app.comments.iter().any(|c| c.path() == f.path);
        // The dot carries "this one has notes"; the counts carry what
        // changed. Colouring the counts yellow to say both made them say
        // neither.
        let cx = put_right_parts(
            buf,
            area.right() - 1,
            y,
            &[
                (&format!("+{}", f.add), count_style(base, f.add, true)),
                (" ", base),
                (&format!("−{}", f.del), count_style(base, f.del, false)),
                (if noted { " ●" } else { "" }, base.fg(theme::yellow())),
            ],
        );
        put_trunc(
            buf,
            area.x + 4,
            y,
            cx.saturating_sub(1),
            f.name(),
            base.fg(if sel { theme::bright() } else { theme::fg() }),
        );
        y += 1;
    }
}

// -------------------------------------------------------------------- diff

fn diff(buf: &mut Buffer, area: Rect, app: &mut App) {
    let head = Rect { height: 1, ..area };
    fill(buf, head, theme::panel());
    let hs = Style::default().bg(theme::panel());
    let x = put(
        buf,
        area.x + 1,
        area.y,
        area.right(),
        "DIFF",
        hs.fg(theme::yellow()),
    );
    let right = format!(
        " blame {} │ ctx ±{} ",
        if app.blame_on { "on" } else { "off" },
        app.context
    );
    // The path gives way to the right-hand label rather than running under
    // it: knowing the blame is off matters more than the middle of a path.
    let room = area.right().saturating_sub(right.width() as u16 + 1);
    put_trunc(buf, x + 2, area.y, room, app.path(), hs.fg(theme::bright()));
    put_right(
        buf,
        area.right(),
        area.y,
        &right,
        hs.fg(if app.blame_on {
            theme::yellow()
        } else {
            theme::dimmer()
        }),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    let body = Rect {
        y: area.y + 2,
        height: area.height.saturating_sub(2),
        ..area
    };
    let rows = app.diff_rows().to_vec();

    if rows.is_empty() {
        let state = app.diff_state();
        if state.is_loading() {
            let avail = body.width.saturating_sub(12);
            let widths = [64, 40, 78, 30, 56, 70, 44];
            for row in 0..(body.height as usize).min(widths.len() * 2) {
                if row % 4 == 3 {
                    continue;
                }
                skel_bar(
                    buf,
                    body.x + 8,
                    body.y + row as u16,
                    crate::tui::pct(avail, widths[row % widths.len()]),
                    row,
                    app.anim,
                );
            }
            return;
        }
        let (msg, color) = match state.error() {
            Some(e) => (e.to_string(), theme::red()),
            None if app.files.is_empty() => ("nothing to review".into(), theme::dimmer()),
            None => ("no textual changes in this file".into(), theme::dimmer()),
        };
        put_trunc(
            buf,
            body.x + 2,
            body.y,
            area.right() - 1,
            &msg,
            Style::default().bg(theme::bg()).fg(color),
        );
        return;
    }

    if app.split {
        draw_split(buf, body, app, &rows);
        return;
    }

    let height = body.height as usize;
    // Handed back so that `H`, `M`, `L`, `^d` and the `z` commands can be
    // about the window: they are the motions that need to know its size, and
    // this is the only place it is known.
    app.view_height = height;
    scroll_into_view(&mut app.diff_scroll, app.cursor, height, rows.len());

    let (lo, hi) = app.span();
    let visual = app.visual();
    let spans = app.spans.get(app.path()).cloned().unwrap_or_default();
    let blame = app.blame_lines().cloned().unwrap_or_default();
    let blame_w: u16 = if app.blame_on { 30 } else { 0 };
    let focused = app.pane == Pane::Diff;

    for (i, row) in rows.iter().enumerate().skip(app.diff_scroll) {
        let y = body.y + (i - app.diff_scroll) as u16;
        if y >= body.bottom() {
            break;
        }
        let on_cursor = i == app.cursor;
        let in_sel = visual && i >= lo && i <= hi;

        let (mut bg, fg, sign_fg) = match row.kind {
            Kind::Added => (theme::diff_add_bg(), theme::green(), theme::green()),
            Kind::Deleted => (theme::diff_del_bg(), theme::red(), theme::red()),
            Kind::Header => (theme::panel(), theme::cyan_soft(), theme::dimmer()),
            Kind::Context => (theme::bg(), theme::fg(), theme::dimmer()),
        };
        if in_sel {
            bg = theme::sel();
        }
        if on_cursor {
            bg = theme::sel_mark_idle();
        }

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

        if on_cursor || in_sel {
            let mark = if visual {
                theme::purple()
            } else if focused {
                theme::yellow()
            } else {
                theme::sel_mark_idle()
            };
            put(buf, body.x, y, body.x + 1, "▌", base.fg(mark));
        }

        // The two gutters, old side then new, as the design has them.
        let num = |n: Option<u32>| n.map(|v| v.to_string()).unwrap_or_default();
        put_right(buf, body.x + 6, y, &num(row.old), base.fg(theme::dimmer()));
        put_right(buf, body.x + 12, y, &num(row.new), base.fg(theme::dimmer()));
        let mut cx = body.x + 13;

        if blame_w > 0 && row.kind.is_code() {
            let who = row
                .new
                .and_then(|n| blame.get(n as usize - 1))
                .map(String::as_str)
                .unwrap_or("");
            put_trunc(
                buf,
                cx,
                y,
                cx + blame_w,
                who,
                base.fg(theme::dimmer()).add_modifier(Modifier::ITALIC),
            );
        }
        cx += blame_w;

        cx = put(buf, cx, y, area.right(), row.sign(), base.fg(sign_fg));
        cx += 1;

        // A badge on the first line of each note, and only there: repeating it
        // down a twelve-line comment would be noise.
        let badge = app.comment_head_at(row);
        let end = if badge > 0 {
            let label = format!(" ● {badge} ");
            put_right(buf, area.right(), y, &label, base.fg(theme::yellow()))
        } else {
            area.right()
        };

        draw_code(
            buf,
            cx,
            y,
            end,
            Code {
                text: &row.text,
                spans: spans.get(i),
                plain: base.fg(fg),
                skip: app.hscroll,
            },
        );
    }
}

/// Side by side: what the file said on the left, what it says on the right.
///
/// The cursor is still an index into the unified rows — this only changes
/// where a row is drawn, never what it is — so selecting, commenting and the
/// anchors all work here without knowing about it.
fn draw_split(buf: &mut Buffer, body: Rect, app: &mut App, rows: &[super::model::Row]) {
    let pairs = super::model::pair_rows(rows);

    // Which line the cursor is on, in this view's units.
    let at = pairs
        .iter()
        .position(|p| {
            p.left == Some(app.cursor)
                || p.right == Some(app.cursor)
                || p.header == Some(app.cursor)
        })
        .unwrap_or(0);
    let height = body.height as usize;
    app.view_height = height;
    scroll_into_view(&mut app.diff_scroll, at, height, pairs.len());

    let (lo, hi) = app.span();
    let visual = app.visual();
    let spans = app.spans.get(app.path()).cloned().unwrap_or_default();
    let focused = app.pane == Pane::Diff;

    // Half each, and a rule down the middle so the eye knows which side it is
    // reading without counting columns.
    let half = body.width / 2;
    let mid = body.x + half;
    vline(buf, mid, body.y, body.height, theme::border_soft());

    for (n, pair) in pairs.iter().enumerate().skip(app.diff_scroll) {
        let y = body.y + (n - app.diff_scroll) as u16;
        if y >= body.bottom() {
            break;
        }

        if let Some(i) = pair.header {
            let row = &rows[i];
            fill(
                buf,
                Rect {
                    x: body.x,
                    y,
                    width: body.width,
                    height: 1,
                },
                theme::panel(),
            );
            put_trunc(
                buf,
                body.x + 1,
                y,
                body.right(),
                &row.text,
                Style::default().bg(theme::panel()).fg(theme::cyan_soft()),
            );
            continue;
        }

        for (side, at_x, width, is_left) in [
            (pair.left, body.x, half, true),
            (pair.right, mid + 1, body.right() - mid - 1, false),
        ] {
            let Some(i) = side else {
                // Nothing was here. Painted rather than left as background so
                // the gap reads as part of the diff and not as the end of it.
                fill(
                    buf,
                    Rect {
                        x: at_x,
                        y,
                        width,
                        height: 1,
                    },
                    theme::panel_alt(),
                );
                continue;
            };
            let row = &rows[i];
            let on_cursor = i == app.cursor;
            let in_sel = visual && i >= lo && i <= hi;

            let (mut bg, fg) = match row.kind {
                Kind::Added => (theme::diff_add_bg(), theme::green()),
                Kind::Deleted => (theme::diff_del_bg(), theme::red()),
                _ => (theme::bg(), theme::fg()),
            };
            if in_sel {
                bg = theme::sel();
            }
            if on_cursor {
                bg = theme::sel_mark_idle();
            }
            fill(
                buf,
                Rect {
                    x: at_x,
                    y,
                    width,
                    height: 1,
                },
                bg,
            );
            let base = Style::default().bg(bg);

            if on_cursor || in_sel {
                let mark = if visual {
                    theme::purple()
                } else if focused {
                    theme::yellow()
                } else {
                    theme::sel_mark_idle()
                };
                put(buf, at_x, y, at_x + 1, "▌", base.fg(mark));
            }

            // One gutter per side, and each side shows its own number: a
            // context line is line 5 on the left and line 6 on the right once
            // something above it has been added, and saying 5 twice would
            // quietly misreport where the new file's line actually is.
            let num = if is_left { row.old } else { row.new }
                .map(|v| v.to_string())
                .unwrap_or_default();
            put_right(buf, at_x + 6, y, &num, base.fg(theme::dimmer()));

            let badge = app.comment_head_at(row);
            let end = if badge > 0 {
                put_right(
                    buf,
                    at_x + width,
                    y,
                    &format!(" ● {badge} "),
                    base.fg(theme::yellow()),
                )
            } else {
                at_x + width
            };

            draw_code(
                buf,
                at_x + 7,
                y,
                end,
                Code {
                    text: &row.text,
                    spans: spans.get(i),
                    plain: base.fg(fg),
                    skip: app.hscroll,
                },
            );
        }
    }
}

/// One line of code, coloured by the lexer where it has something to say.
///
/// A line too long for the pane is cut and *marked*. Without the mark a cut
/// line reads as a line that ends there, which is a different program's code —
/// and there is no horizontal scroll here to reveal the rest.
/// What one line of code needs to draw itself, which is more than a signature
/// wants to carry loose.
#[derive(Clone, Copy)]
struct Code<'a> {
    text: &'a str,
    spans: Option<&'a Vec<crate::syntax::Span>>,
    plain: Style,
    /// Columns the pane is scrolled right by.
    skip: usize,
}

fn draw_code(buf: &mut Buffer, x: u16, y: u16, max: u16, code: Code<'_>) {
    let Code {
        text,
        spans,
        plain,
        skip,
    } = code;
    // Where the visible part of the line starts. Everything below counts in
    // bytes from the start of the whole line, not of the visible part, because
    // that is what the colour spans are written in.
    let start = skip_cols(text, skip);

    let Some(spans) = spans.filter(|s| !s.is_empty()) else {
        put_trunc(buf, x, y, max, &text[start..], plain);
        return;
    };

    // One column is held back for the mark when there is more than fits.
    let room = max.saturating_sub(x) as usize;
    let cut = cut_at(&text[start..], room.saturating_sub(1));
    let end = start + cut.unwrap_or(text.len() - start);
    let limit = if cut.is_some() {
        max.saturating_sub(1)
    } else {
        max
    };

    let mut cx = x;
    // Starting at `start` rather than 0 is what drops the scrolled-off spans:
    // one that ends before it collapses to an empty range below.
    let mut at = start;
    for s in spans {
        if s.from >= end {
            break;
        }
        // Clamped rather than trusted: a span that started before the cursor
        // would otherwise be drawn twice, which reads as doubled text.
        let (from, to) = (s.from.max(at), s.to.min(end));
        if to <= from {
            continue;
        }
        if from > at {
            cx = put(buf, cx, y, limit, &text[at..from], plain);
        }
        cx = put(
            buf,
            cx,
            y,
            limit,
            &text[from..to],
            plain.fg(kind_color(s.kind)),
        );
        at = to;
    }
    if at < end {
        cx = put(buf, cx, y, limit, &text[at..end], plain);
    }
    if cut.is_some() {
        put(buf, cx, y, max, "…", plain.fg(theme::dimmer()));
    }
}

/// The byte index at which `text` would exceed `cols` columns, if it does.
///
/// A byte index rather than a character count because the colour spans are
/// written in bytes, and a cut landing inside a character would slice a string
/// Rust will not slice.
/// The byte index `cols` columns into `text`, or its end if it is shorter.
///
/// The counterpart of `cut_at`: that one finds where to stop, this one finds
/// where to start.
fn skip_cols(text: &str, cols: usize) -> usize {
    if cols == 0 {
        return 0;
    }
    let mut used = 0usize;
    for (i, c) in text.char_indices() {
        if used >= cols {
            return i;
        }
        used += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
    }
    text.len()
}

fn cut_at(text: &str, cols: usize) -> Option<usize> {
    let mut used = 0usize;
    for (i, c) in text.char_indices() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > cols {
            return Some(i);
        }
        used += w;
    }
    None
}

fn kind_color(kind: crate::syntax::Kind) -> ratatui::style::Color {
    use crate::syntax::Kind as K;
    match kind {
        K::Comment => theme::dimmer(),
        K::Str => theme::green(),
        K::Number => theme::orange(),
        K::Keyword => theme::purple(),
        K::Type => theme::cyan_soft(),
    }
}

// ------------------------------------------------------------------- queue

fn queue(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel_alt());
    let head = Rect { height: 1, ..area };
    fill(buf, head, theme::panel());
    let hs = Style::default().bg(theme::panel());
    put(
        buf,
        area.x + 1,
        area.y,
        area.right(),
        "REVIEW QUEUE",
        hs.fg(theme::yellow()),
    );
    put_right(
        buf,
        area.right() - 1,
        area.y,
        &app.comments.len().to_string(),
        hs.fg(theme::dimmer()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    // The footer names the target and what sending would do.
    let foot_y = area.bottom() - 2;
    hline(buf, area.x, foot_y - 1, area.width, theme::border_soft());
    let fs = Style::default().bg(theme::panel_alt());
    // A pending new agent wins over the running one: it is what sending would
    // actually reach, and a footer naming the other would be a lie at exactly
    // the moment it is being read.
    let (who, dot) = match (&app.new_kind, app.agent()) {
        (Some(kind), _) => (format!("a new {kind} · here"), theme::green()),
        (None, Some(a)) => (
            format!("{} · {}", a.kind, a.where_short()),
            match a.status {
                crate::mux::AgentStatus::Working => theme::yellow(),
                crate::mux::AgentStatus::Blocked => theme::red(),
                crate::mux::AgentStatus::Idle | crate::mux::AgentStatus::Done => theme::green(),
                crate::mux::AgentStatus::Unknown => theme::dimmer(),
            },
        ),
        (None, None) => ("no agent — press a".into(), theme::dimmer()),
    };
    put(buf, area.x + 1, foot_y, area.right(), "●", fs.fg(dot));
    put_trunc(
        buf,
        area.x + 3,
        foot_y,
        area.right() - 6,
        &who,
        fs.fg(theme::fg()),
    );
    put_right(buf, area.right() - 1, foot_y, "a", fs.fg(theme::dimmer()));

    let send = format!(" ⏎ S · send {} ", app.comments.len());
    let ready = !app.comments.is_empty();
    put(
        buf,
        area.x + 1,
        area.bottom() - 1,
        area.right(),
        &send,
        Style::default()
            .bg(if ready {
                theme::yellow()
            } else {
                theme::panel()
            })
            .fg(if ready {
                theme::panel()
            } else {
                theme::dimmer()
            }),
    );

    let list = Rect {
        y: area.y + 2,
        height: foot_y.saturating_sub(area.y + 3),
        ..area
    };

    if app.comments.is_empty() && app.replies.is_empty() {
        for (n, line) in [
            "No comments yet.",
            "",
            "Move to a line and press c.",
            "V first to take a range.",
        ]
        .iter()
        .enumerate()
        {
            put_trunc(
                buf,
                list.x + 2,
                list.y + n as u16,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::dimmer()),
            );
        }
        return;
    }

    let focused = app.pane == Pane::Queue;
    let mut y = list.y;
    for (i, c) in app.comments.iter().enumerate() {
        if y + 2 >= list.bottom() {
            break;
        }
        let sel = focused && i == app.queue_sel;
        let border = match c.state {
            State::Sending | State::Sent => theme::green(),
            State::Queued if sel => theme::yellow(),
            State::Queued => theme::border(),
        };
        let base = Style::default().bg(theme::bg());
        fill(
            buf,
            Rect {
                x: list.x,
                y,
                width: list.width,
                height: 3,
            },
            theme::bg(),
        );
        put(buf, list.x, y, area.right(), "▌", base.fg(border));

        put(
            buf,
            list.x + 2,
            y,
            area.right(),
            &format!("#{}", i + 1),
            base.fg(theme::yellow()),
        );
        let state = match c.state {
            State::Queued => "queued",
            State::Sending => "sending →",
            State::Sent => "sent",
        };
        let sx = put_right(buf, area.right() - 1, y, state, base.fg(border));
        put_trunc(
            buf,
            list.x + 6,
            y,
            sx.saturating_sub(1),
            &c.where_label(),
            base.fg(theme::dim()),
        );

        put_trunc(
            buf,
            list.x + 2,
            y + 1,
            area.right() - 1,
            &c.snippet,
            base.fg(theme::dimmer()),
        );
        put_trunc(
            buf,
            list.x + 2,
            y + 2,
            area.right() - 1,
            &c.body,
            base.fg(theme::fg()),
        );
        y += 4;
    }

    for reply in &app.replies {
        for line in reply.lines() {
            if y >= list.bottom() {
                return;
            }
            put_trunc(
                buf,
                list.x + 2,
                y,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::green()),
            );
            y += 1;
        }
        y += 1;
    }
}

// ------------------------------------------------------------------ status

fn status_bar(buf: &mut Buffer, area: Rect, app: &App) {
    fill(buf, area, theme::panel());
    let base = Style::default().bg(theme::panel());

    let (mode, mode_bg) = match app.modal {
        Some(Modal::Comment) => ("INSERT", theme::green()),
        Some(_) => ("SEARCH", theme::purple()),
        None if app.visual() => ("VISUAL LINE", theme::cyan()),
        None => ("NORMAL", theme::yellow()),
    };
    // What has been typed and not yet resolved, shown where vim shows it.
    // A count or a half-finished prefix is a keystroke the reader is in the
    // middle of, and not showing it is how you end up pressing it twice.
    let pending = {
        let mut p = app.count.map(|n| n.to_string()).unwrap_or_default();
        p.push_str(match app.pending {
            super::app::Pending::Leader => "␣",
            super::app::Pending::G => "g",
            super::app::Pending::Z => "z",
            super::app::Pending::Bracket(crate::nav::Dir::Prev) => "[",
            super::app::Pending::Bracket(crate::nav::Dir::Next) => "]",
            super::app::Pending::None => "",
        });
        p
    };
    let mut x = put(
        buf,
        0,
        area.y,
        area.right(),
        &format!(" {mode} "),
        Style::default()
            .bg(mode_bg)
            .fg(theme::panel())
            .add_modifier(Modifier::BOLD),
    );

    x = put_trunc(
        buf,
        x + 1,
        area.y,
        area.right() / 2,
        app.path(),
        base.fg(theme::dim()),
    );

    let (lo, hi) = app.span();
    let pos = if app.visual() {
        format!("{} lines selected", hi - lo + 1)
    } else {
        format!("{}/{}", app.cursor + 1, app.diff_rows().len())
    };
    put(
        buf,
        x + 2,
        area.y,
        area.right(),
        &pos,
        base.fg(theme::dimmer()),
    );

    let hint = if app.visual() {
        "any motion extends · ␣n note on range · o other end · esc cancel"
    } else {
        "j/k move · }/]c hunk/change · ␣ leader · : commands · ␣? help"
    };
    let toast = format!(" {} ", app.toast);
    let tx = put_right(
        buf,
        area.right(),
        area.y,
        &toast,
        Style::default().bg(theme::sel()).fg(theme::yellow()),
    );
    // Where vim puts it: right of the hint, left of everything else.
    let hx = if pending.is_empty() {
        tx
    } else {
        put_right(
            buf,
            tx.saturating_sub(2),
            area.y,
            &pending,
            base.fg(theme::bright()),
        )
    };
    put_right(
        buf,
        hx.saturating_sub(2),
        area.y,
        hint,
        base.fg(theme::dimmer()),
    );
}

// ------------------------------------------------------------------ modals

/// The query line every searching modal starts with.
fn query_line(buf: &mut Buffer, m: Rect, y: u16, app: &App, lead: &str, placeholder: &str) {
    let base = Style::default().bg(theme::panel());
    let x = put(
        buf,
        m.x + 2,
        y,
        m.right() - 2,
        lead,
        base.fg(theme::yellow()),
    );
    if app.query.is_empty() {
        put_trunc(
            buf,
            x,
            y,
            m.right() - 2,
            placeholder,
            base.fg(theme::dimmer()),
        );
    } else {
        let end = put_trunc(
            buf,
            x,
            y,
            m.right() - 2,
            &app.query,
            base.fg(theme::bright()),
        );
        if app.blink {
            put(buf, end, y, m.right() - 2, "█", base.fg(theme::yellow()));
        }
    }
}

fn finder(buf: &mut Buffer, area: Rect, app: &App) {
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
    query_line(buf, m, m.y + 3, app, "❯ ", "fuzzy find…");
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
        let rows = app.rows.get(path).cloned().unwrap_or_default();
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

fn palette(buf: &mut Buffer, area: Rect, app: &App) {
    let hits = app.palette_hits();
    let h = (hits.len() as u16 + 6).min(area.height.saturating_sub(4));
    let m = centered(area, 64, h);
    frame(buf, m, theme::purple());

    query_line(buf, m, m.y + 1, app, ": ", "command…");
    rule(buf, m, m.y + 2, theme::border());

    for (n, label) in hits.iter().enumerate() {
        let y = m.y + 3 + n as u16;
        if y >= m.bottom() - 1 {
            break;
        }
        let sel = n == app.sel;
        let bg = if sel { theme::sel() } else { theme::panel() };
        fill(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 1,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if sel {
            put(buf, m.x + 1, y, m.right(), "▌", s.fg(theme::purple()));
        }
        put_trunc(buf, m.x + 3, y, m.right() - 8, label, s.fg(theme::fg()));
        let key = super::input::COMMANDS
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, k)| *k)
            .unwrap_or("");
        put_right(buf, m.right() - 2, y, key, s.fg(theme::yellow()));
    }
}

fn comment(buf: &mut Buffer, area: Rect, app: &App) {
    let anchors = app.selected_anchors();
    let m = centered(area, 72, 9);
    frame(buf, m, theme::yellow());
    let base = Style::default().bg(theme::panel());

    let head = Rect {
        x: m.x + 1,
        y: m.y + 1,
        width: m.width - 2,
        height: 1,
    };
    fill(buf, head, theme::yellow());
    let hs = Style::default().bg(theme::yellow()).fg(theme::panel());
    put(buf, head.x + 1, head.y, head.right(), "COMMENT", hs);
    let where_ = match (anchors.first(), anchors.last()) {
        (Some(a), Some(b)) if a.line == b.line => format!("{}:{}", short_path(&a.path), a.line),
        (Some(a), Some(b)) => format!(
            "{}:{}-{}  ({} lines)",
            short_path(&a.path),
            a.line.min(b.line),
            a.line.max(b.line),
            anchors.len()
        ),
        _ => "—".into(),
    };
    put_right(buf, head.right() - 1, head.y, &where_, hs);

    let snippet = app
        .diff_rows()
        .get(app.span().0)
        .map(|r| r.text.trim())
        .unwrap_or("");
    put_trunc(
        buf,
        m.x + 3,
        m.y + 3,
        m.right() - 2,
        snippet,
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 4, theme::border());

    let x = put(
        buf,
        m.x + 2,
        m.y + 5,
        m.right() - 2,
        "❯ ",
        base.fg(theme::yellow()),
    );
    if app.draft.is_empty() {
        put_trunc(
            buf,
            x,
            m.y + 5,
            m.right() - 2,
            "what should the agent do here?",
            base.fg(theme::dimmer()),
        );
    } else {
        let end = put_trunc(
            buf,
            x,
            m.y + 5,
            m.right() - 2,
            &app.draft,
            base.fg(theme::bright()),
        );
        if app.blink {
            put(
                buf,
                end,
                m.y + 5,
                m.right() - 2,
                "█",
                base.fg(theme::yellow()),
            );
        }
    }

    rule(buf, m, m.bottom() - 3, theme::border());
    put(
        buf,
        m.x + 2,
        m.bottom() - 2,
        m.right() - 2,
        "↵ save to queue · esc discard",
        base.fg(theme::dimmer()),
    );
}

fn short_path(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn agents(buf: &mut Buffer, area: Rect, app: &App) {
    let kinds = app.agent_choices().len() - app.agents.len();
    let rows = app.agents.len() as u16 * 2 + kinds as u16 + 1;
    let h = (rows + 6).min(area.height.saturating_sub(4));
    let m = centered(area, 76, h.max(7));
    frame(buf, m, theme::cyan());
    let base = Style::default().bg(theme::panel());

    put(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        "AGENTS ON THIS MACHINE",
        base.fg(theme::yellow()),
    );
    put_right(
        buf,
        m.right() - 2,
        m.y + 1,
        "via herdr",
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 2, theme::border());

    let mut y = m.y + 3;

    if app.agents.is_empty() {
        let msg = match app.agents_state.error() {
            Some(e) => e.to_string(),
            None if app.agents_state.is_loading() => "looking…".into(),
            None => "none running yet".into(),
        };
        put_trunc(
            buf,
            m.x + 3,
            y,
            m.right() - 2,
            &msg,
            base.fg(theme::dimmer()),
        );
        y += 2;
    }

    for (i, a) in app.agents.iter().enumerate() {
        if y + 1 >= m.bottom() - 1 {
            break;
        }
        let sel = i == app.sel;
        let bg = if sel { theme::sel() } else { theme::panel() };
        fill(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 2,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if i == app.agent_idx && app.new_kind.is_none() {
            put(buf, m.x + 1, y, m.right(), "▌", s.fg(theme::yellow()));
        }
        let dot = match a.status {
            crate::mux::AgentStatus::Working => theme::yellow(),
            crate::mux::AgentStatus::Blocked => theme::red(),
            crate::mux::AgentStatus::Idle | crate::mux::AgentStatus::Done => theme::green(),
            crate::mux::AgentStatus::Unknown => theme::dimmer(),
        };
        put(buf, m.x + 3, y, m.right(), "●", s.fg(dot));
        put(
            buf,
            m.x + 5,
            y,
            m.right(),
            &crate::config::agent_icon(&a.kind),
            s.fg(theme::purple()),
        );
        put_trunc(
            buf,
            m.x + 7,
            y,
            m.right() - 14,
            &a.kind,
            s.fg(theme::bright()),
        );
        put_right(buf, m.right() - 2, y, a.status.label(), s.fg(dot));
        put_trunc(
            buf,
            m.x + 7,
            y + 1,
            m.right() - 2,
            &a.cwd,
            s.fg(theme::dimmer()),
        );
        y += 2;
    }

    // Below the line, the ones that are not running: picking one of these
    // starts it in this repository when the queue is sent, so an agent is
    // never opened for a review that then does not happen.
    if y + 1 < m.bottom() - 1 {
        rule(buf, m, y, theme::border());
        put(
            buf,
            m.x + 3,
            y,
            m.right() - 2,
            " start a new one here ",
            base.fg(theme::dimmer()),
        );
        y += 1;
    }

    let running = app.agents.len();
    for (i, (kind, _)) in app.agent_choices().iter().enumerate().skip(running) {
        if y >= m.bottom() - 1 {
            break;
        }
        let bg = if i == app.sel {
            theme::sel()
        } else {
            theme::panel()
        };
        fill(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 1,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if app.new_kind.as_deref() == Some(kind.as_str()) {
            put(buf, m.x + 1, y, m.right(), "▌", s.fg(theme::yellow()));
        }
        put(buf, m.x + 3, y, m.right(), "+", s.fg(theme::green()));
        put(
            buf,
            m.x + 5,
            y,
            m.right(),
            &crate::config::agent_icon(kind),
            s.fg(theme::purple()),
        );
        put_trunc(buf, m.x + 7, y, m.right() - 2, kind, s.fg(theme::bright()));
        y += 1;
    }
}

/// The theme picker. Small on purpose — it sits over the diff, and the diff
/// is what you are actually judging the colours against.
fn themes(buf: &mut Buffer, area: Rect, app: &App) {
    let all = crate::theme::Theme::all();
    let m = centered(area, 60, (all.len() as u16 * 2 + 5).min(area.height - 2));
    frame(buf, m, theme::cyan());
    let base = Style::default().bg(theme::panel());
    put(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        "THEME",
        base.fg(theme::yellow()),
    );
    put_right(
        buf,
        m.right() - 2,
        m.y + 1,
        "⏎ keep · esc undo",
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 2, theme::border());

    for (i, t) in all.iter().enumerate() {
        let y = m.y + 3 + i as u16 * 2;
        if y + 1 >= m.bottom() {
            break;
        }
        let bg = if i == app.sel {
            theme::sel()
        } else {
            theme::panel()
        };
        fill(
            buf,
            Rect {
                x: m.x + 1,
                y,
                width: m.width - 2,
                height: 2,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if i == app.sel {
            put(buf, m.x + 1, y, m.right(), "▌", s.fg(theme::yellow()));
        }
        put_trunc(
            buf,
            m.x + 3,
            y,
            m.right() - 14,
            t.name(),
            s.fg(theme::bright()),
        );
        put_trunc(
            buf,
            m.x + 3,
            y + 1,
            m.right() - 2,
            t.about(),
            s.fg(theme::dimmer()),
        );
        // A row of the accents, so the list shows the theme rather than
        // naming it — the picker is already painted in whichever is selected.
        let mut x = m.right() - 12;
        for c in [
            theme::green(),
            theme::yellow(),
            theme::red(),
            theme::cyan(),
            theme::purple(),
        ] {
            x = put(buf, x, y, m.right() - 2, "██", s.fg(c));
        }
    }
}

fn deps(buf: &mut Buffer, area: Rect, app: &App) {
    let m = centered(area, 86, 14);
    frame(buf, m, theme::cyan());
    let base = Style::default().bg(theme::panel());
    put_trunc(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        &format!("BLAST RADIUS — {}", app.path()),
        base.fg(theme::cyan()),
    );
    rule(buf, m, m.y + 2, theme::border());

    // Said rather than guessed. An import graph needs a parser per language,
    // which is the trade this program declined for colour and declines again
    // here; claiming to know what depends on a file when nothing has been
    // parsed would be worse than saying so.
    for (n, line) in [
        "No import graph.",
        "",
        "Working it out means a parser per language, which is the",
        "same cost this program declined for syntax colour. Until",
        "there is one, the honest answer is that it does not know.",
        "",
        "What it can tell you is what else this change touches:",
    ]
    .iter()
    .enumerate()
    {
        put_trunc(
            buf,
            m.x + 3,
            m.y + 4 + n as u16,
            m.right() - 2,
            line,
            base.fg(if n == 0 { theme::fg() } else { theme::dimmer() }),
        );
    }
    for (y, f) in (m.y + 11..m.bottom() - 1).zip(app.files.iter().take(2)) {
        put_trunc(
            buf,
            m.x + 5,
            y,
            m.right() - 2,
            &format!("└─ {}", f.path),
            base.fg(theme::fg()),
        );
    }
}

/// Width of the key column in the help. Named because two places depend on
/// it agreeing.
const KEY_W: u16 = 14;

/// The keymap, read off the keymap.
///
/// Generated rather than written down: a help that is a second list of the
/// bindings is a help that is wrong the first time somebody rebinds a key,
/// and being wrong about that is worse than not being there.
fn help(buf: &mut Buffer, area: Rect, app: &App) {
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

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::diffline::model::{ChangedFile, Kind, Row, Scope, Status};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// A file whose lines are far longer than any pane.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
        );
        a.service = None;
        a.files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: Status::Added,
            add: 3,
            del: 0,
        }];
        a.files_state = crate::diffline::app::Load::Ready;
        let long = "// ".to_string() + &"MARKER_".repeat(40);
        a.rows.insert(
            "src/a.rs".into(),
            vec![
                Row {
                    kind: Kind::Header,
                    old: None,
                    new: None,
                    text: "@@ -0,0 +1,3 @@".into(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(1),
                    text: long.clone(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(2),
                    text: long,
                },
            ],
        );
        a.rows_state
            .insert("src/a.rs".into(), crate::diffline::app::Load::Ready);
        // Coloured, as the real thing is: the uncoloured path and the
        // coloured one write the line differently, and only one of them was
        // being exercised before.
        let rows = a.rows["src/a.rs"].clone();
        let spans = rows
            .iter()
            .map(|r| {
                crate::syntax::of_path("a.rs")
                    .map(|l| {
                        crate::syntax::highlight(l, &r.text)
                            .pop()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
            })
            .collect();
        a.spans.insert("src/a.rs".into(), spans);
        a.cursor = 1;
        a
    }

    /// Every row of the screen, as one string.
    fn rows(term: &Terminal<TestBackend>) -> Vec<String> {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()).to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_line_that_fits_is_not_cut() {
        assert_eq!(cut_at("hello", 20), None);
        assert_eq!(cut_at("hello", 5), None, "exactly is still fits");
    }

    #[test]
    fn a_cut_lands_on_a_character_boundary() {
        // slicing a string mid-character is a panic, not a display bug
        let text = "日本語のコメントがここにある";
        for cols in 0..30 {
            if let Some(i) = cut_at(text, cols) {
                assert!(text.is_char_boundary(i), "cut at {i} for {cols} columns");
            }
        }
    }

    #[test]
    fn a_wide_character_is_not_half_drawn() {
        // two columns each: three of them do not fit in five
        assert_eq!(cut_at("漢漢漢", 5), Some("漢漢".len()));
    }

    #[test]
    fn no_room_at_all_cuts_at_the_start() {
        assert_eq!(cut_at("anything", 0), Some(0));
    }

    #[test]
    fn a_long_line_never_reaches_the_queue_pane() {
        // The bug this exists for: diff text drawn past its own pane lands on
        // top of the queue, which is drawn before it.
        for width in [150u16, 160, 170, 200, 240] {
            let mut a = app();
            // The invariant is about overwriting the queue, so there has to be
            // one: it is hidden until asked for now.
            a.queue_shown = true;
            let mut term = Terminal::new(TestBackend::new(width, 20)).unwrap();
            term.draw(|f| draw(f, &mut a)).unwrap();

            // the queue occupies the rightmost QUEUE_W columns
            let queue_x = (width - QUEUE_W) as usize;
            for (y, row) in rows(&term).iter().enumerate() {
                let tail: String = row.chars().skip(queue_x).collect();
                assert!(
                    !tail.contains("MARKER"),
                    "at width {width}, row {y} put diff text in the queue:\n  {}",
                    row.trim_end()
                );
            }
        }
    }

    #[test]
    fn the_tab_says_how_many_are_queued_while_the_queue_is_away() {
        // The queue is hidden to begin with, so this count is the only thing
        // saying there is anything in it at all.
        let mut a = app();
        a.queue_shown = false;
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term).join("\n");
        assert!(screen.contains("no comments"), "{screen}");

        a.comments.push(super::super::model::Comment {
            anchors: vec![],
            file: "src/a.rs".into(),
            snippet: "fn main() {".into(),
            body: "look at this".into(),
            state: State::Queued,
        });
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term).join("\n");
        assert!(screen.contains("1 queued"), "{screen}");

        // and it gets out of the way once the queue itself is on screen
        a.queue_shown = true;
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term).join("\n");
        assert!(!screen.contains("queued · "), "{screen}");
    }

    #[test]
    fn each_side_of_the_split_numbers_its_own_file() {
        // A context line below an insertion is line 2 on the left and 3 on
        // the right. Showing the old number on both sides would misreport
        // where the new file's line actually is — and a reader writing a
        // comment off that number would anchor it to the wrong line.
        let mut a = app();
        a.split = true;
        a.rows.insert(
            "src/a.rs".into(),
            vec![
                Row {
                    kind: Kind::Context,
                    old: Some(1),
                    new: Some(1),
                    text: "fn main() {".into(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(2),
                    text: "    added();".into(),
                },
                Row {
                    kind: Kind::Context,
                    old: Some(2),
                    new: Some(3),
                    text: "CLOSING".into(),
                },
            ],
        );

        let mut term = Terminal::new(TestBackend::new(150, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term);
        let rendered = screen.join("\n");

        let row = screen
            .iter()
            .find(|r| r.matches("CLOSING").count() == 2)
            .unwrap_or_else(|| panic!("the line should be drawn on both sides:\n{rendered}"));
        let half = row.len() / 2;
        assert!(
            row[..half].contains('2'),
            "the left half should carry the old number 2:\n  {row}"
        );
        assert!(
            row[half..].contains('3'),
            "the right half should carry the new number 3:\n  {row}"
        );
    }

    #[test]
    fn the_counts_are_green_and_red_and_a_zero_is_neither() {
        let mut a = app();
        a.files = vec![
            ChangedFile {
                path: "src/a.rs".into(),
                status: Status::Added,
                add: 12,
                del: 0,
            },
            ChangedFile {
                path: "src/b.rs".into(),
                status: Status::Modified,
                add: 0,
                del: 7,
            },
        ];
        let mut term = Terminal::new(TestBackend::new(150, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let buf = term.backend().buffer();

        // Walk the cells and collect the colour each run of digits was in,
        // keyed by the sign in front of it.
        let mut seen: Vec<(char, ratatui::style::Color)> = Vec::new();
        for y in 0..buf.area.height {
            let mut sign = None;
            for x in 0..buf.area.width {
                let Some(cell) = buf.cell((x, y)) else {
                    continue;
                };
                match cell.symbol() {
                    "+" => sign = Some('+'),
                    "−" => sign = Some('-'),
                    sym if sym.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                        if let Some(s) = sign.take()
                            && let Some(fg) = cell.fg.into()
                        {
                            seen.push((s, fg));
                        }
                    }
                    _ => sign = None,
                }
            }
        }

        assert!(
            seen.iter().any(|(s, c)| *s == '+' && *c == theme::green()),
            "an addition count should be green: {seen:?}"
        );
        assert!(
            seen.iter().any(|(s, c)| *s == '-' && *c == theme::red()),
            "a deletion count should be red: {seen:?}"
        );
        assert!(
            seen.iter().any(|(_, c)| *c == theme::dimmer()),
            "a zero should stay quiet rather than shout its colour: {seen:?}"
        );
    }

    #[test]
    fn a_cut_line_says_it_was_cut() {
        // Without a mark, a line that stops at the pane edge reads as a line
        // that ends there — which is a different program's code.
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();

        let cut = rows(&term)
            .into_iter()
            .find(|r| r.contains("MARKER"))
            .expect("the long line should be on screen");
        assert!(
            cut.contains('…'),
            "no ellipsis on a line that was cut:\n  {}",
            cut.trim_end()
        );
    }

    #[test]
    fn the_panes_give_way_before_the_diff_does() {
        // narrow enough that the side panes have to go
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term).join("\n");
        assert!(
            screen.contains("MARKER"),
            "the diff is the point and should survive a narrow terminal"
        );
    }
}
