//! The diff itself, unified and side by side.
//!
//! The one pane that is the point of the program, and the only one whose
//! drawing is not a list: it carries syntax colour, a blame column, comment
//! badges, a cursor and a horizontal scroll, and any of those can be off.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Pane};
use crate::hit::{Region, Target};
use crate::model::Kind;
use crate::tui::theme;
use crate::tui::{fill, hline, put, put_right, put_trunc, scroll_into_view, skel_bar, vline};

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
    spans: Option<&'a Vec<crate::shared::syntax::Span>>,
    plain: Style,
    /// Columns the pane is scrolled right by.
    skip: usize,
}

pub(super) fn diff(buf: &mut Buffer, area: Rect, app: &mut App) {
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
    // Counted rather than copied. This used to be `.to_vec()`, which deep
    // copies every row and its text on every frame — eight thousand `String`
    // allocations to draw the forty that fit. It was copied because the
    // mutations below borrow `app` mutably while the loop wants it
    // immutably, and the way out of that is to do the mutations first.
    let len = app.diff_rows().len();

    if len == 0 {
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
        draw_split(buf, body, app);
        return;
    }

    let height = body.height as usize;
    // Handed back so that `H`, `M`, `L`, `^d` and the `z` commands can be
    // about the window: they are the motions that need to know its size, and
    // this is the only place it is known.
    app.view_height = height;
    let cursor = app.cursor;
    scroll_into_view(&mut app.diff_scroll, cursor, height, len);
    let scroll = app.diff_scroll;
    app.hits
        .push(Region::rows(Target::Pane(Pane::Diff), body, 1, scroll, len));

    // Everything mutable is done with; from here the borrows are shared and
    // nothing is copied.
    let app = &*app;
    let (lo, hi) = app.span();
    let visual = app.visual();
    let empty_spans = Vec::new();
    let spans = app.spans.get(app.path()).unwrap_or(&empty_spans);
    let empty_blame = Vec::new();
    let blame = app.blame_lines().unwrap_or(&empty_blame);
    let blame_w: u16 = if app.blame_on { 30 } else { 0 };
    let focused = app.pane == Pane::Diff;

    for (i, row) in app.diff_rows().iter().enumerate().skip(scroll) {
        let y = body.y + (i - scroll) as u16;
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
fn draw_split(buf: &mut Buffer, body: Rect, app: &mut App) {
    // The pairs are indices, so this is the one thing here worth building:
    // a `Vec<Pair>` of three `Option<usize>` each, not a copy of the text.
    let pairs = crate::model::pair_rows(app.diff_rows());

    // Which line the cursor is on, in this view's units.
    let cursor = app.cursor;
    let at = pairs
        .iter()
        .position(|p| p.left == Some(cursor) || p.right == Some(cursor) || p.header == Some(cursor))
        .unwrap_or(0);
    let height = body.height as usize;
    app.view_height = height;
    scroll_into_view(&mut app.diff_scroll, at, height, pairs.len());
    let scroll = app.diff_scroll;

    // Shared borrows from here, so nothing is copied.
    let app = &*app;
    let rows = app.diff_rows();
    let (lo, hi) = app.span();
    let visual = app.visual();
    let empty_spans = Vec::new();
    let spans = app.spans.get(app.path()).unwrap_or(&empty_spans);
    let focused = app.pane == Pane::Diff;

    // Half each, and a rule down the middle so the eye knows which side it is
    // reading without counting columns.
    let half = body.width / 2;
    let mid = body.x + half;
    vline(buf, mid, body.y, body.height, theme::border_soft());

    for (n, pair) in pairs.iter().enumerate().skip(scroll) {
        let y = body.y + (n - scroll) as u16;
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

fn kind_color(kind: crate::shared::syntax::Kind) -> ratatui::style::Color {
    use crate::shared::syntax::Kind as K;
    match kind {
        K::Comment => theme::dimmer(),
        K::Str => theme::green(),
        K::Number => theme::orange(),
        K::Keyword => theme::purple(),
        K::Type => theme::cyan_soft(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;
    use crate::model::{ChangedFile, Kind, Row, Scope, Status};
    use crate::tui::probe;
    use crate::view::draw;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// A file whose lines are far longer than any pane.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
            None,
        );
        a.files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: Status::Added,
            add: 3,
            del: 0,
        }];
        a.files_state = crate::app::Load::Ready;
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
            .insert("src/a.rs".into(), crate::app::Load::Ready);
        // Coloured, as the real thing is: the uncoloured path and the
        // coloured one write the line differently, and only one of them was
        // being exercised before.
        let rows = a.rows["src/a.rs"].clone();
        let spans = rows
            .iter()
            .map(|r| {
                crate::shared::syntax::of_path("a.rs")
                    .map(|l| {
                        crate::shared::syntax::highlight(l, &r.text)
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

            // the queue occupies the rightmost crate::view::QUEUE_W columns
            let queue_x = (width - crate::view::QUEUE_W) as usize;
            for (y, row) in probe::rows(&term).iter().enumerate() {
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
        let screen = probe::rows(&term);
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
    fn a_cut_line_says_it_was_cut() {
        // Without a mark, a line that stops at the pane edge reads as a line
        // that ends there — which is a different program's code.
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();

        let cut = probe::rows(&term)
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
}
