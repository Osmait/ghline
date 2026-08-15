//! Diff view: changed files on the left and their contents on the right, in
//! unified or split mode.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::github::app::hit::{Region, Target};
use crate::github::app::{App, Pane};
use crate::github::data::{DiffKind, DiffRow};
use crate::tui::diff::{Pair, Side, pair};
use crate::tui::theme;
use crate::tui::{fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar, vline};

const FILES_W: u16 = 38;

/// Background for each line kind, as in the design.
fn row_bg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::diff_add_bg(),
        DiffKind::Del => theme::diff_del_bg(),
        DiffKind::Hdr => theme::tab_active_bg(),
        DiffKind::Ctx => theme::bg(),
    }
}

fn row_fg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::green(),
        DiffKind::Del => theme::red(),
        DiffKind::Hdr => theme::purple(),
        DiffKind::Ctx => theme::step_fg(),
    }
}

pub fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
    let files_w = FILES_W.min(area.width / 2);
    let files = Rect {
        x: area.x,
        y: area.y,
        width: files_w,
        height: area.height,
    };
    let body = Rect {
        x: area.x + files_w + 1,
        y: area.y,
        width: area.width - files_w - 1,
        height: area.height,
    };
    draw_files(buf, files, app);
    vline(buf, area.x + files_w, area.y, area.height, theme::border());
    draw_body(buf, body, app);
}

/// The changed-files pane: a fixed header and footer, rows scrolling between.
///
/// ```text
///   area.y     │ FILES CHANGED           header
///        +1    │ ─────────────────────   rule
///        +2    │ src/layout/solver.rs    the rows, one line each and the
///         ⋮    │ src/app/reducer.rs      only part that scrolls
///   bottom-2   │ ─────────────────────   rule
///   bottom-1   │ 6 files changed · …     footer, the PR's totals
/// ```
///
/// The two rules are drawn from the fixed ends inwards, so the rows get
/// whatever is left and a pane too short to hold both simply has no rows.
fn draw_files(buf: &mut Buffer, area: Rect, app: &mut App) {
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
        "FILES CHANGED",
        Style::default().bg(theme::panel()).fg(theme::dim()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    // footer with the PR total
    let foot_y = area.bottom() - 1;
    hline(buf, area.x, foot_y - 1, area.width, theme::border_soft());
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
    let stats = match app.current() {
        Some(cur) => cur.as_pr().map_or_else(String::new, |p| {
            format!("{} files changed · {} {}", p.files, p.add, p.del)
        }),
        None => String::new(),
    };
    let stats = if app.ws {
        format!("{stats} · whitespace ignored")
    } else {
        stats
    };
    put_trunc(
        buf,
        area.x + 1,
        foot_y,
        area.right() - 1,
        &stats,
        Style::default().bg(theme::panel()).fg(theme::dimmer()),
    );

    let sel = app.file_idx();
    let len = app.diff_files().len();
    if len == 0 && app.diff_status().is_loading() {
        let avail = area.width.saturating_sub(14);
        let names = [58, 44, 70, 50, 38, 62];
        for row in 0..((foot_y.saturating_sub(area.y + 3)) as usize).min(6) {
            let y = area.y + 2 + row as u16;
            skel_bar(
                buf,
                area.x + 2,
                y,
                pct(avail, names[row % names.len()]),
                row,
                app.anim,
            );
            skel_bar(buf, area.right().saturating_sub(9), y, 7, row, app.anim);
        }
        return;
    }
    let list_h = foot_y.saturating_sub(area.y + 3) as usize;
    scroll_into_view(&mut app.repo_scroll, sel, list_h, len);
    let scroll = app.repo_scroll;
    app.hits.push(Region::rows(
        Target::Pane(Pane::Files),
        Rect {
            x: area.x,
            y: area.y + 2,
            width: area.width,
            height: list_h as u16,
        },
        1,
        scroll,
        len,
    ));
    let focused = app.pane == Pane::Files;

    for (row, i) in (scroll..len).enumerate() {
        if row >= list_h {
            break;
        }
        let y = area.y + 2 + row as u16;
        let f = &app.diff_files()[i];
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

        // counts on the right; the name on the left, with its directory in
        // grey so the file itself stands out
        let del_x = put_right(buf, area.right() - 1, y, &f.del, base.fg(theme::red()));
        let add_x = put_right(buf, del_x - 1, y, &f.add, base.fg(theme::green()));

        let (dir, name) = match f.path.rfind('/') {
            Some(i) => f.path.split_at(i + 1),
            None => ("", f.path.as_str()),
        };
        let fg = if selected {
            theme::bright()
        } else {
            theme::fg()
        };
        let max = add_x.saturating_sub(1);
        // when it does not all fit, the directory is cut before the name
        let name_w = name.chars().count() as u16;
        let dir_max = max.saturating_sub(name_w).max(area.x + 2);
        let cx = put_trunc(buf, area.x + 2, y, dir_max, dir, base.fg(theme::dimmer()));
        put_trunc(buf, cx, y, max, name, base.fg(fg));
    }
}

fn draw_body(buf: &mut Buffer, area: Rect, app: &mut App) {
    // below the header row; the diff scrolls rather than selects
    app.hits.push(Region::plain(
        Target::Pane(Pane::DiffBody),
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        },
    ));
    // ---- header
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::panel());
    let base = Style::default().bg(theme::panel());

    let split_label = if app.split {
        "▥ split [s]"
    } else {
        "▤ unified [s]"
    };
    let ws_label = if app.ws {
        "● ignore ws [w]"
    } else {
        "○ ignore ws [w]"
    };
    let ws_x = put_right(
        buf,
        area.right() - 1,
        area.y,
        ws_label,
        base.fg(theme::dimmer()),
    );
    let split_x = put_right(buf, ws_x - 2, area.y, split_label, base.fg(theme::purple()));

    let rows = app.diff_rows();
    let hunks = rows.iter().filter(|r| r.kind == DiffKind::Hdr).count();
    let branch = app
        .current()
        .map(crate::github::data::Item::branch)
        .unwrap_or_default();
    let meta = if branch.is_empty() {
        format!("{hunks} hunks")
    } else {
        format!("{hunks} hunks · {branch} → main")
    };
    let meta_x = put_right(
        buf,
        split_x.saturating_sub(2),
        area.y,
        &meta,
        base.fg(theme::dimmer()),
    );
    let path = app.diff_file().map(|f| f.path.clone()).unwrap_or_default();
    put_trunc(
        buf,
        area.x + 1,
        area.y,
        meta_x.saturating_sub(1),
        &path,
        base.fg(theme::fg()),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::border_soft());

    let view = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(2),
    };
    if view.height == 0 {
        return;
    }
    app.detail_height = view.height;

    if rows.is_empty() {
        empty(buf, view, app);
        return;
    }
    if app.split {
        unified_or_split(buf, view, app, &rows, true);
    } else {
        unified_or_split(buf, view, app, &rows, false);
    }
}

/// Empty state: either the file has no textual changes, or the diff has not
/// been fetched yet.
fn empty(buf: &mut Buffer, area: Rect, app: &App) {
    let Some(f) = app.diff_file() else { return };
    let binary = f.add == "+0" && f.del == "-0";
    let st = app.diff_status();
    if st.is_loading() {
        let avail = area.width.saturating_sub(22);
        let widths = [44, 56, 34, 50, 38, 60, 29];
        for row in 0..(area.height as usize).min(16) {
            let ry = area.y + row as u16;
            skel_bar(buf, area.x + 2, ry, 3, row, app.anim);
            skel_bar(buf, area.x + 8, ry, 3, row, app.anim);
            skel_bar(
                buf,
                area.x + 14,
                ry,
                pct(avail, widths[row % widths.len()]),
                row,
                app.anim,
            );
        }
        return;
    }
    let (title, sub) = if let Some(e) = st.error() {
        ("diff unavailable".to_string(), e.to_string())
    } else if binary {
        (
            "no textual changes".to_string(),
            format!("{} · 0 hunks · mode 100644", f.path),
        )
    } else if app.live() {
        (
            "diff not fetched yet".to_string(),
            format!(
                "{} · {} {} · press r to fetch it again",
                f.path, f.add, f.del
            ),
        )
    } else {
        (
            "diff not available".to_string(),
            format!("{} · {} {}", f.path, f.add, f.del),
        )
    };

    let y = area.y + area.height / 2;
    let center = |text: &str, y: u16, style: Style, buf: &mut Buffer| {
        let w = text.chars().count() as u16;
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        put_trunc(buf, x, y, area.right(), text, style);
    };
    let base = Style::default().bg(theme::bg());
    center(&format!("— {title} —"), y, base.fg(theme::dim()), buf);
    center(&sub, y + 1, base.fg(theme::dimmer()), buf);
}

/// Both modes share the clipping and the scrolling; only the column layout
/// differs.
fn unified_or_split(buf: &mut Buffer, area: Rect, app: &mut App, rows: &[DiffRow], split: bool) {
    let pairs = if split { split_rows(rows) } else { Vec::new() };
    let len = if split { pairs.len() } else { rows.len() };
    let h = area.height as usize;
    app.diff_scroll = app.diff_scroll.min(len.saturating_sub(h));
    let scroll = app.diff_scroll;
    let focused = app.pane == Pane::DiffBody;

    for row in 0..h.min(len.saturating_sub(scroll)) {
        let y = area.y + row as u16;
        let i = scroll + row;
        if split {
            draw_split_row(buf, area, y, &pairs[i], rows);
        } else {
            draw_unified_row(buf, area, y, &rows[i]);
        }
        if focused {
            put(
                buf,
                area.x,
                y,
                area.x + 1,
                "▌",
                Style::default().bg(theme::bg()).fg(theme::cyan()),
            );
        }
    }
}

fn draw_unified_row(buf: &mut Buffer, area: Rect, y: u16, r: &DiffRow) {
    let bg = row_bg(r.kind);
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
    // two numbering columns: original file and new file
    put_right(buf, area.x + 6, y, &r.lo, base.fg(theme::gutter()));
    put_right(buf, area.x + 12, y, &r.ln, base.fg(theme::gutter()));
    put_trunc(
        buf,
        area.x + 14,
        y,
        area.right(),
        &r.text,
        base.fg(row_fg(r.kind)),
    );
}

/// One side-by-side line.
///
/// Reads its cells out of `rows` by index rather than being handed copies of
/// them: the fold works in indices now, and cloning two strings per row per
/// frame to hand them here was paying for the convenience twice.
fn draw_split_row(buf: &mut Buffer, area: Rect, y: u16, p: &Pair, rows: &[DiffRow]) {
    let half = area.width / 2;
    let left = Rect {
        x: area.x,
        y,
        width: half,
        height: 1,
    };
    let right = Rect {
        x: area.x + half,
        y,
        width: area.width - half,
        height: 1,
    };

    if let Some(hdr) = p.header.and_then(|i| rows.get(i)) {
        fill(buf, left, theme::tab_active_bg());
        fill(buf, right, theme::tab_active_bg());
        let s = Style::default()
            .bg(theme::tab_active_bg())
            .fg(theme::purple());
        put_trunc(buf, area.x + 2, y, area.right(), &hdr.text, s);
        return;
    }

    // Each side shows its own file's number: a context line below an
    // insertion is line 5 on the left and 6 on the right.
    let side = |rect: Rect, at: Option<usize>, old: bool, buf: &mut Buffer| {
        let Some(r) = at.and_then(|i| rows.get(i)) else {
            // the gap of an unbalanced pair gets a duller grey
            fill(buf, rect, theme::diff_void_bg());
            return;
        };
        let bg = row_bg(r.kind);
        fill(buf, rect, bg);
        let base = Style::default().bg(bg);
        let num = if old { &r.lo } else { &r.ln };
        put_right(buf, rect.x + 5, y, num, base.fg(theme::gutter()));
        put_trunc(
            buf,
            rect.x + 6,
            y,
            rect.right(),
            &r.text,
            base.fg(row_fg(r.kind)),
        );
    };

    side(left, p.left, true, buf);
    side(right, p.right, false, buf);
    // separator between the two halves
    let sep_bg = p
        .right
        .and_then(|i| rows.get(i))
        .map_or(DiffKind::Ctx, |r| r.kind);
    put(
        buf,
        right.x,
        y,
        right.x + 1,
        "│",
        Style::default().bg(row_bg(sep_bg)).fg(theme::border_soft()),
    );
}

/// Folds the rows into side-by-side lines.
///
/// The fold itself is `tui::diff`, shared with the other program — it was
/// written out here and there, the same algorithm twice. This is only the
/// seam: what our kinds are called on the way in.
fn split_rows(rows: &[DiffRow]) -> Vec<Pair> {
    let sides: Vec<Side> = rows
        .iter()
        .map(|r| match r.kind {
            DiffKind::Hdr => Side::Header,
            DiffKind::Ctx => Side::Context,
            DiffKind::Del => Side::Deleted,
            DiffKind::Add => Side::Added,
        })
        .collect();
    pair(&sides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(spec: &[(DiffKind, &str)]) -> Vec<DiffRow> {
        spec.iter()
            .enumerate()
            .map(|(i, (kind, text))| DiffRow {
                kind: *kind,
                text: text.to_string(),
                lo: (i + 1).to_string(),
                ln: (i + 1).to_string(),
            })
            .collect()
    }

    #[test]
    fn our_kinds_arrive_at_the_fold_as_the_right_sides() {
        // The fold is `tui::diff`, tested there. This is the seam: that a
        // deletion goes left, an addition right, and a header alone.
        let pairs = split_rows(&rows(&[
            (DiffKind::Hdr, "@@ -1 +1 @@"),
            (DiffKind::Del, "-gone"),
            (DiffKind::Add, "+new"),
            (DiffKind::Ctx, " same"),
        ]));
        assert_eq!(pairs.len(), 3, "header, edit, context");
        assert_eq!(pairs[0].header, Some(0));
        assert_eq!((pairs[1].left, pairs[1].right), (Some(1), Some(2)));
        assert_eq!((pairs[2].left, pairs[2].right), (Some(3), Some(3)));
    }

    fn rendered(rows: &[DiffRow], split: bool, w: u16, h: u16) -> Vec<String> {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        let pairs = if split { split_rows(rows) } else { Vec::new() };
        for row in 0..h.min(if split { pairs.len() } else { rows.len() } as u16) {
            if split {
                draw_split_row(&mut buf, area, row, &pairs[row as usize], rows);
            } else {
                draw_unified_row(&mut buf, area, row, &rows[row as usize]);
            }
        }
        (0..h)
            .map(|y| {
                (0..w)
                    .filter_map(|x| buf.cell((x, y)).map(|c| c.symbol().to_string()))
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn each_side_of_a_split_shows_its_own_file_number() {
        // A context line below an insertion is line 5 on the left and 6 on
        // the right. This is what the fold moving to indices had to preserve:
        // it used to be handed two pre-made cells with a number each.
        let mut src = rows(&[(DiffKind::Add, "+added"), (DiffKind::Ctx, " same")]);
        src[1].lo = "5".into();
        src[1].ln = "6".into();

        let out = rendered(&src, true, 60, 2);
        let ctx = out
            .iter()
            .find(|l| l.matches("same").count() == 2)
            .unwrap_or_else(|| panic!("context should be on both sides:\n{out:#?}"));
        // split on the separator rather than on a byte count: `│` is three
        // bytes, and slicing a string in the middle of one is a panic
        let (l, r) = ctx.split_once('│').expect("the halves are divided");
        assert!(l.contains('5'), "left carries the old number: {ctx:?}");
        assert!(r.contains('6'), "right carries the new one: {ctx:?}");
    }

    #[test]
    fn the_blank_half_of_an_uneven_edit_is_painted_rather_than_left_bare() {
        // Nothing was there, and the duller ground is how that is said.
        let src = rows(&[(DiffKind::Add, "+only")]);
        let out = rendered(&src, true, 60, 1);
        assert_eq!(out[0].matches("only").count(), 1, "one side only");
        assert!(out[0].contains('│'), "and the separator still divides them");
    }
}
