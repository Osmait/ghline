//! The parts a terminal interface is made of.
//!
//! Both programs draw a grid of cells with the same hands, and this is where
//! the hands live: writing text that stops at an edge, filling a rectangle,
//! ruling a line, framing a modal, keeping a selection on screen.
//!
//! It is here rather than in either program because it belongs to neither.
//! These lived in `ui`, which is github-tui's, and diffline imported ten of
//! them out of it — the same arrow that `data` and `theme` had, a shared
//! thing inside one program's own. It cost what that always costs: `centered`,
//! `frame` and `rule` were written a second time in diffline and drifted, one
//! copy picking up a `saturating_sub` that the other never got.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::theme;

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
        // A backstop, not the fix: text is expanded where it is read, in
        // `crate::text`. But this is the one place every string in either
        // program passes through on its way to a cell, and a control
        // character reaching one moves the terminal's cursor somewhere the
        // layout did not account for — which is how a diff line ends up
        // painted over the pane beside it.
        if g.chars()
            .next()
            .is_some_and(|c| (c as u32) < 0x20 || c == '\x7f')
        {
            continue;
        }
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

// --- modals -----------------------------------------------------------------

/// Centred, never larger than what it is centred in.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 0, 0)
}

/// Centred with a gutter kept either side, so the thing underneath still
/// shows and the modal reads as floating over it rather than replacing it.
///
/// The difference from `centered` used to be the difference between the two
/// programs' copies of this function, which is to say it was an accident.
/// Named, it is a choice: diffline's modals sit over a diff you are still
/// reading, github-tui's cover a list you are done with.
pub fn centered_over(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 4, 2)
}

fn inset(area: Rect, w: u16, h: u16, mx: u16, my: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(mx));
    let h = h.min(area.height.saturating_sub(my));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// A single-line frame in the modal's accent colour.
///
/// `saturating_sub` throughout: one copy of this had a plain subtraction and
/// was saved only by a guard elsewhere refusing to draw below forty columns.
/// A drawing primitive should not depend on somebody else's guard.
pub fn frame(buf: &mut Buffer, area: Rect, color: Color) {
    clear(buf, area, theme::panel());
    let s = Style::default().bg(theme::panel()).fg(color);
    let run = "─".repeat(area.width.saturating_sub(2) as usize);
    put(buf, area.x, area.y, area.right(), &format!("┌{run}┐"), s);
    put(
        buf,
        area.x,
        area.bottom().saturating_sub(1),
        area.right(),
        &format!("└{run}┘"),
        s,
    );
    for y in area.y + 1..area.bottom().saturating_sub(1) {
        put(buf, area.x, y, area.right(), "│", s);
        put(buf, area.right().saturating_sub(1), y, area.right(), "│", s);
    }
}

/// The modal's inner horizontal rule.
///
/// With the `├` and `┤` that join it to the frame — diffline's copy drew a
/// plain line and left a gap at both ends, which is what you get when a fix
/// lands on one of two copies.
pub fn rule(buf: &mut Buffer, area: Rect, y: u16, color: Color) {
    hline(
        buf,
        area.x + 1,
        y,
        area.width.saturating_sub(2),
        theme::border(),
    );
    let s = Style::default().bg(theme::panel()).fg(color);
    put(buf, area.x, y, area.right(), "├", s);
    put(buf, area.right().saturating_sub(1), y, area.right(), "┤", s);
    // the rule's own background is the modal's, not the pane's underneath
    for x in area.x + 1..area.right().saturating_sub(1) {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(theme::panel());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn truncate_to_counts_columns_not_bytes() {
        // accented letters are one column each despite being two bytes
        assert_eq!(truncate_to("áéíóú", 3), "áéí");
        // and a wide glyph takes two columns, so only one fits in three
        assert_eq!(truncate_to("→→", 3), "→→");
    }
}
