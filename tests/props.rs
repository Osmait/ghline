//! Properties, where the rest of the suite has examples.
//!
//! Everything here is a parser: `git diff` output, a line of source, a line
//! about to be cut into terminal cells. They are the three places in the crate
//! that take input nobody in this repository wrote — a file from a repository
//! being reviewed can be minified JavaScript, a CSV with a quote in it, CRLF,
//! a combining accent, an emoji sequence, or forty megabytes on one line.
//!
//! An example test says "this input gives that output", and a hundred of them
//! say it a hundred times about inputs somebody thought of. What is asserted
//! here instead is what has to be true of *every* input: that the pieces a
//! line is wrapped into still spell the line, that an offset handed back can
//! be used to slice the string it came from, that nothing panics. `unwrap`,
//! `expect` and `panic!` are lints in this crate — these are the tests that
//! check the third one is not reached by arithmetic instead.
//!
//! When one fails, proptest shrinks the input to the smallest that still fails
//! and writes it to `tests/props.proptest-regressions`, which is committed:
//! the case is then a permanent example test, run first on every future run.

use proptest::prelude::*;

use github_tui::diffline::source::git::parse_unified;
use github_tui::shared::syntax;
use github_tui::shared::text::expand_tabs;

/// Text with the awkward parts over-represented: control characters, wide
/// glyphs, combining marks and the two newline conventions, none of which a
/// uniformly random `String` produces often enough to be worth waiting for.
fn awkward_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        4 => "[a-z ]{0,8}",
        2 => Just("\t".to_string()),
        2 => Just("\r\n".to_string()),
        1 => Just("\n".to_string()),
        // A wide glyph, a combining accent on its own, a zero-width joiner
        // sequence, and a right-to-left mark: four different disagreements
        // between "one character" and "one column".
        1 => prop_oneof![
            Just("漢".to_string()),
            Just("\u{0301}".to_string()),
            Just("👩\u{200d}💻".to_string()),
            Just("\u{200f}".to_string()),
        ],
        1 => "[\u{0}-\u{1f}\u{7f}]",
        1 => any::<String>(),
    ];
    proptest::collection::vec(piece, 0..24).prop_map(|v| v.concat())
}

// --- expand_tabs ---------------------------------------------------------
//
// Every line of every diff goes through this, and everything downstream —
// the width the layout thinks a line is, the byte offset a comment is
// anchored to, the span a colour covers — is an offset into what it returns.

proptest! {
    /// The whole point of the function. A tab reaching the terminal is
    /// measured as one column by us and drawn as up to eight by it, and every
    /// other control character moves the cursor somewhere we did not draw.
    #[test]
    fn nothing_that_moves_a_cursor_survives_expansion(s in awkward_text()) {
        for c in expand_tabs(&s).chars() {
            prop_assert!(
                c == '\n' || ((c as u32) >= 0x20 && c != '\u{7f}'),
                "{c:?} reached a cell",
            );
        }
    }

    /// Expanding twice is expanding once. It has to be: the text is expanded
    /// where it is read, and anything that expanded again on the way to the
    /// screen would move every offset taken in between.
    #[test]
    fn expanding_what_is_already_expanded_changes_nothing(s in awkward_text()) {
        let once = expand_tabs(&s).into_owned();
        prop_assert_eq!(&once, &expand_tabs(&once).into_owned());
    }

    /// Only spaces are added and only control characters are dropped — so
    /// with the spaces taken out of both sides, nothing has moved. This is
    /// what says a tab became indentation rather than eating the word after
    /// it.
    #[test]
    fn only_spacing_changes(s in awkward_text()) {
        let out = expand_tabs(&s);
        let kept: String = out.chars().filter(|c| *c != ' ').collect();
        let expected: String = s
            .chars()
            .filter(|c| *c != ' ')
            .filter(|c| *c == '\n' || ((*c as u32) >= 0x20 && *c != '\u{7f}'))
            .collect();
        prop_assert_eq!(kept, expected);
    }

    /// A line is still a line. The rows of a diff are numbered by counting
    /// them, so a function that could merge or split one would renumber the
    /// file.
    #[test]
    fn the_lines_are_the_same_lines(s in awkward_text()) {
        prop_assert_eq!(
            expand_tabs(&s).matches('\n').count(),
            s.matches('\n').count(),
        );
    }
}

// --- wrap_ranges ---------------------------------------------------------
//
// Byte ranges rather than substrings, because a colour span and a comment
// anchor are offsets and have to survive the wrap. Which means a range that
// is not on a character boundary is not a slow path — it is a panic in the
// pane that slices with it.

proptest! {
    /// The pieces spell the line, in order, once each.
    #[test]
    fn wrapping_loses_nothing_and_invents_nothing(
        line in awkward_text(),
        width in 0usize..40,
    ) {
        let ranges = syntax::wrap_ranges(&line, width);
        prop_assert!(!ranges.is_empty(), "a line always wraps to at least itself");
        prop_assert_eq!(ranges[0].0, 0);
        prop_assert_eq!(ranges[ranges.len() - 1].1, line.len());
        for w in ranges.windows(2) {
            prop_assert_eq!(w[0].1, w[1].0, "a gap or an overlap between pieces");
        }
        let rejoined: String = ranges
            .iter()
            .filter_map(|&(a, b)| line.get(a..b))
            .collect();
        prop_assert_eq!(rejoined, line);
    }

    /// Every offset can be sliced with. `line.get(a..b)` above returns `None`
    /// rather than panicking on a bad boundary, so this is what would catch
    /// one — and the callers use `&line[a..b]`, which does not.
    #[test]
    fn every_offset_is_a_character_boundary(
        line in awkward_text(),
        width in 0usize..40,
    ) {
        for (a, b) in syntax::wrap_ranges(&line, width) {
            prop_assert!(line.is_char_boundary(a), "{a} splits a character");
            prop_assert!(line.is_char_boundary(b), "{b} splits a character");
        }
    }

    /// No piece is wider than the pane, unless it is one character that is —
    /// which is the only honest answer for a glyph two columns wide in a
    /// column one wide, and is why the exception is written down rather than
    /// asserted away.
    ///
    /// On one expanded line, because that is the only thing this is ever
    /// called with and the only thing the property is true of. A control
    /// character measures zero columns asked one at a time and one column
    /// asked about a string — the disagreement `expand_tabs` exists to
    /// remove, and the reason the pane expands the text and splits the lines
    /// before it wraps anything. Asserting around it here would be asserting
    /// about text that never reaches this function.
    #[test]
    fn a_piece_fits_the_width_it_was_given(
        raw in awkward_text(),
        width in 1usize..40,
    ) {
        use unicode_width::UnicodeWidthStr;
        let line = expand_tabs(&raw).replace('\n', "");
        for (a, b) in syntax::wrap_ranges(&line, width) {
            let Some(piece) = line.get(a..b) else { continue };
            prop_assert!(
                piece.width() <= width || piece.chars().count() == 1,
                "{piece:?} is {} columns in a pane {width} wide",
                piece.width(),
            );
        }
    }
}

// --- highlight -----------------------------------------------------------

/// One language of each shape the lexer knows: block comments and escapes,
/// a hash comment and none, and a markup language whose strings are not
/// strings.
fn langs() -> Vec<&'static syntax::Lang> {
    ["x.rs", "x.py", "x.toml", "x.sh", "x.md", "x.json"]
        .iter()
        .filter_map(|p| syntax::of_path(p))
        .collect()
}

proptest! {
    /// The pane draws a line by slicing it with the spans it was given, so a
    /// span that runs past the end, or stops halfway through a character, is
    /// a panic rather than a wrong colour.
    #[test]
    fn a_span_can_always_slice_the_line_it_describes(src in awkward_text()) {
        for lang in langs() {
            let lines: Vec<&str> = src.lines().collect();
            let spans = syntax::highlight(lang, &src);
            prop_assert_eq!(
                spans.len(),
                lines.len(),
                "one vector of spans per line, always",
            );
            for (line, spans) in lines.iter().zip(&spans) {
                for s in spans {
                    prop_assert!(s.from < s.to, "an empty span is not worth drawing");
                    prop_assert!(s.to <= line.len(), "{} is past the end of the line", s.to);
                    prop_assert!(line.is_char_boundary(s.from), "{} splits a character", s.from);
                    prop_assert!(line.is_char_boundary(s.to), "{} splits a character", s.to);
                }
            }
        }
    }

    /// Spans arrive in order and do not overlap. The renderer walks them once,
    /// filling the gaps with the default colour as it goes; two that overlap
    /// would draw the same cells twice, and one that went backwards would
    /// draw a negative gap.
    #[test]
    fn spans_are_in_order_and_do_not_overlap(src in awkward_text()) {
        for lang in langs() {
            for spans in syntax::highlight(lang, &src) {
                for w in spans.windows(2) {
                    prop_assert!(
                        w[0].to <= w[1].from,
                        "{:?} and {:?} overlap or are out of order",
                        w[0],
                        w[1],
                    );
                }
            }
        }
    }
}

// --- parse_unified -------------------------------------------------------

/// Something shaped like `git diff` output, including the shapes git would
/// never produce. The line numbers are drawn from the whole of `u32` on
/// purpose: a hunk header is text, and what is on the other side of it is
/// arithmetic.
fn diff_text() -> impl Strategy<Value = String> {
    // Weighted towards the end of `u32` as well as spread across it. A
    // uniform `u32` is very unlikely to land near the top, and the top is
    // where a counter that only ever increments gets interesting.
    let start = prop_oneof![
        2 => any::<u32>(),
        1 => (u32::MAX - 4)..=u32::MAX,
        1 => 0u32..4,
    ];
    let line = prop_oneof![
        3 => (start.clone(), 0u32..8, start, 0u32..8)
            .prop_map(|(a, b, c, d)| format!("@@ -{a},{b} +{c},{d} @@ fn thing()")),
        1 => Just("@@".to_string()),
        1 => Just("@@ -x,y +z @@".to_string()),
        // Always a sign, because only a line that starts with one becomes a
        // row — anything else is skipped, and a generator that mostly makes
        // skipped lines is mostly testing nothing.
        6 => "[+\\- ][a-z(){};\t ]{0,12}",
        2 => "[+\\- \\\\]?[a-z(){};\t ]{0,12}",
        2 => awkward_text(),
    ];
    proptest::collection::vec(line, 0..30).prop_map(|v| v.join("\n"))
}

proptest! {
    /// Both counters only ever move forwards. They are what the two number
    /// columns show, and a row numbered below the row above it is a diff that
    /// cannot be read — or, if the counter wrapped to get there, one that
    /// panicked in a debug build before anybody saw it.
    #[test]
    fn the_line_numbers_never_go_backwards(text in diff_text()) {
        use github_tui::diffline::model::Kind;

        let (mut old, mut new) = (0u32, 0u32);
        for row in parse_unified(&text) {
            if row.kind == Kind::Header {
                // A header restarts the counting wherever it says.
                old = row.old.unwrap_or(0);
                new = row.new.unwrap_or(0);
                continue;
            }
            if let Some(o) = row.old {
                prop_assert!(o >= old, "{o} is above the {old} on the row before it");
                old = o;
            }
            if let Some(n) = row.new {
                prop_assert!(n >= new, "{n} is above the {new} on the row before it");
                new = n;
            }
        }
    }

    /// Rows carry text that is ready to draw, because they are drawn without
    /// being touched again.
    #[test]
    fn a_row_holds_no_control_characters(text in diff_text()) {
        for row in parse_unified(&text) {
            for c in row.text.chars() {
                prop_assert!(
                    (c as u32) >= 0x20 && c != '\u{7f}',
                    "{c:?} in a row of the diff",
                );
            }
        }
    }

    /// Everything before the first `@@` is preamble — the `diff --git`, the
    /// `index`, the `---` and `+++` — and the pane's header already says what
    /// file this is.
    ///
    /// Its own generator rather than `diff_text` with the hunks filtered out:
    /// nearly every value `diff_text` produces has a `@@` in it somewhere, so
    /// filtering would throw away almost every case and then give up.
    #[test]
    fn text_with_no_hunk_in_it_has_no_rows(
        lines in proptest::collection::vec(
            prop_oneof![
                "[+\\- \\\\]?[a-z(){};\t ]{0,12}",
                Just("diff --git a/x b/x".to_string()),
                Just("index 0123456..789abcd 100644".to_string()),
                Just("--- a/x".to_string()),
                Just("+++ b/x".to_string()),
                awkward_text(),
            ],
            0..20,
        ),
    ) {
        let text = lines.join("\n");
        prop_assume!(!text.lines().any(|l| l.starts_with("@@")));
        prop_assert!(parse_unified(&text).is_empty());
    }
}
