//! A pull request's changed files as a folder tree that folds.
//!
//! The tree and its folding are `line_shared::tree`, which diffline's file
//! list uses too; this is ghline's side of it — its files as the tree's
//! leaves, and what a choice in the tree does to the diff beside it.

use super::{App, Pane};
use crate::data::FileChange;
use crate::shared::tree::{self, Leaf};
pub use crate::shared::tree::{Entry, Row};

/// `"+12"` → 12. The counts are kept as the text `gh` printed, and one that
/// does not read is a file with nothing to count rather than an error.
fn count(s: &str) -> u32 {
    s.trim_start_matches(['+', '-']).parse().unwrap_or(0)
}

/// A pull request's files as the tree's leaves.
pub fn leaves(files: &[FileChange]) -> Vec<Leaf> {
    files
        .iter()
        .map(|f| Leaf {
            path: f.path.clone(),
            add: count(&f.add),
            del: count(&f.del),
        })
        .collect()
}

impl App {
    fn leaves(&self) -> Vec<Leaf> {
        leaves(self.diff_files())
    }

    /// The changed files as the tree pane draws them.
    pub fn file_rows(&self) -> Vec<Row> {
        self.diff_folds.rows(&self.leaves())
    }

    /// The row the cursor is on. See `Folds::cursor`.
    pub fn file_row(&self, rows: &[Row]) -> usize {
        self.diff_folds
            .cursor(rows, &self.leaves(), self.file_idx())
    }

    /// Where the selected file falls among all of them in tree order, for the
    /// `n/total` counter — the order the reader sees, not the order `gh` sent.
    pub fn file_ordinal(&self) -> usize {
        tree::ordinal(&self.leaves(), self.file_idx())
    }

    /// The first file in tree order, which is where a diff opens.
    pub(super) fn first_file(&self) -> usize {
        tree::first(&self.leaves())
    }

    /// Shows file `f` from its top, when it is not the one already shown.
    fn show(&mut self, f: Option<usize>) {
        if let Some(f) = f
            && f != self.file_idx
        {
            self.file_idx = f;
            self.diff_scroll = 0;
            self.diff_cursor = 0;
            self.diff_pan = 0;
        }
    }

    /// Puts the cursor on row `i`. A file becomes the one the diff shows; a
    /// folder leaves the diff on whatever it was showing.
    pub(super) fn select_file_row(&mut self, i: usize) {
        let files = self.leaves();
        let f = self.diff_folds.select(&files, i);
        self.show(f);
    }

    pub(super) fn move_file_row(&mut self, d: i64) {
        let files = self.leaves();
        let at = self.file_idx();
        let f = self.diff_folds.step(&files, at, d);
        self.show(f);
    }

    /// `enter` on the tree: a folder folds or opens, a file opens its diff.
    pub(super) fn enter_file_row(&mut self) {
        let files = self.leaves();
        let at = self.file_idx();
        if !self.diff_folds.enter(&files, at) {
            self.pane = Pane::DiffBody;
        }
    }

    /// `o`: folds or opens the folder the cursor is in.
    pub(super) fn toggle_cursor_folder(&mut self) {
        let files = self.leaves();
        let at = self.file_idx();
        self.diff_folds.toggle_here(&files, at);
    }

    /// `h` on the tree. `false` at the top level, where `h` keeps its
    /// ordinary meaning.
    pub(super) fn fold_left(&mut self) -> bool {
        let files = self.leaves();
        let at = self.file_idx();
        self.diff_folds.fold_left(&files, at)
    }

    /// `l` on the tree. `false` unless a folded folder is under the cursor,
    /// where `l` keeps its ordinary meaning of moving into the diff.
    pub(super) fn unfold_right(&mut self) -> bool {
        let files = self.leaves();
        let at = self.file_idx();
        self.diff_folds.unfold_right(&files, at)
    }

    /// `O`: everything open if anything is folded, everything folded if not.
    pub(super) fn toggle_all_folders(&mut self) {
        let files = self.leaves();
        let at = self.file_idx();
        self.diff_folds.toggle_all(&files, at);
    }
}
