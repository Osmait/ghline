//! Diff view: changed files on the left and their contents on the right, in
//! unified or split mode.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::diffnav::find;
use crate::app::files::Entry;
use crate::app::hit::{Region, Target};
use crate::app::prepared::{Prepared, SIGN, code, split_rows};
use crate::app::{App, Pane};
use crate::data::{Change, DiffKind};
use crate::shared::icons::{file as icon_file, folder};
use crate::shared::syntax::Span;
use crate::tui::diff::Pair;
use crate::tui::theme;
use crate::tui::{
    bold, fill, hline, pct, put, put_right, put_trunc, scroll_into_view, skel_bar, vline,
};

const FILES_W: u16 = 38;

/// Where a review comment's bar is drawn. Near the edge rather than under
/// the code: the line numbers mean nothing to a comment, and on a narrow pane
/// the columns they would cost are most of the room its text has.
const NOTE_X: u16 = 2;

/// Background for each line kind, as in the design.
fn row_bg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::diff_add_bg(),
        DiffKind::Del => theme::diff_del_bg(),
        DiffKind::Hdr => theme::tab_active_bg(),
        DiffKind::Ctx => theme::bg(),
        // a comment sits on a panel of its own, apart from the code
        DiffKind::NoteHead | DiffKind::Note => theme::panel(),
    }
}

fn row_fg(kind: DiffKind) -> Color {
    match kind {
        DiffKind::Add => theme::green(),
        DiffKind::Del => theme::red(),
        DiffKind::Hdr => theme::purple(),
        DiffKind::Ctx => theme::step_fg(),
        DiffKind::NoteHead => theme::cyan(),
        DiffKind::Note => theme::body(),
    }
}

pub(crate) fn draw(buf: &mut Buffer, area: Rect, app: &mut App) {
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
    // One commit's files say which commit, in the colour its picker wears,
    // so the tree is never read as the whole pull request when it is not.
    let commit = app
        .diff_commit
        .as_ref()
        .and_then(|sha| app.commits().iter().find(|c| &c.sha == sha));
    let (title, color) = match commit {
        Some(c) => (format!("COMMIT {}", c.short()), theme::yellow()),
        None => ("FILES CHANGED".to_string(), theme::dim()),
    };
    put_trunc(
        buf,
        area.x + 1,
        area.y,
        area.right() - 1,
        &title,
        Style::default().bg(theme::panel()).fg(color),
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
    let stats = if app.diff_commit.is_some() {
        // a commit's totals are its own files', not the pull request's
        let count = |s: &str| s.trim_start_matches(['+', '-']).parse::<u32>().unwrap_or(0);
        let files = app.diff_files();
        let (add, del) = files
            .iter()
            .fold((0, 0), |(a, d), f| (a + count(&f.add), d + count(&f.del)));
        format!("{} in this commit · +{add} -{del}", files_n(files.len()))
    } else {
        match app.current() {
            Some(cur) => cur.as_pr().map_or_else(String::new, |p| {
                format!(
                    "{} changed · {} {}",
                    files_n(p.files as usize),
                    p.add,
                    p.del
                )
            }),
            None => String::new(),
        }
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

    let tree = app.file_rows();
    let len = tree.len();
    let sel = app.file_row(&tree);
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
    let icon_style = crate::shared::config::file_icons();
    let shown = app.file_idx();

    for (row, r) in tree.iter().enumerate().skip(scroll) {
        if row - scroll >= list_h {
            break;
        }
        let y = area.y + 2 + (row - scroll) as u16;
        let selected = row == sel;
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

        // The counts on the right. An open folder leaves them to the files
        // under it, which are right there; a folded one stands in for them.
        let dir = matches!(r.entry, Entry::Dir(_));
        let counts = !dir || r.folded;
        let mut right = area.right() - 1;
        if counts {
            let del_x = put_right(buf, right, y, &format!("-{}", r.del), base.fg(theme::red()));
            right = put_right(
                buf,
                del_x - 1,
                y,
                &format!("+{}", r.add),
                base.fg(theme::green()),
            );
        }
        // open threads on a file: what is still being talked about in it
        if let Entry::File(i) = r.entry
            && let Some(f) = app.diff_files().get(i)
        {
            let open = app
                .threads_on(&f.path)
                .iter()
                .filter(|t| !t.resolved)
                .count();
            if open > 0 {
                right = put_right(
                    buf,
                    right - 1,
                    y,
                    &format!("●{open}"),
                    base.fg(theme::yellow()),
                );
            }
            let drafts = app.drafts_on(&f.path).len();
            if drafts > 0 {
                right = put_right(
                    buf,
                    right - 1,
                    y,
                    &format!("✎{drafts}"),
                    base.fg(theme::orange()),
                );
            }
        }
        if dir && r.folded {
            right = put_right(
                buf,
                right - 1,
                y,
                &format!(
                    "{} {}",
                    r.files,
                    if r.files == 1 { "file" } else { "files" }
                ),
                base.fg(theme::dimmer()),
            );
        }

        // Two columns a level, but never so deep the name has no room: a
        // PR's tree is rarely more than five levels and a pane is 38 wide.
        let indent = (r.depth as u16 * 2).min(area.width.saturating_sub(16));
        let mut x = area.x + 2 + indent;
        let (arrow, icon) = match &r.entry {
            Entry::Dir(_) => (
                if r.folded { "▸" } else { "▾" },
                folder(icon_style, !r.folded),
            ),
            Entry::File(_) => (" ", ""),
        };
        // a file keeps the arrow's column blank, as in the repository's own
        // tree, so it lines up under its folder's name and not its arrow
        x = put(buf, x, y, area.right(), arrow, base.fg(theme::cyan_soft()));
        let icon = match &r.entry {
            Entry::File(i) => app
                .diff_files()
                .get(*i)
                .map_or("", |f| icon_file(icon_style, &f.path)),
            Entry::Dir(_) => icon,
        };
        if !icon.is_empty() {
            let color = match &r.entry {
                Entry::File(i) => app.diff_files().get(*i).map_or(theme::dimmer(), |f| {
                    theme::lang(crate::shared::icons::language(&f.path))
                }),
                Entry::Dir(_) => theme::cyan_soft(),
            };
            x = put(buf, x, y, area.right(), icon, base.fg(color));
            x = put(buf, x, y, area.right(), " ", base);
        }
        let (label, style) = match r.entry {
            Entry::Dir(_) => (format!("{}/", r.label), base.fg(theme::cyan_soft())),
            Entry::File(i) => {
                let lit = selected || i == shown;
                let change = app
                    .diff_files()
                    .get(i)
                    .map_or(Change::Modified, |f| f.change);
                // what happened to it outranks being lit; being lit then shows
                // as weight, so the file on show still stands out
                let style = match change_style(change, base) {
                    Some(s) if lit => s.add_modifier(Modifier::BOLD),
                    Some(s) => s,
                    None if lit => base.fg(theme::bright()),
                    None => base.fg(theme::fg()),
                };
                (r.label.clone(), style)
            }
        };
        put_trunc(buf, x, y, right.saturating_sub(1), &label, style);
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

    // Narrow, the two toggles keep their glyph and key and lose their words:
    // the words are in the help, and the file's name is in no other place.
    let narrow = area.width < 90;
    let split_label = match (app.split, narrow) {
        (true, false) => "▥ split [s]",
        (false, false) => "▤ unified [s]",
        (true, true) => "▥ [s]",
        (false, true) => "▤ [s]",
    };
    let ws_label = match (app.ws, narrow) {
        (true, false) => "● ignore ws [w]",
        (false, false) => "○ ignore ws [w]",
        (true, true) => "● [w]",
        (false, true) => "○ [w]",
    };
    let ws_x = put_right(
        buf,
        area.right() - 1,
        area.y,
        ws_label,
        base.fg(theme::dimmer()),
    );
    let split_x = put_right(buf, ws_x - 2, area.y, split_label, base.fg(theme::purple()));

    let prep = app.prepared();
    let rows = &prep.rows;
    let hunks = rows.iter().filter(|r| r.kind == DiffKind::Hdr).count();
    let onto = app
        .current()
        .map(crate::data::Item::onto)
        .unwrap_or_default();
    let noun = if hunks == 1 { "hunk" } else { "hunks" };
    let (path, tag) = app.diff_file().map_or_else(
        || (String::new(), None),
        |f| (f.path.clone(), change_tag(f)),
    );
    // The file is what this header is for. The branches are the same for
    // every file and already in the detail, so they are the first thing to
    // go when the two do not fit together.
    let room = split_x.saturating_sub(2).saturating_sub(area.x + 1) as usize;
    let tag_w = tag.as_ref().map_or(0, |(t, _)| t.width() + 2);
    let short = format!("{hunks} {noun}");
    let meta =
        if onto.is_empty() || path.width() + tag_w + 2 + short.width() + 3 + onto.width() > room {
            short
        } else {
            format!("{short} · {onto}")
        };
    let meta_x = put_right(
        buf,
        split_x.saturating_sub(2),
        area.y,
        &meta,
        base.fg(theme::dimmer()),
    );
    // Both are cut from the left when they must be cut: a name is at the end,
    // and the name is what says which file this is. The path gives way to the
    // tag only down to its own file name, so both names stay in view: on a
    // rename those two are the whole story.
    let free = meta_x.saturating_sub(1).saturating_sub(area.x + 1) as usize;
    let name_w = path.rsplit('/').next().unwrap_or(&path).width() + 2; // `…/`
    let path_cols = if path.width() + tag_w <= free {
        path.width()
    } else {
        path.width().min(free.saturating_sub(tag_w).max(name_w))
    };
    let px = put_trunc(
        buf,
        area.x + 1,
        area.y,
        meta_x.saturating_sub(1),
        &tail(&path, path_cols),
        base.fg(theme::fg()),
    );
    if let Some((tag, color)) = tag {
        let left = meta_x.saturating_sub(1).saturating_sub(px + 2) as usize;
        put_trunc(
            buf,
            px + 2,
            area.y,
            meta_x.saturating_sub(1),
            &tail_tag(&tag, left),
            base.fg(color),
        );
    }
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
    app.diff_width = view.width as usize;

    if rows.is_empty() {
        empty(buf, view, app);
        return;
    }
    let split = app.split;
    unified_or_split(buf, view, app, &prep, split);
}

/// What the header says beside a file that was not simply modified.
fn change_tag(f: &crate::data::FileChange) -> Option<(String, Color)> {
    match (f.change, &f.from) {
        (Change::Added, _) => Some(("new file".into(), theme::green())),
        (Change::Deleted, _) => Some(("deleted".into(), theme::red())),
        (Change::Renamed, Some(old)) => {
            Some((format!("← {}", moved_from(&f.path, old)), theme::yellow()))
        }
        (Change::Renamed, None) => Some(("renamed".into(), theme::yellow())),
        (Change::Modified, _) => None,
    }
}

/// What to say a renamed file was: its old name alone when it stayed in the
/// same folder — the folder is in the path beside it — and its whole old
/// path when it moved.
fn moved_from<'a>(path: &str, old: &'a str) -> &'a str {
    let dir = |p: &str| p.rfind('/').map_or("", |i| &p[..i]).to_string();
    if dir(path) == dir(old) {
        old.rsplit('/').next().unwrap_or(old)
    } else {
        old
    }
}

/// A tag cut to `cols`, keeping its arrow and the end of what follows it.
fn tail_tag(tag: &str, cols: usize) -> String {
    match tag.strip_prefix("← ") {
        Some(rest) if tag.width() > cols => format!("← {}", tail(rest, cols.saturating_sub(2))),
        _ => tag.to_string(),
    }
}

/// The colour and style a file's name takes in the tree for what happened to
/// it, when that was more than an edit.
fn change_style(change: Change, base: Style) -> Option<Style> {
    match change {
        Change::Added => Some(base.fg(theme::green())),
        // struck through as well as red: a deleted file is read for what it
        // took away, and red alone is also what a big deletion count is
        Change::Deleted => Some(base.fg(theme::red()).add_modifier(Modifier::CROSSED_OUT)),
        Change::Renamed => Some(base.fg(theme::yellow())),
        Change::Modified => None,
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
        // The mode this used to print was made up: `gh` does not send it.
        let why = match (f.change, &f.from) {
            (Change::Renamed, Some(old)) => format!("renamed from {old}, contents unchanged"),
            (Change::Renamed, None) => "renamed, contents unchanged".to_string(),
            (Change::Added | Change::Deleted, _) => "an empty file".to_string(),
            (Change::Modified, _) => "binary, or only its mode changed".to_string(),
        };
        (
            "no textual changes".to_string(),
            format!("{} · {why}", f.path),
        )
    } else {
        (
            "diff not fetched yet".to_string(),
            format!(
                "{} · {} {} · press r to fetch it again",
                f.path, f.add, f.del
            ),
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
fn unified_or_split(buf: &mut Buffer, area: Rect, app: &mut App, prep: &Prepared, split: bool) {
    let rows = &prep.rows;
    let pairs = if split { split_rows(rows) } else { Vec::new() };
    let len = if split { pairs.len() } else { rows.len() };
    let h = area.height as usize;
    // the cursor is kept on a line that exists, and the view on the cursor
    app.diff_cursor = app.diff_cursor.min(len.saturating_sub(1));
    scroll_into_view(&mut app.diff_scroll, app.diff_cursor, h, len);
    app.diff_scroll = app.diff_scroll.min(len.saturating_sub(h));
    let scroll = app.diff_scroll;
    let cursor = app.diff_cursor;
    let focused = app.pane == Pane::DiffBody;
    let query = app.diff_query.clone();
    let pan = app.diff_pan;

    for row in 0..h.min(len.saturating_sub(scroll)) {
        let y = area.y + row as u16;
        let i = scroll + row;
        if split {
            draw_split_row(buf, area, y, &pairs[i], prep, (&query, pan));
        } else {
            draw_unified_row(buf, area, y, prep, i, (&query, pan));
        }
        // The cursor's line: its ground lifted to the selection's, its colours
        // kept, and a bar at the edge — lit while the pane has the focus, as
        // every other pane's cursor is.
        if i == cursor {
            for x in area.x..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_bg(theme::sel());
                }
            }
            let mark = if focused {
                theme::cyan()
            } else {
                theme::sel_mark_idle()
            };
            put(
                buf,
                area.x,
                y,
                area.x + 1,
                "▌",
                Style::default().bg(theme::sel()).fg(mark),
            );
        }
    }
}

fn draw_unified_row(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    prep: &Prepared,
    i: usize,
    (query, pan): (&str, usize),
) {
    let r = &prep.rows[i];
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
    if r.kind.is_note() {
        draw_note(buf, area.x + NOTE_X, y, area.right(), r, base);
        return;
    }
    // two numbering columns: original file and new file
    put_right(buf, area.x + 6, y, &r.lo, base.fg(theme::gutter()));
    put_right(buf, area.x + 12, y, &r.ln, base.fg(theme::gutter()));
    draw_line(
        buf,
        (area.x + 14, y),
        area.right(),
        Line {
            prep,
            i,
            query,
            pan,
        },
        base,
    );
}

/// One row's text: its sign, then its code in syntax colour with the words
/// that changed reversed, cut with a `…` when it does not fit.
///
/// Syntax colour goes on context lines always, and on added and deleted ones
/// only when the theme tints their ground. The terminal theme cannot, and
/// there the green and red of the text are all that tell an addition from a
/// deletion — colouring its keywords purple would take that away.
///
/// The changed words are reversed rather than given a ground of their own,
/// which is what git's `diff-highlight` does: it reads in every theme, the
/// terminal one included, without a role for "a stronger green".
///
/// `tinted` is whether the theme gives added and deleted lines a ground of
/// their own, passed in rather than read here so a test can draw both ways.
fn draw_line(buf: &mut Buffer, at: (u16, u16), max: u16, line: Line<'_>, base: Style) {
    draw_line_tinted(buf, at, max, line, base, tints());
}

/// Which row of which file to draw, and what the search is looking for.
#[derive(Clone, Copy)]
struct Line<'a> {
    prep: &'a Prepared,
    i: usize,
    query: &'a str,
    /// Columns of code scrolled off the left.
    pan: usize,
}

/// Whether the theme in force tints added and deleted lines.
fn tints() -> bool {
    theme::diff_add_bg() != theme::bg()
}

fn draw_line_tinted(
    buf: &mut Buffer,
    (x, y): (u16, u16),
    max: u16,
    Line {
        prep,
        i,
        query,
        pan,
    }: Line<'_>,
    base: Style,
    tinted: bool,
) {
    let r = &prep.rows[i];
    let ink = base.fg(row_fg(r.kind));
    if r.kind == DiffKind::Hdr {
        put_trunc(buf, x, y, max, &r.text, ink);
        return;
    }
    let sign = r.text.get(..SIGN).unwrap_or("");
    let x = put(buf, x, y, max, sign, ink);
    let text = code(r);

    let coloured = r.kind == DiffKind::Ctx || tinted;
    let spans: &[Span] = if coloured {
        prep.spans.get(i).map_or(&[], Vec::as_slice)
    } else {
        &[]
    };
    let word = prep.words.get(i).cloned().flatten();

    // Panned: the code starts `pan` columns in, and a `…` where it was cut
    // says there is more to the left. Everything below is still counted in
    // bytes from the start of the whole line, which is what the spans use.
    let start = fit(text, pan);
    let x = if start > 0 {
        put(buf, x, y, max, "…", base.fg(theme::dimmer()))
    } else {
        x
    };
    // where the text has to stop to leave a column for the `…`
    let room = max.saturating_sub(x) as usize;
    let cut = if text[start..].width() > room {
        Some(start + fit(&text[start..], room.saturating_sub(1)))
    } else {
        None
    };
    let end = cut.unwrap_or(text.len());

    // Every place the style can change, in order; between two of them it
    // cannot, so each stretch is one `put`.
    let mut marks = vec![start, end];
    for s in spans {
        marks.extend([s.from, s.to]);
    }
    if let Some(w) = &word {
        marks.extend([w.start, w.end]);
    }
    // every place the search matches, so each one is lit and not just the
    // first — the eye goes down a column of them
    let mut hits = Vec::new();
    let mut from = 0;
    while let Some(m) = text.get(from..).and_then(|t| find(t, query)) {
        if m.is_empty() {
            break;
        }
        let m = from + m.start..from + m.end;
        from = m.end;
        marks.extend([m.start, m.end]);
        hits.push(m);
    }
    marks.retain(|m| (start..=end).contains(m) && text.is_char_boundary(*m));
    marks.sort_unstable();
    marks.dedup();

    let mut cx = x;
    for pair in marks.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let mut style = spans
            .iter()
            .find(|s| s.from <= from && to <= s.to)
            .map_or(ink, |s| base.fg(syntax_color(s.kind)));
        if word
            .as_ref()
            .is_some_and(|w| w.start <= from && to <= w.end)
        {
            style = style.add_modifier(Modifier::REVERSED);
        }
        // a search hit wins over both: it is what you are looking for now
        if hits.iter().any(|h| h.start <= from && to <= h.end) {
            style = base.fg(theme::yellow()).add_modifier(Modifier::REVERSED);
        }
        cx = put(buf, cx, y, max, &text[from..to], style);
    }
    if cut.is_some() {
        put(buf, cx, y, max, "…", base.fg(theme::dimmer()));
    }
}

/// The end of `path` that fits in `cols` columns, behind a `…` when some of
/// it had to go.
fn tail(path: &str, cols: usize) -> String {
    if path.width() <= cols || cols == 0 {
        return path.to_string();
    }
    let mut used = 1; // the `…`
    let mut start = path.len();
    for (i, c) in path.char_indices().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > cols {
            break;
        }
        used += w;
        start = i;
    }
    format!("…{}", &path[start..])
}

/// A review comment's row: a bar down the left in the accent, then who said
/// it and when, or a line of what they said.
///
/// The head is styled by how it starts — a settled thread with its tick, an
/// outdated one with its warning — so the rows need no kind beyond the two.
fn draw_note(buf: &mut Buffer, x: u16, y: u16, max: u16, r: &crate::data::DiffRow, base: Style) {
    use crate::app::notes::{OUTDATED, RESOLVED};
    let x = put(buf, x, y, max, "┃ ", base.fg(theme::yellow()));
    if r.kind == DiffKind::Note {
        put_trunc(buf, x, y, max, &r.text, base.fg(theme::body()));
        return;
    }
    if r.text == crate::app::review::DRAFT {
        // not yet said: the colour a warning wears, since it is one — it is
        // lost if it is never sent
        put_trunc(buf, x, y, max, &r.text, bold(base.fg(theme::orange())));
        return;
    }
    if let Some(rest) = r.text.strip_prefix(RESOLVED) {
        let x = put(buf, x, y, max, RESOLVED, base.fg(theme::green()));
        put_trunc(buf, x, y, max, rest, base.fg(theme::dim()));
        return;
    }
    let (x, rest) = match r.text.strip_prefix(OUTDATED) {
        Some(rest) => (
            put(buf, x, y, max, OUTDATED, base.fg(theme::orange())),
            rest,
        ),
        None => (x, r.text.as_str()),
    };
    // `who · when`: the name lit, the time quiet
    let (who, when) = rest.split_once(" · ").unwrap_or((rest, ""));
    let x = put(buf, x, y, max, who, bold(base.fg(theme::cyan())));
    if !when.is_empty() {
        put_trunc(
            buf,
            x,
            y,
            max,
            &format!(" · {when}"),
            base.fg(theme::dimmer()),
        );
    }
}

/// `1 file`, `2 files`.
fn files_n(n: usize) -> String {
    format!("{n} {}", if n == 1 { "file" } else { "files" })
}

/// The byte where `text` stops fitting in `cols` columns.
fn fit(text: &str, cols: usize) -> usize {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        let w = c.width().unwrap_or(0);
        if used + w > cols {
            return i;
        }
        used += w;
    }
    text.len()
}

/// The explorer's colours, so a file reads the same in both places.
fn syntax_color(kind: crate::shared::syntax::Kind) -> Color {
    use crate::shared::syntax::Kind;
    match kind {
        Kind::Comment => theme::dimmer(),
        Kind::Str => theme::green(),
        Kind::Number => theme::orange(),
        Kind::Keyword => theme::purple(),
        Kind::Type => theme::cyan_soft(),
    }
}

/// One side-by-side line.
///
/// Reads its cells out of `rows` by index rather than being handed copies of
/// them: the fold works in indices now, and cloning two strings per row per
/// frame to hand them here was paying for the convenience twice.
fn draw_split_row(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    p: &Pair,
    prep: &Prepared,
    (query, pan): (&str, usize),
) {
    let rows = &prep.rows;
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

    if let Some(r) = p
        .header
        .and_then(|i| rows.get(i))
        .filter(|r| r.kind.is_note())
    {
        let full = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        fill(buf, full, row_bg(r.kind));
        draw_note(
            buf,
            area.x + NOTE_X,
            y,
            area.right(),
            r,
            Style::default().bg(row_bg(r.kind)),
        );
        return;
    }
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
        let Some((i, r)) = at.and_then(|i| rows.get(i).map(|r| (i, r))) else {
            // the gap of an unbalanced pair gets a duller grey
            fill(buf, rect, theme::diff_void_bg());
            return;
        };
        let bg = row_bg(r.kind);
        fill(buf, rect, bg);
        let base = Style::default().bg(bg);
        let num = if old { &r.lo } else { &r.ln };
        put_right(buf, rect.x + 5, y, num, base.fg(theme::gutter()));
        draw_line(
            buf,
            (rect.x + 6, y),
            rect.right(),
            Line {
                prep,
                i,
                query,
                pan,
            },
            base,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DiffRow;

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
        let prep = crate::app::prepared::prepare("", rows.to_vec());
        let pairs = if split { split_rows(rows) } else { Vec::new() };
        for row in 0..h.min(if split { pairs.len() } else { rows.len() } as u16) {
            if split {
                draw_split_row(&mut buf, area, row, &pairs[row as usize], &prep, ("", 0));
            } else {
                draw_unified_row(&mut buf, area, row, &prep, row as usize, ("", 0));
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

    /// The style of each column of row 0, as `(symbol, fg, reversed)`.
    fn cells(prep: &Prepared, tinted: bool) -> Vec<(String, Option<Color>, bool)> {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        let line = Line {
            prep,
            i: 1,
            query: "",
            pan: 0,
        };
        draw_line_tinted(&mut buf, (0, 0), 40, line, Style::default(), tinted);
        (0..40)
            .filter_map(|x| buf.cell((x, 0)))
            .map(|c| {
                let s = c.style();
                (
                    c.symbol().to_string(),
                    s.fg,
                    s.add_modifier.contains(Modifier::REVERSED),
                )
            })
            .collect()
    }

    fn edit() -> Prepared {
        crate::app::prepared::prepare(
            "x.rs",
            rows(&[
                (DiffKind::Del, "- let n = total;"),
                (DiffKind::Add, "+ let n = width;"),
            ]),
        )
    }

    #[test]
    fn the_word_that_changed_is_reversed_and_the_rest_is_not() {
        let c = cells(&edit(), true);
        let reversed: String = c.iter().filter(|c| c.2).map(|c| c.0.as_str()).collect();
        assert_eq!(reversed, "width");
    }

    #[test]
    fn an_untinted_theme_keeps_an_added_line_all_green() {
        // The terminal theme has no ground for an addition, so its green ink
        // is what says "added" — syntax colour would take it away.
        //
        // Measured against the sign's own ink rather than `theme::green()`:
        // the theme is process-wide, other tests switch it, and a colour read
        // a moment later can be another theme's green.
        let c = cells(&edit(), false);
        let sign = c.iter().find(|c| c.0 == "+").map(|c| c.1);
        let text: Vec<_> = c.iter().filter(|c| c.0.trim() != "").collect();
        assert!(text.iter().all(|c| Some(c.1) == sign), "{c:?}");
        // a tinted one colours the keyword apart from the sign
        let c = cells(&edit(), true);
        let sign = c.iter().find(|c| c.0 == "+").map(|c| c.1);
        assert!(c.iter().any(|c| c.0 == "l" && Some(c.1) != sign));
    }

    #[test]
    fn a_line_too_long_for_its_room_ends_in_an_ellipsis() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let prep = crate::app::prepared::prepare(
            "",
            rows(&[(DiffKind::Ctx, "  abcdefghijklmnopqrstuvwxyz")]),
        );
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let line = Line {
            prep: &prep,
            i: 0,
            query: "",
            pan: 0,
        };
        draw_line_tinted(&mut buf, (0, 0), 10, line, Style::default(), true);
        let line: String = (0..10)
            .filter_map(|x| buf.cell((x, 0)).map(|c| c.symbol().to_string()))
            .collect();
        assert_eq!(line, "  abcdefg…");
    }

    #[test]
    fn a_path_too_long_for_the_header_keeps_its_name() {
        assert_eq!(
            tail(".github/workflows/backend.yaml", 16),
            "…ws/backend.yaml"
        );
        assert_eq!(tail("short.rs", 16), "short.rs");
    }

    #[test]
    fn a_panned_line_says_there_is_more_to_its_left() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let prep =
            crate::app::prepared::prepare("", rows(&[(DiffKind::Ctx, "  0123456789abcdef")]));
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        let line = Line {
            prep: &prep,
            i: 0,
            query: "",
            pan: 4,
        };
        draw_line_tinted(&mut buf, (0, 0), 12, line, Style::default(), true);
        let shown: String = (0..12)
            .filter_map(|x| buf.cell((x, 0)).map(|c| c.symbol().to_string()))
            .collect();
        assert_eq!(shown, "  …456789ab…");
    }

    #[test]
    fn a_rename_within_its_folder_names_only_the_old_file() {
        assert_eq!(
            moved_from("web/proxy.ts", "web/middleware.ts"),
            "middleware.ts"
        );
        assert_eq!(moved_from("web/proxy.ts", "old/proxy.ts"), "old/proxy.ts");
        assert_eq!(moved_from("proxy.ts", "middleware.ts"), "middleware.ts");
    }

    #[test]
    fn a_rename_tag_too_long_keeps_its_arrow_and_its_end() {
        assert_eq!(tail_tag("← some/deep/old/name.rs", 12), "← …d/name.rs");
        assert_eq!(
            tail_tag("deleted", 3),
            "deleted",
            "a word tag is not cut here"
        );
    }
}
