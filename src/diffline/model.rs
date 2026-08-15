//! What a diff is, and where a comment sits in one.
//!
//! The subtle part is anchoring. A comment is attached to a *line of a file*,
//! not to a row on screen: rows are rebuilt whenever the context is expanded,
//! a different scope is chosen, or the file is re-read, and a comment written
//! against row 14 would then be against whatever row 14 became.
//!
//! So an anchor names the side and the number — `src/a.rs:n42` for the new
//! side, `:o42` for a line that only exists in the old one. Those survive
//! everything except the file changing underneath, which is the one case where
//! the comment genuinely is about something that is no longer there.

use std::fmt;

/// What is being diffed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Uncommitted changes, staged and not.
    WorkingTree,
    /// Everything this branch has that `base` does not — `git diff base...HEAD`,
    /// so a merge into base does not show up as this branch's work.
    Branch { base: String },
    /// One commit against its parent.
    Commit { sha: String },
}

impl Scope {
    /// What the header calls it.
    pub fn label(&self) -> String {
        match self {
            Self::WorkingTree => "working tree".into(),
            Self::Branch { base } => format!("branch…{base}"),
            Self::Commit { sha } => format!("commit {}", short(sha)),
        }
    }

    /// The arguments that select this scope, after `git diff`.
    pub fn args(&self) -> Vec<String> {
        match self {
            Self::WorkingTree => vec!["HEAD".into()],
            // Three dots: the changes this branch made, not the ones base made
            // while we were away. Two dots would blame us for both.
            Self::Branch { base } => vec![format!("{base}...HEAD")],
            Self::Commit { sha } => vec![format!("{sha}^!")],
        }
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

/// How a file changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl Status {
    /// The single letter git uses, which is also what the tree shows.
    pub fn mark(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
        }
    }

    /// The word, for a place with room for one — an agent reads "added"
    /// more readily than it reads "A".
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.chars().next() {
            Some('A') => Self::Added,
            Some('D') => Self::Deleted,
            Some('R') => Self::Renamed,
            _ => Self::Modified,
        }
    }
}

/// One file in the diff.
#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub path: String,
    pub status: Status,
    pub add: u32,
    pub del: u32,
}

impl ChangedFile {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn dir(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }
}

/// What a row of the diff pane is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The `@@ … @@` line.
    Header,
    /// A line present in both sides.
    Context,
    Added,
    Deleted,
}

impl Kind {
    /// Whether a comment can be attached here. A hunk header is a coordinate,
    /// not a line of anybody's code.
    pub fn is_code(self) -> bool {
        !matches!(self, Self::Header)
    }
}

/// One rendered row of a file's diff.
#[derive(Clone, Debug)]
pub struct Row {
    pub kind: Kind,
    /// Line number on the old side, when the row has one.
    pub old: Option<u32>,
    /// Line number on the new side, when the row has one.
    pub new: Option<u32>,
    pub text: String,
}

impl Row {
    /// The sign column: what `git diff` would print in front of this line.
    pub fn sign(&self) -> &'static str {
        match self.kind {
            Kind::Added => "+",
            Kind::Deleted => "-",
            Kind::Context => " ",
            Kind::Header => "",
        }
    }

    /// Where this row lives, for a comment to hold on to.
    ///
    /// The new side wins when a row has both, because that is the text that
    /// will still be there afterwards — a note on a context line is a note
    /// about the code as it will stand.
    pub fn anchor(&self, path: &str) -> Option<Anchor> {
        if !self.kind.is_code() {
            return None;
        }
        match (self.new, self.old) {
            (Some(n), _) => Some(Anchor {
                path: path.to_string(),
                side: Side::New,
                line: n,
            }),
            (None, Some(o)) => Some(Anchor {
                path: path.to_string(),
                side: Side::Old,
                line: o,
            }),
            (None, None) => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Old,
    New,
}

/// A place in a file that a comment is about.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Anchor {
    pub path: String,
    pub side: Side,
    pub line: u32,
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let side = match self.side {
            Side::Old => 'o',
            Side::New => 'n',
        };
        write!(f, "{}:{}{}", self.path, side, self.line)
    }
}

/// Where a comment is in its life.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Queued,
    Sending,
    Sent,
}

/// A note attached to one line or a run of them.
#[derive(Clone, Debug)]
pub struct Comment {
    /// The lines it is about. Empty when the note is about the file as a
    /// whole — "why is this here at all" is a question about a file, and
    /// there is no line to pin it to.
    pub anchors: Vec<Anchor>,
    /// The file it is about, always. The anchors, when there are any, agree
    /// with it; this is what survives when there are none.
    pub file: String,
    /// The first line of what it is about, for the queue to show.
    pub snippet: String,
    pub body: String,
    pub state: State,
}

impl Comment {
    pub fn path(&self) -> &str {
        &self.file
    }

    /// True for a note about the file rather than about any line in it.
    pub fn is_file_note(&self) -> bool {
        self.anchors.is_empty()
    }

    pub fn lines(&self) -> usize {
        self.anchors.len()
    }

    /// `router.ts:31-44`, or `router.ts:31` for a single line.
    pub fn where_label(&self) -> String {
        let name = self.file.rsplit('/').next().unwrap_or(&self.file);
        let Some(first) = self.anchors.first() else {
            return format!("{name} · the file");
        };
        let name = first.path.rsplit('/').next().unwrap_or(&first.path);
        let lo = self.anchors.iter().map(|a| a.line).min().unwrap_or(0);
        let hi = self.anchors.iter().map(|a| a.line).max().unwrap_or(0);
        if lo == hi {
            format!("{name}:{lo}")
        } else {
            format!("{name}:{lo}-{hi}")
        }
    }
}

/// One line of a side-by-side view: what is on the left, what is on the
/// right, or a header that spans both.
///
/// The fields are indices into the row list rather than rows, so that
/// everything anchored to a row index — the cursor, the selection, the
/// comment badges — keeps working without knowing this view exists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Pair {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub header: Option<usize>,
}

/// Folds a unified diff into side-by-side lines.
///
/// A run of deletions followed by a run of additions is the shape of an edit,
/// so the two runs are zipped: the first line removed sits beside the first
/// line added. When the runs are uneven the surplus gets a blank opposite it,
/// which is the honest answer — nothing was there.
pub fn pair_rows(rows: &[Row]) -> Vec<Pair> {
    let mut out = Vec::with_capacity(rows.len());
    let mut i = 0;
    while i < rows.len() {
        match rows[i].kind {
            Kind::Header => {
                out.push(Pair {
                    header: Some(i),
                    ..Pair::default()
                });
                i += 1;
            }
            Kind::Context => {
                out.push(Pair {
                    left: Some(i),
                    right: Some(i),
                    ..Pair::default()
                });
                i += 1;
            }
            Kind::Deleted | Kind::Added => {
                let del_from = i;
                while i < rows.len() && rows[i].kind == Kind::Deleted {
                    i += 1;
                }
                let add_from = i;
                while i < rows.len() && rows[i].kind == Kind::Added {
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: Kind, old: Option<u32>, new: Option<u32>) -> Row {
        Row {
            kind,
            old,
            new,
            text: "code".into(),
        }
    }

    // --- scopes ---

    #[test]
    fn a_branch_scope_asks_for_what_this_branch_did() {
        // three dots, not two: two would also show what base did meanwhile,
        // and this branch did not do that
        let s = Scope::Branch {
            base: "main".into(),
        };
        assert_eq!(s.args(), vec!["main...HEAD"]);
    }

    #[test]
    fn a_commit_scope_asks_for_that_commit_alone() {
        let s = Scope::Commit {
            sha: "c81d4a9f00".into(),
        };
        assert_eq!(s.args(), vec!["c81d4a9f00^!"]);
        assert_eq!(s.label(), "commit c81d4a9", "shortened for the header");
    }

    #[test]
    fn the_working_tree_is_everything_not_committed() {
        // `HEAD` rather than no argument, so staged changes are included:
        // a bare `git diff` would show only what is unstaged
        assert_eq!(Scope::WorkingTree.args(), vec!["HEAD"]);
    }

    #[test]
    fn a_short_sha_is_not_shortened_past_itself() {
        let s = Scope::Commit { sha: "abc".into() };
        assert_eq!(s.label(), "commit abc");
    }

    // --- files ---

    #[test]
    fn a_file_knows_its_name_and_its_directory() {
        let f = ChangedFile {
            path: "src/server/router.ts".into(),
            status: Status::Modified,
            add: 1,
            del: 0,
        };
        assert_eq!(f.name(), "router.ts");
        assert_eq!(f.dir(), "src/server");
    }

    #[test]
    fn a_file_at_the_root_has_no_directory() {
        let f = ChangedFile {
            path: "Makefile".into(),
            status: Status::Modified,
            add: 0,
            del: 0,
        };
        assert_eq!(f.name(), "Makefile");
        assert_eq!(f.dir(), "");
    }

    #[test]
    fn git_status_letters_map_to_what_they_mean() {
        assert_eq!(Status::parse("A"), Status::Added);
        assert_eq!(Status::parse("D"), Status::Deleted);
        assert_eq!(Status::parse("M"), Status::Modified);
        // git writes a similarity score after a rename: R096
        assert_eq!(Status::parse("R096"), Status::Renamed);
        assert_eq!(Status::parse(""), Status::Modified, "the safe assumption");
    }

    // --- anchors, which is the part that has to hold ---

    #[test]
    fn an_added_line_anchors_to_the_new_side() {
        let a = row(Kind::Added, None, Some(42)).anchor("src/a.rs").unwrap();
        assert_eq!(a.to_string(), "src/a.rs:n42");
    }

    #[test]
    fn a_deleted_line_anchors_to_the_old_side() {
        // it has no new-side number, because it will not be there
        let a = row(Kind::Deleted, Some(17), None)
            .anchor("src/a.rs")
            .unwrap();
        assert_eq!(a.to_string(), "src/a.rs:o17");
    }

    #[test]
    fn a_context_line_anchors_to_the_new_side() {
        // a note on unchanged code is about the code as it will stand
        let a = row(Kind::Context, Some(10), Some(12))
            .anchor("src/a.rs")
            .unwrap();
        assert_eq!(a.to_string(), "src/a.rs:n12");
    }

    #[test]
    fn a_hunk_header_cannot_be_commented_on() {
        // it is a coordinate, not anybody's code
        assert!(row(Kind::Header, None, None).anchor("src/a.rs").is_none());
        assert!(!Kind::Header.is_code());
    }

    #[test]
    fn an_anchor_survives_the_rows_being_rebuilt() {
        // The whole point: expanding the context renumbers every row on
        // screen, and the anchor is written against the file rather than
        // against the screen.
        let before = row(Kind::Added, None, Some(42)).anchor("a.rs").unwrap();
        let after = row(Kind::Added, None, Some(42)).anchor("a.rs").unwrap();
        assert_eq!(before, after);
    }

    // --- pairing ---

    fn rows_of(spec: &[Kind]) -> Vec<Row> {
        let (mut o, mut n) = (0, 0);
        spec.iter()
            .map(|k| {
                let (old, new) = match k {
                    Kind::Header => (None, None),
                    Kind::Deleted => {
                        o += 1;
                        (Some(o), None)
                    }
                    Kind::Added => {
                        n += 1;
                        (None, Some(n))
                    }
                    Kind::Context => {
                        o += 1;
                        n += 1;
                        (Some(o), Some(n))
                    }
                };
                Row {
                    kind: *k,
                    old,
                    new,
                    text: format!("{k:?}"),
                }
            })
            .collect()
    }

    #[test]
    fn an_edit_puts_what_went_beside_what_came() {
        // two lines removed and two added is one edit, not four events
        let rows = rows_of(&[Kind::Deleted, Kind::Deleted, Kind::Added, Kind::Added]);
        let pairs = pair_rows(&rows);
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            pairs[0],
            Pair {
                left: Some(0),
                right: Some(2),
                header: None
            }
        );
        assert_eq!(
            pairs[1],
            Pair {
                left: Some(1),
                right: Some(3),
                header: None
            }
        );
    }

    #[test]
    fn an_uneven_edit_leaves_the_short_side_blank() {
        // one removed, three added: the two extra lines came from nowhere,
        // and saying so is the point of the empty half
        let rows = rows_of(&[Kind::Deleted, Kind::Added, Kind::Added, Kind::Added]);
        let pairs = pair_rows(&rows);
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0].left, Some(0));
        assert_eq!(pairs[1].left, None);
        assert_eq!(pairs[2].left, None);
        assert_eq!(
            pairs.iter().filter_map(|p| p.right).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn context_stands_on_both_sides_and_a_header_on_neither() {
        let rows = rows_of(&[Kind::Header, Kind::Context]);
        let pairs = pair_rows(&rows);
        assert_eq!(pairs[0].header, Some(0));
        assert_eq!((pairs[0].left, pairs[0].right), (None, None));
        assert_eq!((pairs[1].left, pairs[1].right), (Some(1), Some(1)));
    }

    #[test]
    fn every_row_appears_exactly_once() {
        // The cursor is a row index. A row that the split view never draws is
        // a row you can select and not see; one drawn twice is a line you can
        // comment on from two places and queue twice.
        let rows = rows_of(&[
            Kind::Header,
            Kind::Context,
            Kind::Deleted,
            Kind::Added,
            Kind::Added,
            Kind::Context,
            Kind::Deleted,
        ]);
        let mut seen = vec![0usize; rows.len()];
        for p in pair_rows(&rows) {
            for i in [p.left, p.right, p.header].into_iter().flatten() {
                seen[i] += 1;
            }
        }
        // context counts twice on purpose: it is the same line on both sides
        for (i, r) in rows.iter().enumerate() {
            let want = if r.kind == Kind::Context { 2 } else { 1 };
            assert_eq!(
                seen[i], want,
                "row {i} ({:?}) appeared {} times",
                r.kind, seen[i]
            );
        }
    }

    // --- comments ---

    fn comment(lines: &[u32]) -> Comment {
        Comment {
            file: "src/server/router.ts".into(),
            anchors: lines
                .iter()
                .map(|n| Anchor {
                    path: "src/server/router.ts".into(),
                    side: Side::New,
                    line: *n,
                })
                .collect(),
            snippet: "const cache = …".into(),
            body: "scope this by org".into(),
            state: State::Queued,
        }
    }

    #[test]
    fn a_single_line_comment_reads_as_one_number() {
        assert_eq!(comment(&[31]).where_label(), "router.ts:31");
    }

    #[test]
    fn a_range_comment_reads_as_a_range() {
        assert_eq!(comment(&[31, 32, 33]).where_label(), "router.ts:31-33");
        assert_eq!(comment(&[31, 32, 33]).lines(), 3);
    }

    #[test]
    fn a_range_label_does_not_care_what_order_it_was_selected_in() {
        // selecting upwards is the same range as selecting downwards
        assert_eq!(comment(&[33, 32, 31]).where_label(), "router.ts:31-33");
    }

    #[test]
    fn a_note_with_no_anchor_is_about_the_file() {
        // Not a degenerate case any more: this is what `c` in the tree makes,
        // for asking about a file rather than about a line in it.
        let c = Comment {
            file: "src/server/router.ts".into(),
            anchors: Vec::new(),
            snippet: String::new(),
            body: String::new(),
            state: State::Queued,
        };
        assert!(c.is_file_note());
        assert_eq!(c.where_label(), "router.ts · the file");
        assert_eq!(c.path(), "src/server/router.ts");
    }
}
