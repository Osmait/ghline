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
/// have scored better — "gt" against "github-tui" takes the `t` of "github",
/// not the one starting "tui". Finding the best alignment needs a second pass
/// over the string; for names this short the ranking is the same either way.
pub fn score(query: &str, haystack: &str) -> Option<(i32, Positions)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let hay: Vec<char> = haystack.chars().collect();
    let mut positions = Vec::with_capacity(query.chars().count());
    let mut total = 0;
    let mut at = 0usize;
    let mut previous_hit: Option<usize> = None;

    for q in query.chars() {
        let ql = q.to_ascii_lowercase();
        let found = (at..hay.len()).find(|&i| hay[i].to_ascii_lowercase() == ql)?;

        let mut points = 1;
        if found == 0 {
            points += STRING_START;
        } else if is_boundary(&hay, found) {
            points += WORD_START;
        }
        if hay[found] == q {
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
        at = found + 1;
    }

    // a short haystack that matched is a closer fit than a long one
    total -= (hay.len() / 8) as i32;
    Some((total, positions))
}

/// Is this the first letter of a word? Separators, and the lowercase-to-
/// uppercase step of camelCase, both start one.
fn is_boundary(hay: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = hay[i - 1];
    if matches!(prev, '-' | '_' | '/' | '.' | ' ' | ':') {
        return true;
    }
    prev.is_lowercase() && hay[i].is_uppercase()
}

/// Keeps what matches, best first. Ties keep their original order, so a list
/// with no query is left exactly as it was given.
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

    fn s(q: &str, h: &str) -> Option<i32> {
        score(q, h).map(|(s, _)| s)
    }

    #[test]
    fn a_query_that_is_not_a_subsequence_does_not_match() {
        assert!(s("xyz", "github-tui").is_none());
        assert!(s("tuig", "github-tui").is_none(), "order matters");
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(s("", "anything"), Some(0));
    }

    #[test]
    fn matching_is_case_insensitive_but_prefers_the_exact_case() {
        assert!(s("GH", "github-tui").is_some());
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
        let (_, pos) = score("gh", "github-tui").unwrap();
        assert_eq!(pos, vec![0, 3]);
    }

    #[test]
    fn matching_is_greedy_and_says_so() {
        // the `t` of "github" is taken, not the one that starts "tui": a
        // documented consequence of the single pass, not a defect
        let (_, pos) = score("gt", "github-tui").unwrap();
        assert_eq!(pos, vec![0, 2]);
    }

    #[test]
    fn ranking_puts_the_best_first_and_drops_the_rest() {
        let items = ["dotfiles", "github-tui", "gh-dash", "nvim"];
        let hits = rank("gh", &items, |s| s);
        let names: Vec<&str> = hits.iter().map(|(i, _)| items[*i]).collect();
        assert_eq!(names, vec!["gh-dash", "github-tui"], "nvim has no g-h");
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

    #[test]
    fn a_match_at_the_very_start_beats_the_same_match_later() {
        let start = s("ai", "ai-status").unwrap();
        let later = s("ai", "my-ai-status").unwrap();
        assert!(start > later, "{start} vs {later}");
    }
}
