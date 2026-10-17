//! Review threads, laid into a file's diff under the lines they are about.
//!
//! A comment read apart from its line is a comment read twice: once to learn
//! what it says and again, back in the diff, to find what it says it about.
//! So a thread is drawn as rows of the diff itself, straight under the line it
//! is anchored to, and everything that walks the rows — the scroll, the hunk
//! jumps, the search — walks the comments with them.
//!
//! What becomes a row:
//!
//! - an open thread: a head row per comment (who, and when) and its text,
//!   folded to the pane's width;
//! - a resolved one: a single row, since it is settled and mostly in the way;
//! - one whose line no longer exists — GitHub's "outdated", or a line outside
//!   the hunks shown — goes to the top of the file, marked, rather than being
//!   dropped: it was still said about this file.

use crate::data::{DiffKind, DiffRow, DiffSide, Thread};

/// How threads are shown: whether at all, and how wide their text may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notes {
    pub shown: bool,
    pub width: usize,
}

/// What a head row starts with, so the view can style it without the rows
/// carrying a second kind of their own.
pub const RESOLVED: &str = "✓ ";
pub const OUTDATED: &str = "outdated · ";

/// The rows with `threads` laid in, and for each row the index of the file's
/// own row it is — `None` for a comment's row.
pub fn annotate(
    base: Vec<DiffRow>,
    threads: &[&Thread],
    notes: Notes,
) -> (Vec<DiffRow>, Vec<Option<usize>>) {
    if !notes.shown || threads.is_empty() {
        let map = (0..base.len()).map(Some).collect();
        return (base, map);
    }
    // where each thread goes: after a row, or at the top
    let mut after: Vec<Vec<&Thread>> = vec![Vec::new(); base.len()];
    let mut top: Vec<&Thread> = Vec::new();
    for t in threads {
        match t.line.and_then(|l| anchor(&base, l, t.side)) {
            Some(i) => after[i].push(t),
            None => top.push(t),
        }
    }

    let mut rows = Vec::with_capacity(base.len());
    let mut map = Vec::with_capacity(base.len());
    for t in &top {
        for r in thread_rows(t, true, notes.width) {
            rows.push(r);
            map.push(None);
        }
    }
    for (i, row) in base.into_iter().enumerate() {
        rows.push(row);
        map.push(Some(i));
        for t in after.get(i).into_iter().flatten() {
            for r in thread_rows(t, false, notes.width) {
                rows.push(r);
                map.push(None);
            }
        }
    }
    (rows, map)
}

/// The row a comment on `line` of `side` sits under. A comment on the new
/// file is on an added or unchanged line; one on the old, a deleted or
/// unchanged one.
fn anchor(rows: &[DiffRow], line: u32, side: DiffSide) -> Option<usize> {
    let want = line.to_string();
    rows.iter().position(|r| match side {
        DiffSide::Right => matches!(r.kind, DiffKind::Add | DiffKind::Ctx) && r.ln == want,
        DiffSide::Left => matches!(r.kind, DiffKind::Del | DiffKind::Ctx) && r.lo == want,
    })
}

fn note(kind: DiffKind, text: String) -> DiffRow {
    DiffRow {
        kind,
        text,
        lo: String::new(),
        ln: String::new(),
    }
}

/// One thread as rows.
fn thread_rows(t: &Thread, stale: bool, width: usize) -> Vec<DiffRow> {
    let lead = if stale { OUTDATED } else { "" };
    if t.resolved {
        let (who, first) = t.comments.first().map_or(("", ""), |c| {
            (c.author.as_str(), c.body.lines().next().unwrap_or(""))
        });
        return vec![note(
            DiffKind::NoteHead,
            format!("{RESOLVED}{lead}{who}: {first}"),
        )];
    }
    let mut out = Vec::new();
    for (i, c) in t.comments.iter().enumerate() {
        // the first comment is the thread; the rest are replies to it
        let lead = if i == 0 { lead } else { "↳ " };
        out.push(note(
            DiffKind::NoteHead,
            format!("{lead}{} · {}", c.author, c.when),
        ));
        for line in body_lines(&c.body, width) {
            out.push(note(DiffKind::Note, line));
        }
    }
    out
}

/// A comment's text folded to `width`, with runs of blank lines kept to one
/// so a long review does not cost its paragraphs twice.
fn body_lines(body: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for para in body.trim().lines() {
        let para = para.trim_end();
        if para.is_empty() {
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        out.extend(crate::tui::wrap(para, width.max(20)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ThreadComment;

    fn row(kind: DiffKind, lo: &str, ln: &str) -> DiffRow {
        DiffRow {
            kind,
            text: "  x".into(),
            lo: lo.into(),
            ln: ln.into(),
        }
    }

    /// `@@`, context 10, deleted 11, added 11, context 12/12.
    fn base() -> Vec<DiffRow> {
        vec![
            row(DiffKind::Hdr, "", ""),
            row(DiffKind::Ctx, "10", "10"),
            row(DiffKind::Del, "11", ""),
            row(DiffKind::Add, "", "11"),
            row(DiffKind::Ctx, "12", "12"),
        ]
    }

    fn thread(line: Option<u32>, side: DiffSide, resolved: bool) -> Thread {
        Thread {
            path: "a.rs".into(),
            line,
            side,
            resolved,
            comments: vec![
                ThreadComment {
                    author: "ada".into(),
                    body: "why not a constant?".into(),
                    when: "2h ago".into(),
                },
                ThreadComment {
                    author: "kai".into(),
                    body: "fair, done".into(),
                    when: "1h ago".into(),
                },
            ],
        }
    }

    const SHOWN: Notes = Notes {
        shown: true,
        width: 60,
    };

    fn kinds(rows: &[DiffRow]) -> Vec<DiffKind> {
        rows.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn a_comment_on_the_new_file_sits_under_its_added_line() {
        let t = thread(Some(11), DiffSide::Right, false);
        let (rows, map) = annotate(base(), &[&t], SHOWN);
        use DiffKind::*;
        assert_eq!(
            kinds(&rows),
            [Hdr, Ctx, Del, Add, NoteHead, Note, NoteHead, Note, Ctx]
        );
        assert_eq!(rows[4].text, "ada · 2h ago");
        assert_eq!(rows[6].text, "↳ kai · 1h ago", "a reply says it is one");
        assert_eq!(map[4], None);
        assert_eq!(map[8], Some(4), "the file's own rows keep their index");
    }

    #[test]
    fn a_comment_on_the_old_file_sits_under_its_deleted_line() {
        let t = thread(Some(11), DiffSide::Left, false);
        let (rows, _) = annotate(base(), &[&t], SHOWN);
        assert_eq!(rows[3].kind, DiffKind::NoteHead);
    }

    #[test]
    fn a_resolved_thread_is_one_row() {
        let t = thread(Some(12), DiffSide::Right, true);
        let (rows, _) = annotate(base(), &[&t], SHOWN);
        assert_eq!(rows.len(), base().len() + 1);
        assert_eq!(rows[5].text, "✓ ada: why not a constant?");
    }

    #[test]
    fn an_outdated_thread_goes_to_the_top_marked_rather_than_lost() {
        let gone = thread(None, DiffSide::Right, false);
        let elsewhere = thread(Some(400), DiffSide::Right, false);
        let (rows, _) = annotate(base(), &[&gone, &elsewhere], SHOWN);
        assert!(rows[0].text.starts_with(OUTDATED));
        assert_eq!(
            rows.iter().filter(|r| r.text.starts_with(OUTDATED)).count(),
            2
        );
    }

    #[test]
    fn hidden_comments_leave_the_rows_as_they_were() {
        let t = thread(Some(11), DiffSide::Right, false);
        let notes = Notes {
            shown: false,
            width: 60,
        };
        let (rows, map) = annotate(base(), &[&t], notes);
        assert_eq!(rows.len(), base().len());
        assert!(map.iter().enumerate().all(|(i, m)| *m == Some(i)));
    }

    #[test]
    fn a_long_comment_folds_and_keeps_one_blank_between_paragraphs() {
        let lines = body_lines("first paragraph\n\n\n\nsecond one", 40);
        assert_eq!(lines, ["first paragraph", "", "second one"]);
    }
}
