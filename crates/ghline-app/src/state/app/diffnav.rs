//! Getting about a pull request's diff without leaving it.
//!
//! Reading a PR is reading it through, file after file, and every trip back to
//! the file list to pick the next one is a break in that. So the diff pane
//! moves on its own: by hunk, by file, and by search across every file.
//!
//! The diff pane scrolls rather than holding a cursor, so "where you are" is
//! the scroll offset, counted in the lines the pane draws — rows in unified
//! mode, pairs in split mode. Everything here works in rows and converts at
//! the last moment, which keeps the two modes from each needing their own copy
//! of every motion.

use std::collections::HashSet;

use super::App;
use super::files::{Entry, rows as tree_rows};
use super::input::strip_ws_only;
use super::prepared::{code, split_rows};
use unicode_width::UnicodeWidthStr;

use crate::data::{DiffKind, Hunk};

/// Where a search hit is: a file, by index, and a row of its unified diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub file: usize,
    pub row: usize,
}

/// Whether `query` is in `text`, ignoring case unless the query has a capital
/// in it — the rule vim calls smartcase.
pub fn matches(text: &str, query: &str) -> bool {
    !query.is_empty() && find(text, query).is_some()
}

/// Where `query` first occurs in `text`, by byte, with the same smartcase.
pub fn find(text: &str, query: &str) -> Option<std::ops::Range<usize>> {
    if query.chars().any(char::is_uppercase) {
        return text.find(query).map(|i| i..i + query.len());
    }
    // Lowercasing can change a string's length, so the offset is looked for
    // in a lowercased copy only when that copy is the same length.
    let (t, q) = (text.to_lowercase(), query.to_lowercase());
    if t.len() == text.len() {
        t.find(&q).map(|i| i..i + q.len())
    } else {
        text.find(query).map(|i| i..i + query.len())
    }
}

impl App {
    /// The files in the order the tree shows them, folded or not.
    fn file_order(&self) -> Vec<usize> {
        tree_rows(self.diff_files(), &HashSet::new())
            .into_iter()
            .filter_map(|r| match r.entry {
                Entry::File(i) => Some(i),
                Entry::Dir(_) => None,
            })
            .collect()
    }

    /// Shows file `idx` from its top, with the tree's cursor back on it.
    fn show_file(&mut self, idx: usize) {
        self.file_idx = idx;
        self.diff_dir = None;
        self.diff_scroll = 0;
        self.diff_pan = 0;
    }

    /// `H` / `L`: the code slides by a tab stop's worth either way. Never
    /// past the longest line, which is where the render stops it.
    pub(super) fn pan(&mut self, d: i64) {
        const STEP: usize = 8;
        self.diff_pan = if d < 0 {
            self.diff_pan.saturating_sub(STEP)
        } else {
            let widest = self
                .prepared()
                .rows
                .iter()
                .map(|r| code(r).width())
                .max()
                .unwrap_or(0);
            (self.diff_pan + STEP).min(widest.saturating_sub(STEP))
        };
    }

    /// `J` / `K`: the next or previous file, in the tree's order.
    pub(super) fn step_file(&mut self, d: i64) -> bool {
        let order = self.file_order();
        let at = order
            .iter()
            .position(|i| *i == self.file_idx())
            .unwrap_or(0) as i64;
        let to = at + d;
        match usize::try_from(to).ok().and_then(|t| order.get(t)) {
            Some(&next) => {
                self.show_file(next);
                true
            }
            None => false,
        }
    }

    /// The open file's line count in the mode on screen, and the line each
    /// hunk header is on.
    fn hunk_lines(&mut self) -> (usize, Vec<usize>) {
        let prep = self.prepared();
        if self.split {
            let pairs = split_rows(&prep.rows);
            let starts = pairs
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.header.map(|_| i))
                .collect();
            (pairs.len(), starts)
        } else {
            let starts = prep
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, r)| (r.kind == DiffKind::Hdr).then_some(i))
                .collect();
            (prep.rows.len(), starts)
        }
    }

    /// The furthest the pane can scroll, which is where the render clamps to.
    fn max_scroll(&self, len: usize) -> usize {
        len.saturating_sub(self.detail_height.max(1) as usize)
    }

    /// `}`: the next hunk at the top of the pane. Past the last one — or once
    /// the pane is at its end and every hunk left is already on screen — it
    /// carries on into the next file, so a PR reads straight through.
    pub(super) fn next_hunk(&mut self) {
        let (len, starts) = self.hunk_lines();
        let scroll = self.diff_scroll.min(self.max_scroll(len));
        match starts.iter().find(|s| **s > scroll) {
            Some(&s) if scroll < self.max_scroll(len) => self.diff_scroll = s,
            _ => {
                if !self.step_file(1) {
                    self.flash_ok("last file");
                }
            }
        }
    }

    /// `{`: the previous hunk; from the top of a file, the file before it.
    pub(super) fn prev_hunk(&mut self) {
        let (len, starts) = self.hunk_lines();
        let scroll = self.diff_scroll.min(self.max_scroll(len));
        match starts.iter().rev().find(|s| **s < scroll) {
            Some(&s) => self.diff_scroll = s,
            None if scroll > 0 => self.diff_scroll = 0,
            None => {
                if !self.step_file(-1) {
                    self.flash_ok("first file");
                }
            }
        }
    }

    /// A file's rows as the pane would number them, `w` applied.
    fn unified(&self, idx: usize) -> Vec<crate::data::DiffRow> {
        let Some(f) = self.diff_files().get(idx) else {
            return Vec::new();
        };
        if self.ws {
            Hunk::rows(&f.hunks.iter().map(strip_ws_only).collect::<Vec<_>>())
        } else {
            Hunk::rows(&f.hunks)
        }
    }

    /// The unified row the top of the pane is on.
    fn top_row(&mut self) -> usize {
        if !self.split {
            return self.diff_scroll;
        }
        let prep = self.prepared();
        let pairs = split_rows(&prep.rows);
        pairs
            .get(self.diff_scroll)
            .and_then(|p| [p.header, p.left, p.right].into_iter().flatten().min())
            .unwrap_or(0)
    }

    /// Scrolls so `row` of the open file sits a third of the way down: the
    /// lines above a hit are usually what explains it.
    fn reveal(&mut self, row: usize) {
        let line = if self.split {
            let prep = self.prepared();
            split_rows(&prep.rows)
                .iter()
                .position(|p| [p.header, p.left, p.right].contains(&Some(row)))
                .unwrap_or(0)
        } else {
            row
        };
        self.diff_scroll = line.saturating_sub(self.detail_height as usize / 3);
    }

    /// The next (or previous) line matching the search, across every file in
    /// the tree's order, wrapping at the end.
    ///
    /// `inclusive` lets the line the search starts on count, which is what
    /// typing wants — the hit should not jump away from under the letters —
    /// and `n` does not.
    pub(super) fn search(&mut self, forward: bool, inclusive: bool) {
        let query = self.diff_query.clone();
        if query.is_empty() {
            return;
        }
        let order = self.file_order();
        if order.is_empty() {
            return;
        }
        let here = match self.diff_hit {
            Some(h) if h.file == self.file_idx() => h.row,
            _ => self.top_row(),
        };
        let start = order
            .iter()
            .position(|i| *i == self.file_idx())
            .unwrap_or(0);

        // Every file once, beginning and ending with the open one so that its
        // own lines on the far side of `here` come round last.
        let n = order.len();
        for step in 0..=n {
            let k = if forward {
                (start + step) % n
            } else {
                (start + n - step % n) % n
            };
            let file = order[k];
            let rows = self.unified(file);
            let mut hit_rows = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind != DiffKind::Hdr && matches(code(r), &query))
                .map(|(i, _)| i);
            let found = match (step, forward) {
                (0, true) => hit_rows
                    .into_iter()
                    .find(|i| *i > here || (inclusive && *i == here)),
                (0, false) => hit_rows.rev().find(|i| *i < here),
                (s, true) if s == n => hit_rows.find(|i| *i <= here),
                (s, false) if s == n => hit_rows.rev().find(|i| *i >= here),
                (_, true) => hit_rows.next(),
                (_, false) => hit_rows.next_back(),
            };
            if let Some(row) = found {
                if file != self.file_idx() {
                    self.show_file(file);
                }
                self.diff_hit = Some(Hit { file, row });
                self.reveal(row);
                return;
            }
        }
        self.diff_hit = None;
        self.flash_warn(format!("no match for {query}"));
    }

    /// The search as typed: jumps to the first match from where you are.
    pub(super) fn search_as_typed(&mut self, text: String) {
        self.diff_query = text;
        if self.diff_query.is_empty() {
            self.diff_hit = None;
            return;
        }
        self.search(true, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lowercase_query_ignores_case_and_a_capital_does_not() {
        assert!(matches("let Total = 1", "total"));
        assert!(!matches("let total = 1", "Total"));
        assert!(matches("let Total = 1", "Total"));
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(!matches("anything", ""));
    }

    #[test]
    fn a_match_is_found_at_its_byte_offset() {
        assert_eq!(find("a Total b", "total"), Some(2..7));
        assert_eq!(find("漢 x", "x"), Some(4..5));
    }
}
