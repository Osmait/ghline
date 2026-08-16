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
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
    // By `char`, which is what this has always meant — the `Vec<&str>` of
    // one-character slices it used to build first was a heap allocation on
    // the most-called function in the crate, and `str::width` on each of
    // those slices ran a state machine backwards over one letter. Together
    // they were a third of what a frame cost, and the profile said so before
    // anything here was touched.
    for c in text.chars() {
        // A backstop, not the fix: text is expanded where it is read, in
        // `crate::shared::text`. But this is the one place every string in either
        // program passes through on its way to a cell, and a control
        // character reaching one moves the terminal's cursor somewhere the
        // layout did not account for — which is how a diff line ends up
        // painted over the pane beside it.
        if (c as u32) < 0x20 || c == '\x7f' {
            continue;
        }
        let w = c.width().unwrap_or(0) as u16;
        if w == 0 {
            continue;
        }
        if cx + w > max_x {
            break;
        }
        if let Some(cell) = buf.cell_mut((cx, y)) {
            cell.set_char(c);
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
    // Measured once and counted down. The loop this replaces asked the string
    // its width again for every space it appended, which is quadratic in the
    // padding — and padding a name to a column happens per row of every list.
    let used = out.width();
    out.reserve(w.saturating_sub(used));
    for _ in used..w {
        out.push(' ');
    }
    out
}

fn truncate_to(s: &str, w: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut used = 0;
    for c in s.chars() {
        // `char::width`, not `c.to_string().width()`: that spelling put a heap
        // allocation behind every character of every name that gets cut.
        let cw = c.width().unwrap_or(0);
        if used + cw > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    out
}

/// Lays text with hard breaks out into lines of `width` columns.
///
/// The line's width is carried along rather than re-measured. Building the
/// candidate line with `format!` and asking it its width once per word made
/// this quadratic in the length of a paragraph — a `format!` and a full walk
/// of everything already on the line, for every word appended to it.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let indent: String = para.chars().take_while(|c| *c == ' ').collect();
        // Spaces, so the column count is the character count; kept as a
        // separate name because it is added back on every continuation line.
        let indent_w = indent.len();
        let mut line = String::new();
        let mut line_w = 0usize;
        for word in para.split_whitespace() {
            let word_w = word.width();
            // The first word goes on whatever line is open even when it does
            // not fit: a word wider than the pane is emitted whole rather
            // than cut, so nothing disappears from a URL or a log line.
            if line.is_empty() {
                line.push_str(&indent);
                line.push_str(word);
                line_w = indent_w + word_w;
                continue;
            }
            if line_w + 1 + word_w > width {
                out.push(std::mem::take(&mut line));
                line.push_str(&indent);
                line.push_str(word);
                line_w = indent_w + word_w;
            } else {
                line.push(' ');
                line.push_str(word);
                line_w += 1 + word_w;
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
///
/// This walks every cell of the frame a modal covers, which is the whole
/// difference between what the help screen costs to draw and what an ordinary
/// pane costs. Two things came out of the loop: the theme, which was read
/// twice per cell through an atomic, and the arithmetic.
pub fn scrim(buf: &mut Buffer, area: Rect) {
    // `21/50` rather than `* 0.42`, because that is what 0.42 is. The float
    // spelling disagreed with it once in 256 — `0.42f32` is a hair under the
    // real thing, so a channel whose product lands exactly on an integer
    // truncated one short of it. No theme in the tree has such a channel, so
    // nothing on screen moves; the next theme somebody writes might.
    let shade = |c: Color| match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (u16::from(r) * 21 / 50) as u8,
            (u16::from(g) * 21 / 50) as u8,
            (u16::from(b) * 21 / 50) as u8,
        ),
        other => other,
    };
    let (fg_default, bg_default) = (theme::fg(), theme::bg());
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let s = cell.style();
                let fg = s.fg.unwrap_or(fg_default);
                let bg = s.bg.unwrap_or(bg_default);
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
    fn wrap_fills_a_line_to_exactly_the_width_before_breaking() {
        // the boundary the carried-along width has to land on: the space
        // between two words counts, so "aaa bbb" is seven columns and fits in
        // seven but not in six
        assert_eq!(wrap("aaa bbb", 7), vec!["aaa bbb"]);
        assert_eq!(wrap("aaa bbb", 6), vec!["aaa", "bbb"]);
    }

    #[test]
    fn wrap_measures_columns_rather_than_bytes() {
        // three two-column glyphs fill a width of six exactly, and an
        // accented letter is one column despite being two bytes
        assert_eq!(wrap("漢漢漢 x", 6), vec!["漢漢漢", "x"]);
        assert_eq!(wrap("áéí óú", 6), vec!["áéí óú"]);
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
    fn a_scrim_dims_the_colours_under_it() {
        let mut buf = buffer(1, 1);
        let style = Style::default()
            .fg(Color::Rgb(200, 100, 50))
            .bg(Color::Rgb(150, 25, 0));
        put(&mut buf, 0, 0, 1, "a", style);
        scrim(&mut buf, Rect::new(0, 0, 1, 1));

        let s = buf[(0, 0)].style();
        assert_eq!(s.fg, Some(Color::Rgb(84, 42, 21)));
        // 150 is the one channel in 256 where the old `* 0.42f32` truncated a
        // step short of what 0.42 means. 150 * 21 / 50 is 63, exactly.
        assert_eq!(s.bg, Some(Color::Rgb(63, 10, 0)));
    }

    #[test]
    fn a_scrim_leaves_a_colour_it_cannot_dim_alone() {
        // a terminal's own palette entry has no channels to scale
        let mut buf = buffer(1, 1);
        put(&mut buf, 0, 0, 1, "a", Style::default().fg(Color::Red));
        scrim(&mut buf, Rect::new(0, 0, 1, 1));
        assert_eq!(buf[(0, 0)].style().fg, Some(Color::Red));
    }

    /// How many times a piece of code reached the allocator.
    ///
    /// Counted rather than timed on purpose. Every performance defect this
    /// crate has had was a heap allocation on a path that did not need one,
    /// and a count is the same number on a busy laptop and a shared CI runner
    /// — which is what lets these sit in the ordinary test suite instead of
    /// in a benchmark nobody runs before pushing.
    fn allocations(f: impl FnOnce()) -> u64 {
        allocation_counter::measure(f).count_total
    }

    /// How many bytes it asked the allocator for, in total.
    ///
    /// The other half of the picture: a routine that copies what it has
    /// already built, once per item, allocates a growing block each time
    /// rather than more blocks. The count stays linear while the bytes go up
    /// with the square, so this is the number that sees a quadratic.
    fn bytes(f: impl FnOnce()) -> u64 {
        allocation_counter::measure(f).bytes_total
    }

    #[test]
    fn put_writes_a_row_without_reaching_the_allocator() {
        // The shape this replaced collected a `Vec` of one-character slices
        // before it wrote anything — on the function every label, every row
        // and every number in either program passes through. Nothing about
        // writing a string into cells needs the heap.
        let mut buf = buffer(200, 4);
        let n = allocations(|| {
            put(
                &mut buf,
                0,
                0,
                190,
                "src/github/state/app/input.rs",
                Style::default(),
            );
        });
        assert_eq!(n, 0);
    }

    #[test]
    fn padding_a_name_costs_the_same_whatever_the_column_is() {
        // The loop this replaced appended one space at a time, so the string
        // grew its way to the width and the allocations came with it. What
        // `reserve` buys is that the width stops being in the answer.
        let narrow = allocations(|| {
            let _ = std::hint::black_box(truncate_pad(std::hint::black_box("short"), 16));
        });
        let wide = allocations(|| {
            let _ = std::hint::black_box(truncate_pad(std::hint::black_box("short"), 4096));
        });
        assert_eq!(narrow, wide, "{narrow} for 16 columns, {wide} for 4096");
    }

    #[test]
    fn wrapping_a_paragraph_does_not_allocate_once_per_word() {
        // The shape this replaced built a `format!` for every word, so the
        // count rose with the words rather than with the lines.
        //
        // It also measured the whole accumulated line once per word, which
        // costs time rather than memory — a count cannot see that half, and
        // `make bench-cmp` is what does.
        let para = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do ".repeat(12);
        let words = para.split_whitespace().count() as u64;
        let n = allocations(|| {
            let _ = std::hint::black_box(wrap(std::hint::black_box(&para), 72));
        });
        assert!(n < words, "{n} allocations for {words} words");
    }

    #[test]
    fn wrapping_to_a_wider_column_does_not_copy_more() {
        // This is the quadratic itself, and the axis it lives on is the width
        // rather than the length. The shape this replaced rebuilt the whole
        // line with `format!` for every word added to it, so the second word
        // copied two and the tenth copied ten — but a line is capped by the
        // width, so a longer paragraph only bought more lines and stayed
        // linear. Widening the column is what lets each line grow, and it
        // took the copying up with the square: eight times the width cost six
        // times the bytes. The same text laid out wider is the same text.
        let text = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do ".repeat(24);
        let narrow = bytes(|| {
            let _ = std::hint::black_box(wrap(std::hint::black_box(&text), 40));
        });
        let wide = bytes(|| {
            let _ = std::hint::black_box(wrap(std::hint::black_box(&text), 320));
        });
        assert!(
            wide < narrow * 2,
            "{narrow} bytes at 40 columns, {wide} at 320"
        );
    }

    #[test]
    fn truncate_to_counts_columns_not_bytes() {
        // accented letters are one column each despite being two bytes
        assert_eq!(truncate_to("áéíóú", 3), "áéí");
        // and a wide glyph takes two columns, so only one fits in three
        assert_eq!(truncate_to("→→", 3), "→→");
    }
}
