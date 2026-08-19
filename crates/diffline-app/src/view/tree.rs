//! The list of files a scope touches.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::parts::{count_style, put_right_parts};
use crate::app::{App, Pane};
use crate::hit::{Region, Target};
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

    scroll_into_view(&mut app.tree_scroll, app.file_idx, rows, app.files.len());
    // Directory separators take rows of their own, so a row is not an index
    // here: the loop below records where each file actually landed.
    let mut placed: Vec<(u16, usize)> = Vec::new();
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

        placed.push((y, i));
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
            crate::model::Status::Added => theme::green(),
            crate::model::Status::Deleted => theme::red(),
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

    // One region per file row, so a click reads through the separators.
    for (y, i) in placed {
        app.hits.push(Region::rows(
            Target::Pane(Pane::Tree),
            Rect {
                x: area.x,
                y,
                width: area.width,
                height: 1,
            },
            1,
            i,
            i + 1,
        ));
    }
}
