//! Atoms: the things that write cells.
//!
//! One job each, no layout decisions, and nothing here knows what is being
//! drawn — `put` writes text at a coordinate and stops at a limit, and that is
//! the whole of what it means. Everything visible in either program is
//! ultimately one of these calls.
//!
//! The rule that keeps them atoms: an atom takes a `Buffer`, a coordinate and
//! its own data, and calls nothing in this module but another atom.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::theme;

/// Paints the background of every cell in `area`, leaving the text alone.
///
/// The pair to `clear`: this is what a row's ground is painted with, where
/// whatever glyphs are already there — a mark, a tree's arrow — are meant to
/// survive. Use `clear` when they are not.
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
    // The invariant this program has broken twice: a tab measured as one
    // column and drawn as eight, and a diff line painted over the pane beside
    // it. Both were found by looking at a screenshot. `debug_assert` because
    // the cost of being wrong here is a wrong cell, and the cost of a panic
    // is a terminal left in raw mode with a queue of unsent comments in it.
    debug_assert!(
        x <= max_x,
        "asked to write at {x} with a right edge of {max_x}"
    );
    let mut cx = x;
    if y >= buf.area.bottom() {
        return cx;
    }
    for g in unicode_segmentation(text) {
        // A backstop, not the fix: text is expanded where it is read, in
        // `crate::shared::text`. But this is the one place every string in either
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
    debug_assert!(cx <= max_x, "wrote to {cx}, past the edge at {max_x}");
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

/// Writes `text` so that it ends at `right_x` (exclusive). Returns the x it
/// began at.
///
/// That return is the useful half: it is where the *next* thing to the left has
/// to stop, and laying the right of a line out first is how a row keeps its
/// right-aligned parts from being written over. `agent_row` does exactly this.
///
/// The width is measured in columns, not bytes and not `char`s, so an accented
/// letter takes one and a CJK glyph takes two. Text wider than `right_x` starts
/// at column 0 and is cut on the right rather than underflowing to the left.
pub fn put_right(buf: &mut Buffer, right_x: u16, y: u16, text: &str, style: Style) -> u16 {
    let w = text.width() as u16;
    let x = right_x.saturating_sub(w);
    put(buf, x, y, right_x, text, style);
    x
}

/// Draws `w` columns of `─` from (x, y), on the theme's background.
///
/// `w` is a count of columns rather than a right edge, which is the opposite of
/// `put`'s `max_x` and the reason both spellings exist: a rule is nearly always
/// asked for as "the width of this pane", and the callers that write
/// `area.width` would otherwise all write `area.right()` and one of them would
/// get it wrong.
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

/// Draws `h` rows of `│` down column `x` from `y`, on the theme's background.
///
/// `h` is a count of rows, matching `hline`'s count of columns. One cell per
/// row rather than one `put` of a repeated glyph, because a buffer is laid out
/// by rows and there is no vertical run to write.
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

/// The same style, emboldened.
///
/// Here so that a call site can say `bold(base)` inline where it would
/// otherwise break the expression up to reach `add_modifier`. It adds to the
/// modifiers rather than replacing them, so it composes with whatever the base
/// already carries.
pub fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}

/// Dims what is underneath, like the design's `background: #0b0e14bb`.
pub fn scrim(buf: &mut Buffer, area: Rect) {
    let shade = |c: Color| match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * 0.42) as u8,
            (g as f32 * 0.42) as u8,
            (b as f32 * 0.42) as u8,
        ),
        other => other,
    };
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let s = cell.style();
                let fg = s.fg.unwrap_or(theme::fg());
                let bg = s.bg.unwrap_or(theme::bg());
                cell.set_style(Style::default().fg(shade(fg)).bg(shade(bg)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::probe::{buffer, row};

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
