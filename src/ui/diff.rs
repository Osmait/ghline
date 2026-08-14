//! Diff view: changed files on the left and their contents on the right, in
//! unified or split mode.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{fill, hline, put, put_right, put_trunc, scroll_into_view, vline};
use crate::app::{App, Pane};
use crate::data::{DiffKind, DiffRow};
use crate::theme;

const FILES_W: u16 = 38;

/// Background for each line kind, as in the design.
fn row_bg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::DIFF_ADD_BG,
        DiffKind::Del => theme::DIFF_DEL_BG,
        DiffKind::Hdr => theme::TAB_ACTIVE_BG,
        DiffKind::Ctx => theme::BG,
    }
}

fn row_fg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::GREEN,
        DiffKind::Del => theme::RED,
        DiffKind::Hdr => theme::PURPLE,
        DiffKind::Ctx => theme::STEP_FG,
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
    vline(buf, area.x + files_w, area.y, area.height, theme::BORDER);
    draw_body(buf, body, app);
}

fn draw_files(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::PANEL_ALT);

    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::PANEL);
    put(
        buf,
        area.x + 1,
        area.y,
        area.right(),
        "FILES CHANGED",
        Style::default().bg(theme::PANEL).fg(theme::DIM),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::BORDER_SOFT);

    // footer with the PR total
    let foot_y = area.bottom() - 1;
    hline(buf, area.x, foot_y - 1, area.width, theme::BORDER_SOFT);
    fill(
        buf,
        Rect {
            x: area.x,
            y: foot_y,
            width: area.width,
            height: 1,
        },
        theme::PANEL,
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
        Style::default().bg(theme::PANEL).fg(theme::DIMMER),
    );

    // rows
    let sel = app.file_idx();
    let len = app.diff_files().len();
    let list_h = foot_y.saturating_sub(area.y + 3) as usize;
    scroll_into_view(&mut app.repo_scroll, sel, list_h, len);
    let scroll = app.repo_scroll;
    let focused = app.pane == Pane::Files;

    for (row, i) in (scroll..len).enumerate() {
        if row >= list_h {
            break;
        }
        let y = area.y + 2 + row as u16;
        let f = &app.diff_files()[i];
        let selected = i == sel;
        let bg = if selected {
            theme::SEL
        } else {
            theme::PANEL_ALT
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
                theme::CYAN
            } else {
                theme::SEL_MARK_IDLE
            };
            put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        }

        // counts on the right; the name on the left, with its directory in
        // grey so the file itself stands out
        let del_x = put_right(buf, area.right() - 1, y, &f.del, base.fg(theme::RED));
        let add_x = put_right(buf, del_x - 1, y, &f.add, base.fg(theme::GREEN));

        let (dir, name) = match f.path.rfind('/') {
            Some(i) => f.path.split_at(i + 1),
            None => ("", f.path.as_str()),
        };
        let fg = if selected { theme::BRIGHT } else { theme::FG };
        let max = add_x.saturating_sub(1);
        // when it does not all fit, the directory is cut before the name
        let name_w = name.chars().count() as u16;
        let dir_max = max.saturating_sub(name_w).max(area.x + 2);
        let cx = put_trunc(buf, area.x + 2, y, dir_max, dir, base.fg(theme::DIMMER));
        put_trunc(buf, cx, y, max, name, base.fg(fg));
    }
}

fn draw_body(buf: &mut Buffer, area: Rect, app: &mut App) {
    // ---- header
    let head = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    fill(buf, head, theme::PANEL);
    let base = Style::default().bg(theme::PANEL);

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
        base.fg(theme::DIMMER),
    );
    let split_x = put_right(buf, ws_x - 2, area.y, split_label, base.fg(theme::PURPLE));

    let rows = app.diff_rows();
    let hunks = rows.iter().filter(|r| r.kind == DiffKind::Hdr).count();
    let branch = app
        .current()
        .map(super::super::data::Item::branch)
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
        base.fg(theme::DIMMER),
    );
    let path = app.diff_file().map(|f| f.path.clone()).unwrap_or_default();
    put_trunc(
        buf,
        area.x + 1,
        area.y,
        meta_x.saturating_sub(1),
        &path,
        base.fg(theme::FG),
    );
    hline(buf, area.x, area.y + 1, area.width, theme::BORDER_SOFT);

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
    let (title, sub) = if st.is_loading() {
        (
            "loading diff…".to_string(),
            format!("{} · {} {}", f.path, f.add, f.del),
        )
    } else if let Some(e) = st.error() {
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
    let base = Style::default().bg(theme::BG);
    center(&format!("— {title} —"), y, base.fg(theme::DIM), buf);
    center(&sub, y + 1, base.fg(theme::DIMMER), buf);
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
            draw_split_row(buf, area, y, &pairs[i]);
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
                Style::default().bg(theme::BG).fg(theme::CYAN),
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
    put_right(buf, area.x + 6, y, &r.lo, base.fg(theme::GUTTER));
    put_right(buf, area.x + 12, y, &r.ln, base.fg(theme::GUTTER));
    put_trunc(
        buf,
        area.x + 14,
        y,
        area.right(),
        &r.text,
        base.fg(row_fg(r.kind)),
    );
}

fn draw_split_row(buf: &mut Buffer, area: Rect, y: u16, p: &SplitPair) {
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

    if let Some(hdr) = &p.hdr {
        fill(buf, left, theme::TAB_ACTIVE_BG);
        fill(buf, right, theme::TAB_ACTIVE_BG);
        let s = Style::default().bg(theme::TAB_ACTIVE_BG).fg(theme::PURPLE);
        put_trunc(buf, area.x + 2, y, area.right(), hdr, s);
        return;
    }

    let side = |rect: Rect, cell: &Option<Cell>, buf: &mut Buffer| {
        let (bg, fg, num, text) = match cell {
            Some(c) => (
                row_bg(c.kind),
                row_fg(c.kind),
                c.num.clone(),
                c.text.clone(),
            ),
            // the gap of an unbalanced pair gets a duller grey
            None => (
                theme::DIFF_VOID_BG,
                theme::DIMMER,
                String::new(),
                String::new(),
            ),
        };
        fill(buf, rect, bg);
        let base = Style::default().bg(bg);
        put_right(buf, rect.x + 5, y, &num, base.fg(theme::GUTTER));
        put_trunc(buf, rect.x + 6, y, rect.right(), &text, base.fg(fg));
    };

    side(left, &p.left, buf);
    side(right, &p.right, buf);
    // separator between the two halves
    put(
        buf,
        right.x,
        y,
        right.x + 1,
        "│",
        Style::default()
            .bg(row_bg(p.right.as_ref().map_or(DiffKind::Ctx, |c| c.kind)))
            .fg(theme::BORDER_SOFT),
    );
}

struct Cell {
    kind: DiffKind,
    num: String,
    text: String,
}

struct SplitPair {
    hdr: Option<String>,
    left: Option<Cell>,
    right: Option<Cell>,
}

/// Pairs deletions with additions for split mode, exactly like the design's
/// `splitRows()`.
fn split_rows(rows: &[DiffRow]) -> Vec<SplitPair> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        let r = &rows[i];
        match r.kind {
            DiffKind::Hdr => {
                out.push(SplitPair {
                    hdr: Some(r.text.clone()),
                    left: None,
                    right: None,
                });
                i += 1;
            }
            DiffKind::Ctx => {
                out.push(SplitPair {
                    hdr: None,
                    left: Some(Cell {
                        kind: r.kind,
                        num: r.lo.clone(),
                        text: r.text.clone(),
                    }),
                    right: Some(Cell {
                        kind: r.kind,
                        num: r.ln.clone(),
                        text: r.text.clone(),
                    }),
                });
                i += 1;
            }
            _ => {
                let start = i;
                while i < rows.len() && rows[i].kind == DiffKind::Del {
                    i += 1;
                }
                let dels = &rows[start..i];
                let astart = i;
                while i < rows.len() && rows[i].kind == DiffKind::Add {
                    i += 1;
                }
                let adds = &rows[astart..i];
                for k in 0..dels.len().max(adds.len()) {
                    out.push(SplitPair {
                        hdr: None,
                        left: dels.get(k).map(|d| Cell {
                            kind: d.kind,
                            num: d.lo.clone(),
                            text: d.text.clone(),
                        }),
                        right: adds.get(k).map(|a| Cell {
                            kind: a.kind,
                            num: a.ln.clone(),
                            text: a.text.clone(),
                        }),
                    });
                }
            }
        }
    }
    out
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
    fn context_shows_on_both_sides() {
        let pairs = split_rows(&rows(&[(DiffKind::Ctx, "same")]));
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].left.is_some() && pairs[0].right.is_some());
    }

    #[test]
    fn a_header_spans_the_whole_row() {
        let pairs = split_rows(&rows(&[(DiffKind::Hdr, "@@ -1 +1 @@")]));
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].hdr.is_some());
        assert!(pairs[0].left.is_none() && pairs[0].right.is_none());
    }

    #[test]
    fn equal_runs_pair_up_line_by_line() {
        let pairs = split_rows(&rows(&[
            (DiffKind::Del, "-a"),
            (DiffKind::Del, "-b"),
            (DiffKind::Add, "+a"),
            (DiffKind::Add, "+b"),
        ]));
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().all(|p| p.left.is_some() && p.right.is_some()));
    }

    #[test]
    fn a_longer_side_leaves_gaps_on_the_other() {
        // three additions against one deletion
        let pairs = split_rows(&rows(&[
            (DiffKind::Del, "-a"),
            (DiffKind::Add, "+a"),
            (DiffKind::Add, "+b"),
            (DiffKind::Add, "+c"),
        ]));
        assert_eq!(pairs.len(), 3);
        assert!(pairs[0].left.is_some());
        assert!(pairs[1].left.is_none(), "the left side runs out");
        assert!(pairs[2].left.is_none());
        assert!(pairs.iter().all(|p| p.right.is_some()));
    }

    #[test]
    fn additions_with_no_deletions_are_all_on_the_right() {
        let pairs = split_rows(&rows(&[(DiffKind::Add, "+new")]));
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].left.is_none());
        assert!(pairs[0].right.is_some());
    }

    #[test]
    fn deletions_with_no_additions_are_all_on_the_left() {
        let pairs = split_rows(&rows(&[(DiffKind::Del, "-gone")]));
        assert_eq!(pairs.len(), 1);
        assert!(pairs[0].left.is_some());
        assert!(pairs[0].right.is_none());
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(split_rows(&[]).is_empty());
    }

    #[test]
    fn every_change_survives_the_pairing() {
        // whatever the shape, no line may be dropped
        let src = rows(&[
            (DiffKind::Hdr, "@@"),
            (DiffKind::Ctx, " a"),
            (DiffKind::Del, "-b"),
            (DiffKind::Add, "+c"),
            (DiffKind::Add, "+d"),
            (DiffKind::Ctx, " e"),
        ]);
        let pairs = split_rows(&src);
        let dels = pairs
            .iter()
            .filter(|p| p.left.as_ref().is_some_and(|c| c.kind == DiffKind::Del))
            .count();
        let adds = pairs
            .iter()
            .filter(|p| p.right.as_ref().is_some_and(|c| c.kind == DiffKind::Add))
            .count();
        assert_eq!(dels, 1);
        assert_eq!(adds, 2);
    }
}
