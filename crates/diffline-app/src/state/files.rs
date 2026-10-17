//! The changed files as a folder tree that folds.
//!
//! The tree and its folding are `line_shared::tree`, which ghline's file list
//! uses too; this is diffline's side of it — its files as the tree's leaves,
//! and what a choice in the tree does to the diff beside it. The file the
//! diff shows stays `file_idx`; the tree only says where the cursor is when
//! that is a folder rather than a file.

use crate::app::{App, Pane};
use crate::shared::tree::{self, Leaf, Row};

impl App {
    /// The scope's files as the tree's leaves.
    pub fn leaves(&self) -> Vec<Leaf> {
        self.files
            .iter()
            .map(|f| Leaf {
                path: f.path.clone(),
                add: f.add,
                del: f.del,
            })
            .collect()
    }

    /// The tree as the pane draws it.
    pub fn tree_rows(&self) -> Vec<Row> {
        self.tree_folds.rows(&self.leaves())
    }

    /// The row the cursor is on.
    pub fn tree_row(&self, rows: &[Row]) -> usize {
        self.tree_folds.cursor(rows, &self.leaves(), self.file_idx)
    }

    /// The first file in the tree's order, where a scope opens.
    pub fn first_file(&self) -> usize {
        tree::first(&self.leaves())
    }

    /// Opens `f` when it is a file and not the one already open.
    fn choose(&mut self, f: Option<usize>) {
        if let Some(f) = f
            && f != self.file_idx
        {
            self.goto_file(f);
        }
    }

    /// Puts the cursor on row `i` — a click, or `j`/`k` arriving there.
    pub fn tree_select(&mut self, i: usize) -> bool {
        let files = self.leaves();
        let f = self.tree_folds.select(&files, i);
        let file = f.is_some();
        self.choose(f);
        file
    }

    /// `j`/`k` on the tree.
    pub fn tree_step(&mut self, d: i64) {
        let files = self.leaves();
        let f = self.tree_folds.step(&files, self.file_idx, d);
        self.choose(f);
    }

    /// `enter` on the tree: a folder folds or opens, a file opens its diff.
    pub fn tree_enter(&mut self) {
        let files = self.leaves();
        if !self.tree_folds.enter(&files, self.file_idx) {
            self.pane = Pane::Diff;
        }
    }

    /// `o` on the tree: folds or opens the folder the cursor is in.
    pub fn fold_here(&mut self) {
        let files = self.leaves();
        self.tree_folds.toggle_here(&files, self.file_idx);
    }

    /// `h` on the tree. `false` at the top level, where `h` keeps moving
    /// between panes.
    pub fn fold_left(&mut self) -> bool {
        let files = self.leaves();
        self.tree_folds.fold_left(&files, self.file_idx)
    }

    /// `l` on the tree. `false` unless a folded folder is under the cursor.
    pub fn unfold_right(&mut self) -> bool {
        let files = self.leaves();
        self.tree_folds.unfold_right(&files, self.file_idx)
    }

    /// `O`: everything open if anything is folded, everything folded if not.
    pub fn fold_all(&mut self) {
        let files = self.leaves();
        self.tree_folds.toggle_all(&files, self.file_idx);
    }

    /// The file `d` places from the open one in the tree's order, round the
    /// ends — which is the order the reader sees them in.
    pub fn file_after(&self, d: i64) -> Option<usize> {
        let order = tree::order(&self.leaves());
        let n = order.len() as i64;
        if n == 0 {
            return None;
        }
        let at = order.iter().position(|i| *i == self.file_idx).unwrap_or(0) as i64;
        order.get((at + d).rem_euclid(n) as usize).copied()
    }
}
