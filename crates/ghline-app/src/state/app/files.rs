//! A pull request's changed files as a folder tree that folds.
//!
//! The list arrives flat, one path per file, and a PR of any size is mostly
//! read by where it lands: the backend or the frontend, the tests or the code.
//! A tree says that at a glance, and folding a folder takes a whole area out of
//! the way once it has been read.
//!
//! The tree is rebuilt from the paths whenever it is asked for rather than kept
//! beside them. The file list is replaced wholesale when the diff arrives, and a
//! second copy would be a second thing to keep in step with it; a few thousand
//! paths cost nothing next to drawing the diff beside them.
//!
//! Two pieces of state are all it needs: which folders are folded, and whether
//! the cursor is on a folder. When it is not, it is on `file_idx`, the file the
//! diff pane shows — so the diff keeps the last file opened while the cursor
//! moves over folders, which is what you are still reading.

use std::collections::{BTreeMap, HashSet};

use super::{App, Pane};
use crate::data::FileChange;

/// What a row of the tree stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A folder, by its full path without a trailing slash.
    Dir(String),
    /// A file, by its index into the PR's file list.
    File(usize),
}

/// One row of the tree, as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub entry: Entry,
    /// How many folders up it sits under.
    pub depth: usize,
    /// A file's name, or a folder's. Folders that hold nothing but the next
    /// folder are joined into one row with `/`, as GitHub does, so a Java
    /// package does not cost a row per segment.
    pub label: String,
    /// Lines added and deleted, summed over everything under a folder.
    pub add: u32,
    pub del: u32,
    /// Files under a folder; 1 for a file.
    pub files: usize,
    /// Whether a folder is folded. Always false for a file.
    pub folded: bool,
}

#[derive(Default)]
struct Node {
    dirs: BTreeMap<String, Self>,
    files: Vec<(String, usize)>,
}

impl Node {
    fn totals(&self, all: &[FileChange]) -> (u32, u32, usize) {
        let mut t = (0, 0, self.files.len());
        for (_, i) in &self.files {
            if let Some(f) = all.get(*i) {
                t.0 += count(&f.add);
                t.1 += count(&f.del);
            }
        }
        for d in self.dirs.values() {
            let (a, r, n) = d.totals(all);
            t = (t.0 + a, t.1 + r, t.2 + n);
        }
        t
    }
}

/// `"+12"` → 12. The counts are kept as the text `gh` printed, and one that
/// does not read is a file with nothing to count rather than an error.
fn count(s: &str) -> u32 {
    s.trim_start_matches(['+', '-']).parse().unwrap_or(0)
}

/// The rows of the tree, with the folders in `folded` closed.
///
/// Folders come before files at every level, each alphabetically, which is
/// how GitHub lays out the same tree.
pub fn rows(files: &[FileChange], folded: &HashSet<String>) -> Vec<Row> {
    let mut root = Node::default();
    for (i, f) in files.iter().enumerate() {
        let mut node = &mut root;
        let mut parts: Vec<&str> = f.path.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        for p in parts {
            node = node.dirs.entry(p.to_string()).or_default();
        }
        node.files.push((name.to_string(), i));
    }
    let mut out = Vec::new();
    walk(&root, "", 0, files, folded, &mut out);
    out
}

fn walk(
    node: &Node,
    prefix: &str,
    depth: usize,
    all: &[FileChange],
    folded: &HashSet<String>,
    out: &mut Vec<Row>,
) {
    for (name, child) in &node.dirs {
        let mut label = name.clone();
        let mut dir = child;
        // a folder holding only another folder is shown as one row
        while dir.files.is_empty() && dir.dirs.len() == 1 {
            let Some((next, inner)) = dir.dirs.iter().next() else {
                break;
            };
            label = format!("{label}/{next}");
            dir = inner;
        }
        let path = format!("{prefix}{label}");
        let (add, del, n) = dir.totals(all);
        let closed = folded.contains(&path);
        out.push(Row {
            entry: Entry::Dir(path.clone()),
            depth,
            label,
            add,
            del,
            files: n,
            folded: closed,
        });
        if !closed {
            walk(dir, &format!("{path}/"), depth + 1, all, folded, out);
        }
    }
    let mut files = node.files.clone();
    files.sort();
    for (name, i) in files {
        let (add, del) = all
            .get(i)
            .map_or((0, 0), |f| (count(&f.add), count(&f.del)));
        out.push(Row {
            entry: Entry::File(i),
            depth,
            label: name,
            add,
            del,
            files: 1,
            folded: false,
        });
    }
}

impl App {
    /// The changed files as the tree pane draws them.
    pub fn file_rows(&self) -> Vec<Row> {
        rows(self.diff_files(), &self.diff_folded)
    }

    /// The row the cursor is on.
    ///
    /// On a folder when one was chosen, and otherwise on the file the diff is
    /// showing. When that file is inside a folded folder the cursor rests on
    /// the innermost row still standing over it — the folded one — so folding
    /// never strands it.
    pub fn file_row(&self, rows: &[Row]) -> usize {
        if let Some(dir) = &self.diff_dir
            && let Some(i) = rows.iter().position(|r| r.entry == Entry::Dir(dir.clone()))
        {
            return i;
        }
        let idx = self.file_idx();
        if let Some(i) = rows.iter().position(|r| r.entry == Entry::File(idx)) {
            return i;
        }
        let path = self.diff_files().get(idx).map_or("", |f| f.path.as_str());
        // ancestors come in order from the top down, so the last is innermost
        rows.iter()
            .rposition(|r| match &r.entry {
                Entry::Dir(d) => path.starts_with(&format!("{d}/")),
                Entry::File(_) => false,
            })
            .unwrap_or(0)
    }

    /// Where the selected file falls among all of them in tree order, for the
    /// `n/total` counter — the order the reader sees, not the order `gh` sent.
    pub fn file_ordinal(&self) -> usize {
        let idx = self.file_idx();
        rows(self.diff_files(), &HashSet::new())
            .iter()
            .filter(|r| matches!(r.entry, Entry::File(_)))
            .position(|r| r.entry == Entry::File(idx))
            .unwrap_or(0)
    }

    /// The first file in tree order, which is where a diff opens.
    pub(super) fn first_file(&self) -> usize {
        rows(self.diff_files(), &HashSet::new())
            .iter()
            .find_map(|r| match r.entry {
                Entry::File(i) => Some(i),
                Entry::Dir(_) => None,
            })
            .unwrap_or(0)
    }

    /// Puts the cursor on row `i`. A file becomes the one the diff shows; a
    /// folder leaves the diff on whatever it was showing.
    pub(super) fn select_file_row(&mut self, i: usize) {
        let rows = self.file_rows();
        match rows.get(i).map(|r| r.entry.clone()) {
            Some(Entry::Dir(d)) => self.diff_dir = Some(d),
            Some(Entry::File(f)) => {
                self.diff_dir = None;
                if f != self.file_idx {
                    self.file_idx = f;
                    self.diff_scroll = 0;
                    self.diff_pan = 0;
                }
            }
            None => {}
        }
    }

    pub(super) fn move_file_row(&mut self, d: i64) {
        let rows = self.file_rows();
        if rows.is_empty() {
            return;
        }
        let at = self.file_row(&rows) as i64;
        let to = (at + d).clamp(0, rows.len() as i64 - 1) as usize;
        self.select_file_row(to);
    }

    /// The folder under the cursor, if the cursor is on one.
    fn cursor_dir(&self) -> Option<Row> {
        let rows = self.file_rows();
        rows.get(self.file_row(&rows))
            .filter(|r| matches!(r.entry, Entry::Dir(_)))
            .cloned()
    }

    /// The folder the cursor is in: the one under it, or a file's own.
    fn cursor_folder(&self) -> Option<String> {
        if let Some(Row {
            entry: Entry::Dir(d),
            ..
        }) = self.cursor_dir()
        {
            return Some(d);
        }
        let rows = self.file_rows();
        let path = &self.diff_files().get(self.file_idx())?.path;
        // The nearest folder row above the file, which may be a joined one —
        // `src/layout` rather than `src/layout`'s own parent `src`.
        rows.iter()
            .filter_map(|r| match &r.entry {
                Entry::Dir(d) if path.starts_with(&format!("{d}/")) => Some(d.clone()),
                _ => None,
            })
            .max_by_key(String::len)
    }

    /// `enter` on the tree: a folder folds or opens, a file opens its diff.
    pub(super) fn enter_file_row(&mut self) {
        match self.cursor_dir() {
            Some(Row {
                entry: Entry::Dir(d),
                ..
            }) => self.toggle_folder(d),
            _ => self.pane = Pane::DiffBody,
        }
    }

    /// `o`: folds or opens the folder the cursor is in.
    pub(super) fn toggle_cursor_folder(&mut self) {
        if let Some(d) = self.cursor_folder() {
            self.toggle_folder(d);
        }
    }

    fn toggle_folder(&mut self, d: String) {
        if !self.diff_folded.remove(&d) {
            self.diff_folded.insert(d.clone());
            // folding the folder a file is in moves the cursor up to it
            self.diff_dir = Some(d);
        }
    }

    /// `h` on the tree. Folds the open folder under the cursor; on a file or a
    /// folded folder, folds the one it sits in. `false` at the top level, where
    /// there is nothing to fold and `h` keeps its ordinary meaning.
    pub(super) fn fold_left(&mut self) -> bool {
        if let Some(Row {
            entry: Entry::Dir(d),
            folded: false,
            ..
        }) = self.cursor_dir()
        {
            self.diff_folded.insert(d.clone());
            self.diff_dir = Some(d);
            return true;
        }
        let up = match self.cursor_dir() {
            Some(Row {
                entry: Entry::Dir(d),
                ..
            }) => {
                // the folder over this one, as the tree draws it
                let rows = self.file_rows();
                rows.iter()
                    .filter_map(|r| match &r.entry {
                        Entry::Dir(p) if d.starts_with(&format!("{p}/")) => Some(p.clone()),
                        _ => None,
                    })
                    .max_by_key(String::len)
            }
            _ => self.cursor_folder(),
        };
        match up {
            Some(p) => {
                self.diff_folded.insert(p.clone());
                self.diff_dir = Some(p);
                true
            }
            None => false,
        }
    }

    /// `l` on the tree. Opens a folded folder; `false` otherwise, where `l`
    /// keeps its ordinary meaning of moving into the diff.
    pub(super) fn unfold_right(&mut self) -> bool {
        match self.cursor_dir() {
            Some(Row {
                entry: Entry::Dir(d),
                folded: true,
                ..
            }) => {
                self.diff_folded.remove(&d);
                true
            }
            _ => false,
        }
    }

    /// `O`: everything open if anything is folded, everything folded if not.
    pub(super) fn toggle_all_folders(&mut self) {
        if !self.diff_folded.is_empty() {
            self.diff_folded.clear();
            return;
        }
        let all: HashSet<String> = rows(self.diff_files(), &HashSet::new())
            .into_iter()
            .filter_map(|r| match r.entry {
                Entry::Dir(d) => Some(d),
                Entry::File(_) => None,
            })
            .collect();
        // the cursor goes to the top-level folder its file was in
        let top = self.diff_files().get(self.file_idx()).and_then(|f| {
            let rows = rows(self.diff_files(), &all);
            rows.into_iter().find_map(|r| match r.entry {
                Entry::Dir(d) if f.path.starts_with(&format!("{d}/")) => Some(d),
                _ => None,
            })
        });
        self.diff_folded = all;
        self.diff_dir = top;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, add: u32, del: u32) -> FileChange {
        FileChange {
            path: path.into(),
            add: format!("+{add}"),
            del: format!("-{del}"),
            hunks: Vec::new(),
            ..FileChange::default()
        }
    }

    fn labels(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.label))
            .collect()
    }

    #[test]
    fn folders_come_before_files_at_every_level() {
        let files = [
            file("README.md", 1, 0),
            file("src/main.rs", 2, 0),
            file("src/app/state.rs", 3, 0),
            file("Cargo.toml", 1, 1),
        ];
        let out = rows(&files, &HashSet::new());
        assert_eq!(
            labels(&out),
            [
                "src",
                "  app",
                "    state.rs",
                "  main.rs",
                "Cargo.toml",
                "README.md"
            ]
        );
    }

    #[test]
    fn a_folder_that_holds_only_a_folder_is_one_row() {
        let files = [
            file("BackEnd/internal/platform/oauth.go", 1, 0),
            file("BackEnd/internal/platform/server.go", 1, 0),
            file("BackEnd/go.mod", 1, 0),
        ];
        let out = rows(&files, &HashSet::new());
        assert_eq!(
            labels(&out),
            [
                "BackEnd",
                "  internal/platform",
                "    oauth.go",
                "    server.go",
                "  go.mod"
            ]
        );
        assert_eq!(out[1].entry, Entry::Dir("BackEnd/internal/platform".into()));
    }

    #[test]
    fn a_folded_folder_hides_what_is_under_it_and_sums_it() {
        let files = [
            file("src/a.rs", 10, 2),
            file("src/deep/b.rs", 5, 1),
            file("x.rs", 1, 0),
        ];
        let folded: HashSet<String> = ["src".to_string()].into();
        let out = rows(&files, &folded);
        assert_eq!(labels(&out), ["src", "x.rs"]);
        assert!(out[0].folded);
        assert_eq!((out[0].add, out[0].del, out[0].files), (15, 3, 2));
    }
}
