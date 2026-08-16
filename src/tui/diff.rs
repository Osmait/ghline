//! Folding a unified diff into side-by-side lines.
//!
//! Both programs do this, and both wrote it out: a hunk header stands alone,
//! a context line stands on both sides, and a run of deletions is zipped with
//! the run of additions that follows it — because that is the shape of an
//! edit, and stopping on each line of a twelve-line replacement would make
//! the view useless on exactly the diffs it is for.
//!
//! It works in indices rather than rows, and takes only the *kinds*. That is
//! what lets one copy serve two programs whose rows are different types: this
//! algorithm never looks at the text, the line numbers or anything else, so
//! there is nothing here for a row type to disagree about.

/// Which side of a diff a row belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The `@@ … @@` line.
    Header,
    /// Unchanged, and so on both sides.
    Context,
    /// A `-` line: it goes on the left, opposite whatever replaced it.
    Deleted,
    /// A `+` line: it goes on the right, opposite whatever it replaced.
    Added,
}

/// One line of a side-by-side view.
///
/// Indices into whatever was passed in, so everything the caller anchors to a
/// row — a cursor, a selection, a comment badge — keeps working without this
/// knowing those things exist.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Pair {
    /// The old side. `None` where the run of additions outlasted the run of
    /// deletions, and the honest thing to draw is a void.
    pub left: Option<usize>,
    /// The new side. `None` the other way round.
    pub right: Option<usize>,
    /// Set instead of both sides on a `@@ … @@` line, which spans the width
    /// rather than belonging to either column.
    pub header: Option<usize>,
}

/// Folds `sides` into side-by-side lines.
///
/// Where the runs are uneven the surplus gets a blank opposite it, which is
/// the honest answer: nothing was there.
pub fn pair(sides: &[Side]) -> Vec<Pair> {
    let mut out = Vec::with_capacity(sides.len());
    let mut i = 0;
    while i < sides.len() {
        match sides[i] {
            Side::Header => {
                out.push(Pair {
                    header: Some(i),
                    ..Pair::default()
                });
                i += 1;
            }
            Side::Context => {
                out.push(Pair {
                    left: Some(i),
                    right: Some(i),
                    ..Pair::default()
                });
                i += 1;
            }
            Side::Deleted | Side::Added => {
                let del_from = i;
                while i < sides.len() && sides[i] == Side::Deleted {
                    i += 1;
                }
                let add_from = i;
                while i < sides.len() && sides[i] == Side::Added {
                    i += 1;
                }
                let dels = del_from..add_from;
                let adds = add_from..i;
                for k in 0..dels.len().max(adds.len()) {
                    out.push(Pair {
                        left: dels.clone().nth(k),
                        right: adds.clone().nth(k),
                        ..Pair::default()
                    });
                }
            }
        }
    }

    // The invariant the whole thing rests on: a cursor is a row index, so a
    // row this never places is one you can select and not see, and one placed
    // twice is a line you can comment on from two places and queue twice.
    // Context counts twice on purpose — it is the same line on both sides.
    debug_assert!(
        {
            let mut seen = vec![0usize; sides.len()];
            for p in &out {
                for i in [p.left, p.right, p.header].into_iter().flatten() {
                    seen[i] += 1;
                }
            }
            sides
                .iter()
                .enumerate()
                .all(|(i, side)| seen[i] == if *side == Side::Context { 2 } else { 1 })
        },
        "a row was folded twice or not at all"
    );
    out
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

    #[test]
    fn an_edit_puts_what_went_beside_what_came() {
        // two lines removed and two added is one edit, not four events
        let p = pair(&[Side::Deleted, Side::Deleted, Side::Added, Side::Added]);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].left, p[0].right), (Some(0), Some(2)));
        assert_eq!((p[1].left, p[1].right), (Some(1), Some(3)));
    }

    #[test]
    fn an_uneven_edit_leaves_the_short_side_blank() {
        // one removed, three added: the two extra came from nowhere, and
        // saying so is the point of the empty half
        let p = pair(&[Side::Deleted, Side::Added, Side::Added, Side::Added]);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].left, Some(0));
        assert_eq!(p[1].left, None);
        assert_eq!(p[2].left, None);
        assert_eq!(
            p.iter().filter_map(|q| q.right).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn additions_with_no_deletions_are_all_on_the_right() {
        let p = pair(&[Side::Added, Side::Added]);
        assert!(p.iter().all(|q| q.left.is_none()));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn deletions_with_no_additions_are_all_on_the_left() {
        let p = pair(&[Side::Deleted, Side::Deleted]);
        assert!(p.iter().all(|q| q.right.is_none()));
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn context_stands_on_both_sides_and_a_header_on_neither() {
        let p = pair(&[Side::Header, Side::Context]);
        assert_eq!(p[0].header, Some(0));
        assert_eq!((p[0].left, p[0].right), (None, None));
        assert_eq!((p[1].left, p[1].right), (Some(1), Some(1)));
    }

    #[test]
    fn every_row_appears_exactly_once() {
        // A cursor is a row index. A row this never draws is one you can
        // select and not see; one drawn twice is a line you can comment on
        // from two places and queue twice.
        let sides = [
            Side::Header,
            Side::Context,
            Side::Deleted,
            Side::Added,
            Side::Added,
            Side::Context,
            Side::Deleted,
        ];
        let mut seen = vec![0usize; sides.len()];
        for p in pair(&sides) {
            for i in [p.left, p.right, p.header].into_iter().flatten() {
                seen[i] += 1;
            }
        }
        for (i, side) in sides.iter().enumerate() {
            // context counts twice on purpose: it is the same line both sides
            let want = if *side == Side::Context { 2 } else { 1 };
            assert_eq!(
                seen[i], want,
                "row {i} ({side:?}) appeared {} times",
                seen[i]
            );
        }
    }

    #[test]
    fn nothing_folds_to_nothing() {
        assert!(pair(&[]).is_empty());
    }
}
