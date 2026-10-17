//! The files a scope touches, as a folder tree that folds.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::parts::{count_style, put_right_parts};
use crate::app::{App, Pane};
use crate::hit::{Region, Target};
use crate::shared::tree::Entry;
use crate::tui::theme;
use crate::tui::{Section, fill, put, put_trunc, scroll_into_view};

pub(super) fn tree(buf: &mut Buffer, area: Rect, app: &mut App) {
    let list = Section::new("CHANGES")
        .count(app.files.len())
        .focused(app.pane == Pane::Tree)
        .open(buf, area);
    let rows = list.height as usize;

    if app.files.is_empty() {
        let state = app.files_state.clone();
        let failure = state.error();
        crate::tui::empty(
            buf,
            list,
            &if state.is_loading() {
                crate::tui::Empty::Loading {
                    widths: &[64, 40, 78, 30, 56, 70],
                    phase: app.anim,
                }
            } else if let Some(e) = &failure {
                crate::tui::Empty::Failed(e)
            } else {
                crate::tui::Empty::Nothing("nothing changed")
            },
            theme::panel_alt(),
        );
        return;
    }

    let tree = app.tree_rows();
    let at = app.tree_row(&tree);
    // Snap to the selection only when the selection is what moved. Snapping
    // on every frame meant the wheel could never leave the selection's
    // window — three notches down, and the next draw pulled the list back.
    if at != app.tree_seen {
        scroll_into_view(&mut app.tree_scroll, at, rows, tree.len());
        app.tree_seen = at;
    } else {
        // A free scroll still stops where the snap would: at the last window.
        app.tree_scroll = app.tree_scroll.min(tree.len().saturating_sub(rows.max(1)));
    }
    let focused = app.pane == Pane::Tree;
    let open = app.file_idx;

    for (row, r) in tree.iter().enumerate().skip(app.tree_scroll) {
        let y = list.y + (row - app.tree_scroll) as u16;
        if y >= list.bottom() {
            break;
        }
        let sel = row == at;
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

        // Two columns a level, but never so deep the name has no room.
        let indent = (r.depth as u16 * 2).min(area.width.saturating_sub(14));
        let x = area.x + 2 + indent;
        match r.entry {
            Entry::Dir(_) => {
                // An open folder leaves its counts to the files under it; a
                // folded one stands in for them.
                let right = if r.folded {
                    let files = if r.files == 1 { "file" } else { "files" };
                    put_right_parts(
                        buf,
                        area.right() - 1,
                        y,
                        &[
                            (&format!("{} {files} ", r.files), base.fg(theme::dimmer())),
                            (&format!("+{}", r.add), count_style(base, r.add, true)),
                            (" ", base),
                            (&format!("−{}", r.del), count_style(base, r.del, false)),
                        ],
                    )
                } else {
                    area.right()
                };
                let cx = put(
                    buf,
                    x,
                    y,
                    area.right(),
                    if r.folded { "▸ " } else { "▾ " },
                    base.fg(theme::cyan_soft()),
                );
                put_trunc(
                    buf,
                    cx,
                    y,
                    right.saturating_sub(1),
                    &format!("{}/", r.label),
                    base.fg(theme::dim()),
                );
            }
            Entry::File(i) => {
                let Some(f) = app.files.get(i) else { continue };
                let status_fg = match f.status {
                    crate::model::Status::Added => theme::green(),
                    crate::model::Status::Deleted => theme::red(),
                    _ => theme::cyan(),
                };
                let cx = put(buf, x, y, area.right(), f.status.mark(), base.fg(status_fg));
                // A file with notes on it carries a dot, so the tree says
                // where the work is without opening anything. The dot
                // carries "this one has notes"; the counts carry what changed.
                let noted = app.comments.iter().any(|c| c.path() == f.path);
                let right = put_right_parts(
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
                // the file on show stays lit while the cursor is elsewhere
                let lit = sel || i == open;
                put_trunc(
                    buf,
                    cx + 1,
                    y,
                    right.saturating_sub(1),
                    &r.label,
                    base.fg(if lit { theme::bright() } else { theme::fg() }),
                );
            }
        }
    }

    // One region for the rows: a row is an index into the tree now, folders
    // and all, so a click reads straight through without separators to skip.
    app.hits.push(Region::rows(
        Target::Pane(Pane::Tree),
        list,
        1,
        app.tree_scroll,
        tree.len(),
    ));
}
