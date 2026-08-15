//! Making a line of a file safe to put in a grid of cells.
//!
//! A terminal cell holds one column. A tab does not: it moves the cursor to
//! the next tab stop, which is somewhere between one and eight columns away
//! depending on where the tab was. Nothing that measures a string can know
//! that — `unicode-width` reports 1 for a tab through `UnicodeWidthStr` and
//! nothing at all through `UnicodeWidthChar`, so two callers measuring the
//! same line disagree, and both disagree with the terminal.
//!
//! The way out is to never hand a terminal a tab. Expanding it here, where
//! the text is read, means every later step — wrapping, truncating,
//! highlighting, the byte offsets a comment is anchored to — is working on
//! the same string that will end up on screen.

/// Columns a tab advances to. Four rather than the terminal's own eight: this
/// is read in a pane beside two others, and deeply indented code loses more to
/// eight columns of nothing than it gains in fidelity to the file.
pub const TAB: usize = 4;

/// `text` with tabs expanded and other control characters dropped.
///
/// Tab stops are counted from the start of the line, and a newline starts the
/// count again — so a whole file can be passed in one piece and each of its
/// lines is expanded the way an editor showing that line would.
///
/// Returns the input untouched when there is nothing to do, which is the usual
/// case — this runs over every line of every diff.
pub fn expand_tabs(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.bytes().any(|b| (b < 0x20 && b != b'\n') || b == 0x7f) {
        return std::borrow::Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len() + TAB);
    let mut col = 0usize;
    for c in text.chars() {
        match c {
            '\t' => {
                // to the next stop, and never zero: a tab always moves
                let n = TAB - (col % TAB);
                out.extend(std::iter::repeat_n(' ', n));
                col += n;
            }
            // Kept, and it starts the columns over: the next line's first tab
            // stop is four columns from that line's start, not from this one's.
            '\n' => {
                out.push('\n');
                col = 0;
            }
            // A carriage return would send the cursor back to column zero and
            // overwrite the line with itself; the rest are unprintable — a
            // `\x1b` in a log, a `\x00` in something not quite text.
            c if (c as u32) < 0x20 || c == '\x7f' => {}
            c => {
                col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                out.push(c);
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_with_nothing_to_fix_is_not_copied() {
        let s = "const x = 1;";
        assert!(matches!(expand_tabs(s), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn a_tab_goes_to_the_next_stop_not_a_fixed_width() {
        // the whole point: how far a tab moves depends on where it starts
        assert_eq!(expand_tabs("\tx"), "    x");
        assert_eq!(expand_tabs("a\tx"), "a   x");
        assert_eq!(expand_tabs("ab\tx"), "ab  x");
        assert_eq!(expand_tabs("abc\tx"), "abc x");
        assert_eq!(expand_tabs("abcd\tx"), "abcd    x");
    }

    #[test]
    fn a_tab_always_moves() {
        // a stop that landed exactly on the boundary must not expand to
        // nothing, or two tokens would run together
        assert_eq!(expand_tabs("abcd\t").len(), 8);
    }

    #[test]
    fn successive_tabs_stack() {
        assert_eq!(expand_tabs("\t\tx"), "        x");
    }

    #[test]
    fn the_result_never_costs_more_columns_than_it_claims() {
        use unicode_width::UnicodeWidthStr;
        for line in ["\tfoo", "a\tb\tc", "\t\t}", "x", "  y"] {
            let out = expand_tabs(line);
            // Every character left is one the two width functions agree on,
            // which is what the layout depends on.
            for c in out.chars() {
                assert_eq!(
                    unicode_width::UnicodeWidthChar::width(c).unwrap_or(0),
                    c.to_string().width(),
                    "{c:?} in {line:?} measures differently by char than by str"
                );
            }
        }
    }

    #[test]
    fn a_newline_survives_and_starts_the_columns_over() {
        // passed a whole file, every line has to expand as if it were alone
        assert_eq!(expand_tabs("ab\tx\n\ty"), "ab  x\n    y");
    }

    #[test]
    fn control_characters_are_dropped_rather_than_drawn() {
        assert_eq!(expand_tabs("a\x1b[31mb"), "a[31mb");
        assert_eq!(expand_tabs("a\rb"), "ab");
        assert_eq!(expand_tabs("a\x00b"), "ab");
    }

    #[test]
    fn wide_characters_still_count_for_two_against_the_stop() {
        // a CJK character fills two columns, so the tab after it has one to go
        assert_eq!(expand_tabs("漢\tx"), "漢  x");
    }
}
