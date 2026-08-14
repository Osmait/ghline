//! Render. Each module reproduces one region of the design.

mod agents;
mod confirm;
mod detail;
mod diff;
mod dispatch;
mod explorer;
mod finder;
mod header;
mod list;
mod logs;
mod markdown;
pub mod overlay;
mod sidebar;
mod status;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, View};
use crate::theme;

// ---------------------------------------------------------------- primitivas

pub fn fill(buf: &mut Buffer, area: Rect, bg: ratatui::style::Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(bg);
            }
        }
    }
}

/// Like `fill`, but it also wipes the text: whatever is drawn on top hides
/// what was underneath. This is what the modals need.
pub fn clear(buf: &mut Buffer, area: Rect, bg: ratatui::style::Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(" ");
                cell.set_style(Style::default().bg(bg).fg(theme::fg()));
            }
        }
    }
}

/// Writes `text` at (x, y), clipping at `max_x` (exclusive). Returns the end x.
pub fn put(buf: &mut Buffer, x: u16, y: u16, max_x: u16, text: &str, style: Style) -> u16 {
    let mut cx = x;
    if y >= buf.area.bottom() {
        return cx;
    }
    for g in unicode_segmentation(text) {
        let w = g.width() as u16;
        if w == 0 {
            continue;
        }
        if cx + w > max_x {
            break;
        }
        if let Some(cell) = buf.cell_mut((cx, y)) {
            cell.set_symbol(g);
            cell.set_style(style);
        }
        // clear the cell a wide grapheme covers
        if w == 2
            && let Some(cell) = buf.cell_mut((cx + 1, y))
        {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
        cx += w;
    }
    cx
}

/// Like `put`, but appends `…` when the text does not fit.
pub fn put_trunc(buf: &mut Buffer, x: u16, y: u16, max_x: u16, text: &str, style: Style) -> u16 {
    let avail = max_x.saturating_sub(x);
    if text.width() as u16 <= avail {
        return put(buf, x, y, max_x, text, style);
    }
    if avail == 0 {
        return x;
    }
    let cut = truncate_to(text, avail.saturating_sub(1) as usize);
    let end = put(buf, x, y, max_x, &cut, style);
    put(buf, end, y, max_x, "…", style)
}

pub fn put_right(buf: &mut Buffer, right_x: u16, y: u16, text: &str, style: Style) -> u16 {
    let w = text.width() as u16;
    let x = right_x.saturating_sub(w);
    put(buf, x, y, right_x, text, style);
    x
}

pub fn hline(buf: &mut Buffer, x: u16, y: u16, w: u16, color: ratatui::style::Color) {
    let s = "─".repeat(w as usize);
    put(
        buf,
        x,
        y,
        x + w,
        &s,
        Style::default().fg(color).bg(theme::bg()),
    );
}

pub fn vline(buf: &mut Buffer, x: u16, y: u16, h: u16, color: ratatui::style::Color) {
    for yy in y..y + h {
        put(
            buf,
            x,
            yy,
            x + 1,
            "│",
            Style::default().fg(color).bg(theme::bg()),
        );
    }
}

fn unicode_segmentation(s: &str) -> Vec<&str> {
    // No extra dependency: one `char` is grapheme enough for this glyph set.
    let mut out = Vec::new();
    let mut idx = 0;
    for c in s.chars() {
        let len = c.len_utf8();
        out.push(&s[idx..idx + len]);
        idx += len;
    }
    out
}

/// Cuts to `w` columns and pads with spaces to fill them all.
pub fn truncate_pad(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    let mut out = if s.width() > w {
        let mut t = truncate_to(s, w.saturating_sub(1));
        t.push('…');
        t
    } else {
        s.to_string()
    };
    while out.width() < w {
        out.push(' ');
    }
    out
}

fn truncate_to(s: &str, w: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.to_string().width();
        if used + cw > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    out
}

/// Lays text with hard breaks out into lines of `width` columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let indent: String = para.chars().take_while(|c| *c == ' ').collect();
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                format!("{indent}{word}")
            } else {
                format!("{line} {word}")
            };
            if candidate.width() > width && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line = format!("{indent}{word}");
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// A run of text with its style; a line is several of them in a row.
pub type Seg = (String, Style);

/// One placeholder block of a skeleton.
///
/// The design asks for this shape itself: its `sc-for` elements carry a
/// `hint-placeholder-count`, so a pane that is still loading is meant to show
/// the outline of what is coming rather than a word.
///
/// `row` and `phase` put a highlight band travelling down the rows, which is
/// what separates "on its way" from "stuck".
pub fn skel_bar(buf: &mut Buffer, x: u16, y: u16, w: u16, row: usize, phase: u64) {
    if w == 0 {
        return;
    }
    const CYCLE: u64 = 16;
    let band = (phase % CYCLE) as i64;
    let color = match (row as i64 - band).abs() {
        0 => theme::sel_mark_idle(),
        1 => theme::sel(),
        _ => theme::panel(),
    };
    let block = "█".repeat(w as usize);
    put(
        buf,
        x,
        y,
        x + w,
        &block,
        Style::default().bg(theme::bg()).fg(color),
    );
}

/// A percentage of the available width, so a skeleton keeps its proportions at
/// any pane size.
pub fn pct(avail: u16, p: u16) -> u16 {
    (u32::from(avail) * u32::from(p) / 100) as u16
}

/// Keeps `sel` visible inside a window of `height` rows.
pub fn scroll_into_view(offset: &mut usize, sel: usize, height: usize, len: usize) {
    if height == 0 {
        return;
    }
    if sel < *offset {
        *offset = sel;
    } else if sel >= *offset + height {
        *offset = sel + 1 - height;
    }
    let max = len.saturating_sub(height);
    *offset = (*offset).min(max);
}

pub fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}

// -------------------------------------------------------------------- layout

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    let buf = f.buffer_mut();
    fill(buf, area, theme::bg());

    // The mouse aims at what is on screen, so the regions are rebuilt from
    // scratch each frame: a pane not drawn this time is not there to click.
    // They are pushed in drawing order, which is what puts a modal in front.
    app.hits.clear();

    if area.height < 8 || area.width < 40 {
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

    // footer rows: border + status (+ border + command line)
    let footer_h: u16 = if app.cmd.is_some() { 4 } else { 2 };

    let header = Rect {
        x: 0,
        y: 0,
        width: area.width,
        height: 1,
    };
    let body = Rect {
        x: 0,
        y: 2,
        width: area.width,
        height: area.height - 2 - footer_h,
    };
    let footer = Rect {
        x: 0,
        y: area.height - footer_h,
        width: area.width,
        height: footer_h,
    };

    header::draw(buf, header, app);
    hline(buf, 0, 1, area.width, theme::border());

    let sidebar_w: u16 = 34;
    // logs and diff take the full width, as in the design; below 90 columns
    // there is not enough room for it whatever the reader asked for
    app.sidebar_shown =
        app.sidebar && !matches!(app.view, View::Logs | View::Diff) && area.width >= 90;
    if !app.sidebar_shown {
        draw_content(buf, body, app);
    } else {
        let side = Rect {
            x: 0,
            y: body.y,
            width: sidebar_w,
            height: body.height,
        };
        sidebar::draw(buf, side, app);
        vline(buf, sidebar_w, body.y, body.height, theme::border());
        let content = Rect {
            x: sidebar_w + 1,
            y: body.y,
            width: area.width - sidebar_w - 1,
            height: body.height,
        };
        draw_content(buf, content, app);
    }

    status::draw(buf, footer, app);

    if app.accounts_open {
        overlay::accounts(buf, area, app);
    }
    if app.finder_open {
        finder::draw(buf, area, app);
    }
    if app.dispatch_open {
        dispatch::draw(buf, area, app);
    }
    if app.themes_open {
        overlay::themes(buf, area, app);
    }
    if app.help_open {
        overlay::help(buf, area);
    }
    if let Some(prompt) = app.prompt.clone() {
        confirm::draw(buf, area, app, &prompt);
    }
}

fn draw_content(buf: &mut Buffer, area: Rect, app: &mut App) {
    if app.view == View::Logs {
        logs::draw(buf, area, app);
        return;
    }
    if app.view == View::Diff {
        diff::draw(buf, area, app);
        return;
    }

    // tab bar + its bottom border
    let tabs = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    list::tabs(buf, tabs, app);

    let inner = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(2),
    };

    match app.view {
        View::List if app.tab == crate::data::AGENTS_TAB => agents::draw(buf, inner, app),
        View::List if app.tab == crate::data::FILES_TAB => explorer::draw(buf, inner, app),
        View::List => list::draw(buf, inner, app),
        View::Detail => detail::draw(buf, inner, app),
        View::Diff | View::Logs => {}
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

    // --- text wrapping ---

    #[test]
    fn wrap_respects_hard_newlines() {
        let out = wrap("one\ntwo", 40);
        assert_eq!(out, vec!["one", "two"]);
    }

    #[test]
    fn wrap_keeps_blank_lines_that_separate_paragraphs() {
        let out = wrap("a\n\nb", 40);
        assert_eq!(out, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_breaks_on_width() {
        let out = wrap("aaa bbb ccc", 7);
        assert_eq!(out, vec!["aaa bbb", "ccc"]);
    }

    #[test]
    fn wrap_keeps_the_indentation_of_a_wrapped_paragraph() {
        let out = wrap("  1. a very long numbered item here", 12);
        assert!(out.len() > 1);
        assert!(out[0].starts_with("  "));
        assert!(out[1].starts_with("  "), "continuation keeps the indent");
    }

    #[test]
    fn wrap_does_not_lose_a_word_longer_than_the_width() {
        // a single word that cannot fit is emitted on its own, uncut, so that
        // nothing disappears from a log line or a URL
        let out = wrap("supercalifragilistic", 5);
        assert_eq!(out, vec!["supercalifragilistic"]);
    }

    #[test]
    fn wrap_of_nothing_is_one_empty_line() {
        assert_eq!(wrap("", 10), vec![""]);
    }

    // --- truncation ---

    #[test]
    fn truncate_pad_fills_short_text_to_the_width() {
        assert_eq!(truncate_pad("ab", 5), "ab   ");
    }

    #[test]
    fn truncate_pad_leaves_exact_text_alone() {
        assert_eq!(truncate_pad("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_pad_marks_what_it_cut() {
        let out = truncate_pad("abcdefgh", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_pad_handles_a_zero_width() {
        assert_eq!(truncate_pad("abc", 0), "");
    }

    #[test]
    fn truncate_to_counts_columns_not_bytes() {
        // accented letters are one column each despite being two bytes
        assert_eq!(truncate_to("áéíóú", 3), "áéí");
        // and a wide glyph takes two columns, so only one fits in three
        assert_eq!(truncate_to("→→", 3), "→→");
    }

    // --- scrolling ---

    #[test]
    fn scroll_into_view_does_nothing_when_the_selection_is_visible() {
        let mut off = 5;
        scroll_into_view(&mut off, 7, 10, 100);
        assert_eq!(off, 5);
    }

    #[test]
    fn scroll_into_view_follows_the_selection_up_and_down() {
        let mut off = 10;
        scroll_into_view(&mut off, 3, 5, 100);
        assert_eq!(off, 3, "scrolls up to reveal the selection");

        let mut off = 0;
        scroll_into_view(&mut off, 12, 5, 100);
        assert_eq!(off, 8, "scrolls down just enough");
    }

    #[test]
    fn scroll_into_view_never_scrolls_past_the_end() {
        let mut off = 90;
        scroll_into_view(&mut off, 5, 10, 20);
        assert!(off <= 10, "offset stays within len - height");
    }

    #[test]
    fn scroll_into_view_copes_with_a_list_shorter_than_the_window() {
        let mut off = 3;
        scroll_into_view(&mut off, 0, 20, 2);
        assert_eq!(off, 0);
    }

    #[test]
    fn scroll_into_view_ignores_a_zero_height_pane() {
        let mut off = 4;
        scroll_into_view(&mut off, 9, 0, 50);
        assert_eq!(off, 4, "a pane with no rows cannot scroll");
    }

    // --- buffer writing ---

    fn buffer(w: u16, h: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, w, h))
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn put_stops_at_the_limit_instead_of_overflowing() {
        let mut buf = buffer(10, 1);
        let end = put(&mut buf, 0, 0, 5, "abcdefgh", Style::default());
        assert_eq!(end, 5);
        assert_eq!(row(&buf, 0), "abcde");
    }

    #[test]
    fn put_off_the_bottom_of_the_buffer_is_a_no_op() {
        let mut buf = buffer(10, 1);
        let end = put(&mut buf, 0, 9, 10, "abc", Style::default());
        assert_eq!(end, 0, "nothing was written");
    }

    #[test]
    fn put_trunc_adds_an_ellipsis_only_when_it_cuts() {
        let mut buf = buffer(10, 1);
        put_trunc(&mut buf, 0, 0, 10, "abc", Style::default());
        assert_eq!(row(&buf, 0), "abc");

        let mut buf = buffer(10, 1);
        put_trunc(&mut buf, 0, 0, 4, "abcdefgh", Style::default());
        assert_eq!(row(&buf, 0), "abc…");
    }

    #[test]
    fn put_right_aligns_against_the_right_edge() {
        let mut buf = buffer(10, 1);
        let x = put_right(&mut buf, 10, 0, "abc", Style::default());
        assert_eq!(x, 7);
        assert_eq!(row(&buf, 0), "       abc");
    }

    #[test]
    fn put_right_clamps_text_wider_than_the_space() {
        let mut buf = buffer(4, 1);
        let x = put_right(&mut buf, 2, 0, "abcdefgh", Style::default());
        assert_eq!(x, 0, "it starts at the left edge rather than underflowing");
    }

    #[test]
    fn a_wide_glyph_clears_the_cell_it_covers() {
        // otherwise the second half keeps whatever was underneath
        let mut buf = buffer(4, 1);
        put(&mut buf, 0, 0, 4, "xx", Style::default());
        put(&mut buf, 0, 0, 4, "漢", Style::default());
        assert_eq!(buf[(1, 0)].symbol(), " ");
    }

    #[test]
    fn a_wide_glyph_that_does_not_fit_is_not_written() {
        // writing only half of it would corrupt the row
        let mut buf = buffer(4, 1);
        let end = put(&mut buf, 0, 0, 1, "漢", Style::default());
        assert_eq!(end, 0);
        assert_eq!(buf[(0, 0)].symbol(), " ");
    }

    // --- loading skeletons ---

    #[test]
    fn a_skeleton_bar_fills_exactly_its_width() {
        let mut buf = buffer(10, 1);
        skel_bar(&mut buf, 2, 0, 4, 0, 0);
        assert_eq!(row(&buf, 0), "  ████");
    }

    #[test]
    fn a_zero_width_bar_draws_nothing() {
        let mut buf = buffer(6, 1);
        skel_bar(&mut buf, 0, 0, 0, 0, 0);
        assert_eq!(row(&buf, 0), "");
    }

    #[test]
    fn the_highlight_band_travels_with_the_phase() {
        // the row the band is on is brighter than the rows away from it
        let lit = |row: usize, phase: u64| {
            let mut buf = buffer(4, 1);
            skel_bar(&mut buf, 0, 0, 2, row, phase);
            buf[(0, 0)].style().fg
        };
        assert_ne!(
            lit(0, 0),
            lit(5, 0),
            "row 0 is lit at phase 0, row 5 is not"
        );
        assert_eq!(lit(0, 0), lit(3, 3), "the band moved down with the phase");
    }

    #[test]
    fn a_bar_never_writes_past_its_width() {
        let mut buf = buffer(8, 1);
        skel_bar(&mut buf, 6, 0, 5, 0, 0); // would run off the right edge
        assert_eq!(row(&buf, 0).chars().count(), 8);
    }

    #[test]
    fn pct_keeps_proportions_and_never_overflows() {
        assert_eq!(pct(100, 50), 50);
        assert_eq!(pct(0, 80), 0);
        assert_eq!(pct(u16::MAX, 100), u16::MAX);
    }

    #[test]
    fn clear_wipes_the_text_underneath() {
        let mut buf = buffer(6, 1);
        put(&mut buf, 0, 0, 6, "abcdef", Style::default());
        clear(&mut buf, Rect::new(0, 0, 6, 1), theme::panel());
        assert_eq!(row(&buf, 0), "");
    }
}
