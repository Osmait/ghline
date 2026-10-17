//! Fuzzy matching for the finder.
//!
//! The haystacks here are repository names and issue titles — short strings,
//! thousands at most — so a scorer of this size is enough and keeps a
//! copyleft-licensed dependency out of the tree for something this small.
//!
//! The scoring follows what makes a match feel right rather than any
//! particular published algorithm: letters that run together are worth more
//! than letters scattered across the string, and a letter at the start of a
//! word is worth more than one in the middle, because that is where people
//! aim when they type an abbreviation.

/// Where a query character landed, so the view can underline it.
pub type Positions = Vec<usize>;

// letters that run together must outweigh letters that merely sit at word
// starts, or `g_i_t_x` would beat `github` for "git": every separator would
// hand out a boundary bonus
const CONSECUTIVE: i32 = 14;
const WORD_START: i32 = 10;
const STRING_START: i32 = 12;
const GAP: i32 = -1;
const EXACT_CASE: i32 = 2;

/// Scores `query` against `haystack`, greedily and left to right.
///
/// Returns `None` when the query is not a subsequence of the haystack at all.
/// A higher score is a better match; scores are only comparable between
/// candidates for the same query.
///
/// Greedy means the first letter that fits wins, even where a later one would
/// have scored better — "gt" against "git-host-tui" takes the `t` of "git",
/// not the one starting "tui". Finding the best alignment in general needs a
/// second pass over the string; the one alignment checked besides is the
/// query typed whole, where greedy goes worst — a path, whose first letters
/// are spent on folders before the name the query is.
///
/// ```rust
/// use fuzzy_match::score;
///
/// // Greedy: the `t` taken is the one inside "git", not the one starting "tui".
/// assert_eq!(score("gt", "git-host-tui").map(|(_, at)| at), Some(vec![0, 2]));
/// assert!(score("tg", "git-host-tui").is_none(), "order matters");
/// ```
#[must_use]
pub fn score(query: &str, haystack: &str) -> Option<(i32, Positions)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    // Sized up front. This allocates for candidates that turn out not to
    // match, which looks like the obvious thing to fix, and both cheaper
    // spellings were written and measured and are slower.
    //
    // Leaving it empty so a failure allocates nothing: 18% slower here, 32%
    // over a list. A query whose *first* character misses is the rare one —
    // "zzqx" against "ada-example/…" matches its `z` inside "ada-example" and only
    // fails on the second — so the push happens on nearly every candidate
    // anyway, and all that changed was that the block arrived through `Vec`'s
    // outlined growth path instead of directly.
    //
    // Holding the positions in a stack array until the match is certain: that
    // does keep the failures allocation-free, and was slower again, 26% on a
    // single candidate. A finder spends its time on candidates that *match*,
    // and that path then writes every position twice, once into the array and
    // once into the `Vec` it has to return.
    let mut positions = Vec::with_capacity(query.chars().count());
    let mut total = 0;
    let mut previous_hit: Option<usize> = None;

    // One pass, carrying the character before the cursor rather than a copy of
    // the whole haystack. `at` only ever moved forwards, so the `Vec<char>`
    // this used to collect bought nothing and cost an allocation per
    // candidate — five hundred of them on every character typed into the
    // finder, which the profile showed outweighing the matching itself.
    let mut chars = haystack.chars().enumerate();
    let mut before: Option<char> = None;
    let mut seen = 0usize;

    for q in query.chars() {
        let ql = q.to_ascii_lowercase();
        // `?` on the exhausted iterator is the "not a subsequence" exit, and
        // it is why nothing after this needs the haystack's length yet.
        let (found, hit) = loop {
            let (i, c) = chars.next()?;
            seen = i + 1;
            if c.to_ascii_lowercase() == ql {
                break (i, c);
            }
            before = Some(c);
        };

        let mut points = 1;
        if found == 0 {
            points += STRING_START;
        } else if is_boundary(before, hit) {
            points += WORD_START;
        }
        if hit == q {
            points += EXACT_CASE;
        }
        if previous_hit == Some(found.wrapping_sub(1)) {
            points += CONSECUTIVE;
        } else if let Some(prev) = previous_hit {
            // distance costs, but never enough to reject a real match
            points += GAP * (found - prev - 1).min(10) as i32;
        }

        total += points;
        positions.push(found);
        previous_hit = Some(found);
        before = Some(hit);
    }

    // a short haystack that matched is a closer fit than a long one. Counted
    // rather than known, since the scan above stops at the last match — but
    // only on the paths that matched, which is the minority of candidates.
    let length = ((seen + chars.count()) / 8) as i32;
    total -= length;

    // Greedy takes the first letter that fits, and in a path that is the
    // wrong one: "tree.rs" against `crates/line-shared/src/tree.rs` spent
    // its `t` on "crates" and scattered the rest, losing to `theme.rs`. So a
    // query that is in the haystack as it was typed is scored there too, and
    // the better alignment wins. Only on a match, so only the minority pays.
    if let Some((run, start)) = contiguous(query, haystack)
        && run - length > total
    {
        // the same `Vec`, refilled: it already holds one slot per letter, so
        // this costs no allocation the greedy pass had not already made
        let n = positions.len();
        positions.clear();
        positions.extend(start..start + n);
        return Some((run - length, positions));
    }
    Some((total, positions))
}

/// The best place `query` sits in `haystack` letter for letter, ignoring
/// case — its score, before the length is taken off, and the character it
/// starts at. Walks the haystack in place: the tests below hold `score` to
/// the one allocation of its positions, and this is inside it.
fn contiguous(query: &str, haystack: &str) -> Option<(i32, usize)> {
    let n = query.chars().count();
    let same = |a: char, b: char| a.eq_ignore_ascii_case(&b);
    let mut best: Option<(i32, usize)> = None;
    let mut before: Option<char> = None;
    for (start, (byte, first)) in haystack.char_indices().enumerate() {
        let rest = &haystack[byte..];
        let fits = rest.chars().take(n).count() == n
            && rest.chars().zip(query.chars()).all(|(h, q)| same(h, q));
        if fits {
            let mut total = 0;
            for (k, (h, q)) in rest.chars().zip(query.chars()).enumerate() {
                let mut points = 1;
                if k == 0 {
                    points += if start == 0 {
                        STRING_START
                    } else if is_boundary(before, h) {
                        WORD_START
                    } else {
                        0
                    };
                } else {
                    points += CONSECUTIVE;
                }
                if h == q {
                    points += EXACT_CASE;
                }
                total += points;
            }
            if best.is_none_or(|(b, _)| total > b) {
                best = Some((total, start));
            }
        }
        before = Some(first);
    }
    best
}

/// Is this the first letter of a word? Separators, and the lowercase-to-
/// uppercase step of camelCase, both start one.
///
/// `before` is `None` at the start of the haystack, where the answer is yes.
#[must_use]
fn is_boundary(before: Option<char>, at: char) -> bool {
    let Some(prev) = before else {
        return true;
    };
    if matches!(prev, '-' | '_' | '/' | '.' | ' ' | ':') {
        return true;
    }
    prev.is_lowercase() && at.is_uppercase()
}

/// Keeps what matches, best first. Ties keep their original order, so a list
/// with no query is left exactly as it was given.
///
/// The indices are into `items`, not into a filtered copy of it — the caller
/// still owns the list, and a hit carries the positions the view underlines.
///
/// ```rust
/// use fuzzy_match::rank;
///
/// let repos = ["dotfiles", "git-host-tui", "gh-dash"];
/// let hits: Vec<&str> = rank("gh", &repos, |r| r).iter().map(|(i, _)| repos[*i]).collect();
/// assert_eq!(hits, ["gh-dash", "git-host-tui"], "dotfiles has no g before an h");
/// ```
#[must_use]
pub fn rank<T, F>(query: &str, items: &[T], text: F) -> Vec<(usize, Positions)>
where
    F: Fn(&T) -> &str,
{
    let mut hits: Vec<(usize, i32, Positions)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| score(query, text(it)).map(|(s, p)| (i, s, p)))
        .collect();
    hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    hits.into_iter().map(|(i, _, p)| (i, p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_typed_whole_finds_the_file_it_names_before_a_scatter() {
        let items = [
            "crates/tui-kit/src/theme.rs",
            "crates/line-shared/src/tree.rs",
        ];
        let ranked = rank("tree.rs", &items, |s| s);
        assert_eq!(
            ranked.first().map(|(i, _)| *i),
            Some(1),
            "tree.rs, not theme.rs"
        );
        let (_, at) = score("tree.rs", items[1]).unwrap();
        assert_eq!(
            at,
            (23..30).collect::<Vec<_>>(),
            "underlined where it is whole"
        );
    }

    fn s(q: &str, h: &str) -> Option<i32> {
        score(q, h).map(|(s, _)| s)
    }

    #[test]
    fn a_query_that_is_not_a_subsequence_does_not_match() {
        assert!(s("xyz", "git-host-tui").is_none());
        assert!(s("tuig", "git-host-tui").is_none(), "order matters");
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(s("", "anything"), Some(0));
    }

    #[test]
    fn matching_is_case_insensitive_but_prefers_the_exact_case() {
        assert!(s("GH", "git-host-tui").is_some());
        assert!(
            s("G", "Github").unwrap() > s("g", "Github").unwrap(),
            "the exact case is a better fit"
        );
    }

    #[test]
    fn letters_that_run_together_beat_letters_spread_out() {
        let together = s("git", "github").unwrap();
        let apart = s("git", "g_i_t_x").unwrap();
        assert!(together > apart, "{together} vs {apart}");
    }

    #[test]
    fn the_start_of_a_word_is_worth_more_than_the_middle() {
        // `st` as the initials of two words beats `st` inside one
        let initials = s("st", "second-thing").unwrap();
        let inside = s("st", "xxsxxtxx").unwrap();
        assert!(initials > inside, "{initials} vs {inside}");
    }

    #[test]
    fn camel_case_counts_as_a_word_boundary() {
        let camel = s("gp", "GestorDePresupuesto").unwrap();
        let buried = s("gp", "gxxxxpxxxx").unwrap();
        assert!(camel > buried, "{camel} vs {buried}");
    }

    #[test]
    fn a_shorter_haystack_wins_an_otherwise_equal_match() {
        let short = s("sb", "sbql").unwrap();
        let long = s("sb", "sbql-something-much-longer-indeed").unwrap();
        assert!(short > long, "{short} vs {long}");
    }

    #[test]
    fn positions_point_at_the_letters_that_matched() {
        let (_, pos) = score("gh", "git-host-tui").unwrap();
        assert_eq!(pos, vec![0, 4]);
    }

    #[test]
    fn matching_is_greedy_and_says_so() {
        // the `t` of "git" is taken, not the one that starts "tui": a
        // documented consequence of the single pass, not a defect
        let (_, pos) = score("gt", "git-host-tui").unwrap();
        assert_eq!(pos, vec![0, 2]);
    }

    #[test]
    fn ranking_puts_the_best_first_and_drops_the_rest() {
        let items = ["dotfiles", "git-host-tui", "gh-dash", "nvim"];
        let hits = rank("gh", &items, |s| s);
        let names: Vec<&str> = hits.iter().map(|(i, _)| items[*i]).collect();
        assert_eq!(names, vec!["gh-dash", "git-host-tui"], "nvim has no g-h");
    }

    #[test]
    fn ranking_with_no_query_keeps_the_original_order() {
        let items = ["c", "a", "b"];
        let hits = rank("", &items, |s| s);
        assert_eq!(
            hits.iter().map(|(i, _)| items[*i]).collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    /// See the twin in `tui::atom`: counted rather than timed, so it means the
    /// same thing on a shared runner as it does on a desk.
    fn allocations(f: impl FnOnce()) -> u64 {
        allocation_counter::measure(f).count_total
    }

    /// And its twin: bytes rather than blocks, which is what sees a copy.
    fn bytes(f: impl FnOnce()) -> u64 {
        allocation_counter::measure(f).bytes_total
    }

    #[test]
    fn scoring_allocates_the_positions_and_nothing_else() {
        // The copy of the haystack this used to collect was the other one.
        // Whatever is left has to be the `Vec` the caller is handed back,
        // because the finder pays this per candidate per keystroke.
        let n = allocations(|| {
            let _ = std::hint::black_box(score(
                std::hint::black_box("srn2"),
                std::hint::black_box("ada-example/some-repository-name-42"),
            ));
        });
        assert_eq!(n, 1);
    }

    #[test]
    fn scoring_does_not_copy_the_haystack() {
        // The `Vec<char>` this replaced was four bytes for every character it
        // was handed, so looking at a longer name cost more memory as well as
        // more time. What is left is the positions vector, and how big that
        // is, is the query's business rather than the candidate's.
        let short = "ada-example/repo-1";
        let long = format!(
            "ada-example/repo-1{}",
            "-with-a-much-longer-tail".repeat(40)
        );
        let a = bytes(|| {
            let _ = std::hint::black_box(score(std::hint::black_box("mr1"), short));
        });
        let b = bytes(|| {
            let _ = std::hint::black_box(score(std::hint::black_box("mr1"), &long));
        });
        assert_eq!(
            a,
            b,
            "{a} bytes for a name of {}, {b} for one of {}",
            short.len(),
            long.len()
        );
    }

    #[test]
    fn the_length_penalty_counts_characters_rather_than_bytes() {
        // `ñ` is two bytes and one letter, and a name spelt with them is not
        // a worse match for it. What the walk has to keep true now that the
        // haystack is not collected into a `Vec<char>` first.
        assert_eq!(s("ab", "abnnnnnnnn"), s("ab", "abññññññññ"));
    }

    #[test]
    fn a_boundary_is_found_from_the_letter_before_it() {
        // the camelCase and separator rules both read the previous character,
        // which is carried along rather than indexed backwards
        assert!(s("dp", "GestorDePresupuesto").unwrap() > s("dp", "gestordxpresupuesto").unwrap());
        assert!(s("t", "a-tui").unwrap() > s("t", "aXtui").unwrap());
    }

    #[test]
    fn a_match_at_the_very_start_beats_the_same_match_later() {
        let start = s("ai", "ai-status").unwrap();
        let later = s("ai", "my-ai-status").unwrap();
        assert!(start > later, "{start} vs {later}");
    }
}
