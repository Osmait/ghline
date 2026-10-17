//! Which words of a line changed, against the line it replaced.
//!
//! Two strings in, two byte ranges out: the stretch of each that differs from
//! the other, after the head and the tail they share, widened to whole words
//! so a renamed `total` shows as the word and not its last two letters. Which
//! lines are paired off is the caller's business — this knows nothing of
//! diffs, only of two lines of text.

use std::ops::Range;

/// The span of each line that differs from the other, widened to whole words.
///
/// `None` when the mark would cover all of both lines: that says no more than
/// their colour already does. Also `None` when they are the same, which `w`
/// can leave behind.
pub fn changed(old: &str, new: &str) -> Option<(Range<usize>, Range<usize>)> {
    let pre = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((i, _), _)| i);
    // the common tail, but never reaching back into the common head
    let suf = old[pre..]
        .chars()
        .rev()
        .zip(new[pre.min(new.len())..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    let old_r = widen(old, pre, old.len() - suf);
    let new_r = widen(new, pre, new.len() - suf);
    // checked after widening: `alpha` and `omega` share a letter, and the
    // word around it is the whole of both
    let whole = old_r == (0..old.len()) && new_r == (0..new.len());
    if whole || (old_r.is_empty() && new_r.is_empty()) {
        return None;
    }
    Some((old_r, new_r))
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Stretches `from..to` out to the edges of the words it cuts through, so a
/// renamed `total` shows as the word and not as its last two letters.
///
/// Only a word the change cuts into is taken whole. A change that starts at
/// `, b` is left alone rather than swallowing the `a` beside it, and an empty
/// range — the side of a pure insertion that lost nothing — stays empty.
fn widen(s: &str, from: usize, to: usize) -> Range<usize> {
    let to = to.max(from);
    if from == to {
        return from..to;
    }
    let first_is_word = s[from..].chars().next().is_some_and(word);
    let last_is_word = s[..to].chars().next_back().is_some_and(word);
    let start = if first_is_word {
        s[..from]
            .char_indices()
            .rev()
            .take_while(|(_, c)| word(*c))
            .last()
            .map_or(from, |(i, _)| i)
    } else {
        from
    };
    let end = if last_is_word {
        s[to..]
            .char_indices()
            .take_while(|(_, c)| word(*c))
            .last()
            .map_or(to, |(i, c)| to + i + c.len_utf8())
    } else {
        to
    };
    start..end
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "assertions")]
mod tests {
    use super::*;

    fn marked<'a>(old: &'a str, new: &'a str) -> Option<(&'a str, &'a str)> {
        changed(old, new).map(|(o, n)| (&old[o], &new[n]))
    }

    #[test]
    fn a_renamed_word_is_marked_whole_on_both_sides() {
        assert_eq!(
            marked("let side = total / 4;", "let side = width / 4;"),
            Some(("total", "width"))
        );
    }

    #[test]
    fn lines_with_nothing_in_common_are_not_marked() {
        assert_eq!(changed("alpha", "omega"), None);
    }

    #[test]
    fn an_insertion_marks_only_the_side_that_gained_it() {
        assert_eq!(marked("foo(a)", "foo(a, b)"), Some(("", ", b")));
    }

    #[test]
    fn a_change_past_multibyte_text_does_not_cut_a_character() {
        assert_eq!(marked("\"漢字\" = 1", "\"漢字\" = 2"), Some(("1", "2")));
    }

    #[test]
    fn the_same_line_twice_has_nothing_to_mark() {
        assert_eq!(changed("same", "same"), None);
    }
}
