//! The open file's diff, worked out once rather than once a frame.
//!
//! Drawing a diff wants more than its rows: the syntax colour of each line and,
//! for a line that was edited rather than replaced, which words changed. Both
//! cost a pass over the whole file, and the rows themselves were already being
//! rebuilt — every string formatted again — on every frame. So all three are
//! prepared together and kept until something they depend on changes.
//!
//! What they depend on is in the key: the file, `w`, how many lines the file
//! has, and its review threads and whether and how wide they are shown. The
//! line count is what notices the diff arriving, since a file's list entry
//! exists before its hunks do, and the thread count notices the comments, both
//! without the loader having to know a cache is here at all.
//!
//! Review threads are laid in as rows by `notes`, after the colours and the
//! changed words are worked out on the file's own rows: a comment between a
//! deleted line and its added one must not stop the two being compared.

use std::ops::Range;
use std::rc::Rc;

use super::App;
use super::input::strip_ws_only;
use super::notes::{Notes, annotate};
use crate::data::{DiffKind, DiffRow, FileChange, Hunk, Thread};
use crate::shared::syntax::{self, Span};
use crate::tui::diff::{Pair, Side, pair};

/// A row's text is its sign, a space, then the code; spans and word ranges
/// are written against the code alone.
pub const SIGN: usize = 2;

/// The open file, ready to draw.
pub struct Prepared {
    pub rows: Vec<DiffRow>,
    /// Syntax runs for each row, against its code. Empty for a language the
    /// lexer does not know, and for headers.
    pub spans: Vec<Vec<Span>>,
    /// The part of an edited line that changed, against its code. `None` on a
    /// line that was not paired with another, or shares nothing with it.
    pub words: Vec<Option<Range<usize>>>,
}

/// What a `Prepared` was built from.
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    item: (String, i64),
    commit: Option<String>,
    path: String,
    ws: bool,
    lines: usize,
    threads: usize,
    notes: Notes,
}

impl App {
    /// The open file's diff, from the cache when nothing it depends on moved.
    pub fn prepared(&mut self) -> Rc<Prepared> {
        let Some(f) = self.diff_file() else {
            return Rc::new(prepare("", Vec::new()));
        };
        let threads = self.threads_on(&f.path);
        let notes = self.notes();
        let key = Key {
            item: (self.item_repo_key(), self.current().map_or(0, |c| c.num)),
            commit: self.diff_commit.clone(),
            path: f.path.clone(),
            ws: self.ws,
            lines: f.hunks.iter().map(|h| h.lines.len() + 1).sum(),
            threads: threads.len(),
            notes,
        };
        if let Some((k, p)) = &self.diff_cache
            && *k == key
        {
            return Rc::clone(p);
        }
        let base = base_rows(f, self.ws);
        let p = Rc::new(prepare_with(&f.path, base, &threads, notes));
        self.diff_cache = Some((key, Rc::clone(&p)));
        p
    }

    /// The review threads on `path`. None while one commit is shown: a
    /// thread's line is a line of the pull request's last version, and in an
    /// earlier commit it would sit under whatever happens to have that number.
    pub fn threads_on(&self, path: &str) -> Vec<&Thread> {
        if self.diff_commit.is_some() {
            return Vec::new();
        }
        self.current()
            .and_then(|c| c.as_pr())
            .map(|p| p.threads.iter().filter(|t| t.path == path).collect())
            .unwrap_or_default()
    }

    /// How threads are shown right now. The text runs to the pane's width
    /// less the bar it sits beside and a column of margin either side.
    pub fn notes(&self) -> Notes {
        Notes {
            shown: self.diff_notes,
            width: self.diff_width.saturating_sub(6),
        }
    }
}

/// A file's own rows, `w` applied.
pub fn base_rows(f: &FileChange, ws: bool) -> Vec<DiffRow> {
    if ws {
        Hunk::rows(&f.hunks.iter().map(strip_ws_only).collect::<Vec<_>>())
    } else {
        Hunk::rows(&f.hunks)
    }
}

/// A file's rows as the pane draws them, its threads laid in.
pub fn file_rows(f: &FileChange, ws: bool, threads: &[&Thread], notes: Notes) -> Vec<DiffRow> {
    annotate(base_rows(f, ws), threads, notes).0
}

/// Colours and pairs a file's own rows, then lays its threads in.
pub fn prepare_with(path: &str, base: Vec<DiffRow>, threads: &[&Thread], notes: Notes) -> Prepared {
    let plain = prepare(path, base.clone());
    let (rows, map) = annotate(base, threads, notes);
    let spans = map
        .iter()
        .map(|m| {
            m.and_then(|i| plain.spans.get(i).cloned())
                .unwrap_or_default()
        })
        .collect();
    let words = map
        .iter()
        .map(|m| m.and_then(|i| plain.words.get(i).cloned().flatten()))
        .collect();
    Prepared { rows, spans, words }
}

/// The code part of a row: everything after its sign. A comment's row is all
/// text and has no sign, and the search reads it like code.
pub fn code(r: &DiffRow) -> &str {
    match r.kind {
        DiffKind::Hdr => "",
        DiffKind::NoteHead | DiffKind::Note => &r.text,
        _ => r.text.get(SIGN..).unwrap_or(""),
    }
}

/// Colours and pairs a file's rows.
pub fn prepare(path: &str, rows: Vec<DiffRow>) -> Prepared {
    // The lexer reads whole files and a diff is not one: a hunk starts
    // mid-file, so a comment opened above it is invisible here. Feeding it the
    // rows in order is the closest thing available, and right for everything
    // a hunk contains rather than inherits — the same bargain diffline makes.
    let spans = match syntax::of_path(path) {
        Some(lang) => {
            let joined = rows.iter().map(code).collect::<Vec<_>>().join("\n");
            let mut s = syntax::highlight(lang, &joined);
            // `lines()` drops a trailing empty line `join` did not make
            s.resize(rows.len(), Vec::new());
            s
        }
        None => vec![Vec::new(); rows.len()],
    };
    let words = words(&rows);
    Prepared { rows, spans, words }
}

/// Which words changed on each edited line.
///
/// A run of deletions followed straight away by a run of additions is an
/// edit, and its lines are paired off in order — the first deleted with the
/// first added, and so on — the way a reader lines them up. The rest of a
/// longer run was replaced outright and has nothing to compare with.
fn words(rows: &[DiffRow]) -> Vec<Option<Range<usize>>> {
    let mut out = vec![None; rows.len()];
    let mut i = 0;
    while i < rows.len() {
        let dels = run(rows, i, DiffKind::Del);
        let adds = run(rows, i + dels, DiffKind::Add);
        if dels == 0 || adds == 0 {
            i += dels.max(1);
            continue;
        }
        for k in 0..dels.min(adds) {
            let (d, a) = (i + k, i + dels + k);
            if let Some((old, new)) = changed(code(&rows[d]), code(&rows[a])) {
                out[d] = Some(old);
                out[a] = Some(new);
            }
        }
        i += dels + adds;
    }
    out
}

fn run(rows: &[DiffRow], from: usize, kind: DiffKind) -> usize {
    rows.get(from..)
        .map_or(0, |r| r.iter().take_while(|r| r.kind == kind).count())
}

/// The span of each line that differs from the other, widened to whole words.
///
/// `None` when the mark would cover all of both lines: that says no more than
/// their colour already does. Also `None` when they are the same, which `w`
/// can leave behind.
fn changed(old: &str, new: &str) -> Option<(Range<usize>, Range<usize>)> {
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

/// Folds the rows into side-by-side lines.
///
/// The fold itself is `tui::diff`, shared with the other program — it was
/// written out here and there, the same algorithm twice. This is only the
/// seam: what our kinds are called on the way in.
pub fn split_rows(rows: &[DiffRow]) -> Vec<Pair> {
    let sides: Vec<Side> = rows
        .iter()
        .map(|r| match r.kind {
            // a comment spans both halves, as a header does
            DiffKind::Hdr | DiffKind::NoteHead | DiffKind::Note => Side::Header,
            DiffKind::Ctx => Side::Context,
            DiffKind::Del => Side::Deleted,
            DiffKind::Add => Side::Added,
        })
        .collect();
    pair(&sides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: DiffKind, code: &str) -> DiffRow {
        let sign = match kind {
            DiffKind::Add => '+',
            DiffKind::Del => '-',
            _ => ' ',
        };
        DiffRow {
            kind,
            text: format!("{sign} {code}"),
            lo: String::new(),
            ln: String::new(),
        }
    }

    fn marked(p: &Prepared, i: usize) -> Option<&str> {
        p.words[i].clone().map(|r| &code(&p.rows[i])[r])
    }

    #[test]
    fn an_edited_line_marks_the_word_that_changed_on_both_sides() {
        let p = prepare(
            "x.rs",
            vec![
                row(DiffKind::Del, "let side = total / 4;"),
                row(DiffKind::Add, "let side = width / 4;"),
            ],
        );
        assert_eq!(marked(&p, 0), Some("total"));
        assert_eq!(marked(&p, 1), Some("width"));
    }

    #[test]
    fn lines_with_nothing_in_common_are_not_marked() {
        let p = prepare(
            "x.rs",
            vec![row(DiffKind::Del, "alpha"), row(DiffKind::Add, "omega")],
        );
        assert_eq!(p.words, [None, None]);
    }

    #[test]
    fn the_extra_lines_of_a_longer_run_have_nothing_to_compare_with() {
        let p = prepare(
            "x.rs",
            vec![
                row(DiffKind::Del, "go 1.25.0"),
                row(DiffKind::Add, "go 1.27"),
                row(DiffKind::Add, "toolchain go1.27.2"),
            ],
        );
        assert!(p.words[0].is_some() && p.words[1].is_some());
        assert_eq!(p.words[2], None);
    }

    #[test]
    fn an_insertion_inside_a_line_marks_only_the_new_side() {
        let p = prepare(
            "x.rs",
            vec![
                row(DiffKind::Del, "foo(a)"),
                row(DiffKind::Add, "foo(a, b)"),
            ],
        );
        assert_eq!(marked(&p, 0), Some(""));
        assert_eq!(marked(&p, 1), Some(", b"));
    }

    #[test]
    fn a_change_past_multibyte_text_does_not_cut_a_character() {
        let p = prepare(
            "x.rs",
            vec![
                row(DiffKind::Del, "\"漢字\" = 1"),
                row(DiffKind::Add, "\"漢字\" = 2"),
            ],
        );
        assert_eq!(marked(&p, 0), Some("1"));
        assert_eq!(marked(&p, 1), Some("2"));
    }

    #[test]
    fn a_known_language_is_coloured_and_an_unknown_one_is_not() {
        let rows = || vec![row(DiffKind::Ctx, "fn main() {}")];
        assert!(!prepare("main.rs", rows()).spans[0].is_empty());
        assert!(prepare("notes.unknown", rows()).spans[0].is_empty());
    }
}
