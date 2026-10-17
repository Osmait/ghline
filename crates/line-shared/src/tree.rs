//! A diff's changed files as a folder tree that folds.
//!
//! Both programs list the files a change touches, and a change of any size is
//! mostly read by where it lands: the backend or the frontend, the tests or
//! the code. A tree says that at a glance, and folding a folder takes a whole
//! area out of the way once it has been read.
//!
//! The tree is rebuilt from the paths whenever it is asked for rather than kept
//! beside them. A file list is replaced wholesale when a new diff arrives, and
//! a second copy would be a second thing to keep in step with it; a few
//! thousand paths cost nothing next to drawing the diff beside them.
//!
//! [`Folds`] is the state a tree pane keeps: which folders are folded, and
//! whether the cursor is on a folder. When it is not, the cursor is on the
//! file the caller is showing — so the diff keeps the last file opened while
//! the cursor moves over folders, which is what the reader is still reading.
//! Every method takes that file, by index, rather than holding it: it is the
//! caller's, and each program already keeps one.

use std::collections::{BTreeMap, HashSet};

/// One changed file, as the tree needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leaf {
    /// Its path from the repository's root, `/`-separated.
    pub path: String,
    /// Lines added.
    pub add: u32,
    /// Lines deleted.
    pub del: u32,
}

/// What a row of the tree stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A folder, by its full path without a trailing slash.
    Dir(String),
    /// A file, by its index into the list the tree was built from.
    File(usize),
}

/// One row of the tree, as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// What the row is.
    pub entry: Entry,
    /// How many folders up it sits under.
    pub depth: usize,
    /// A file's name, or a folder's. Folders that hold nothing but the next
    /// folder are joined into one row with `/`, as GitHub does, so a Java
    /// package does not cost a row per segment.
    pub label: String,
    /// Lines added, summed over everything under a folder.
    pub add: u32,
    /// Lines deleted, summed the same way.
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
    fn totals(&self, all: &[Leaf]) -> (u32, u32, usize) {
        let mut t = (0, 0, self.files.len());
        for (_, i) in &self.files {
            if let Some(f) = all.get(*i) {
                t.0 += f.add;
                t.1 += f.del;
            }
        }
        for d in self.dirs.values() {
            let (a, r, n) = d.totals(all);
            t = (t.0 + a, t.1 + r, t.2 + n);
        }
        t
    }
}

/// The rows of the tree, with the folders in `folded` closed.
///
/// Folders come before files at every level, each alphabetically, which is
/// how GitHub lays out the same tree.
pub fn rows(files: &[Leaf], folded: &HashSet<String>) -> Vec<Row> {
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
    all: &[Leaf],
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
        let (add, del) = all.get(i).map_or((0, 0), |f| (f.add, f.del));
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

/// The files in the order the tree shows them, folded or not.
pub fn order(files: &[Leaf]) -> Vec<usize> {
    rows(files, &HashSet::new())
        .into_iter()
        .filter_map(|r| match r.entry {
            Entry::File(i) => Some(i),
            Entry::Dir(_) => None,
        })
        .collect()
}

/// The first file in tree order, which is where a diff opens.
pub fn first(files: &[Leaf]) -> usize {
    order(files).first().copied().unwrap_or(0)
}

/// Where `file` falls among all of them in tree order — the order the reader
/// sees, not the order the list arrived in.
pub fn ordinal(files: &[Leaf], file: usize) -> usize {
    order(files).iter().position(|i| *i == file).unwrap_or(0)
}

/// The folders whose path `path` lies under, as rows of the tree.
fn above<'r>(rows: &'r [Row], path: &str) -> impl Iterator<Item = &'r str> {
    rows.iter().filter_map(move |r| match &r.entry {
        Entry::Dir(d) if path.starts_with(&format!("{d}/")) => Some(d.as_str()),
        _ => None,
    })
}

/// A tree pane's state. See the module documentation for why the selected
/// file is not part of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Folds {
    /// Folded folders, by full path.
    pub folded: HashSet<String>,
    /// The folder the cursor is on; `None` when it is on the caller's file.
    pub dir: Option<String>,
}

impl Folds {
    /// Everything open and the cursor back on the file, for a new list.
    pub fn clear(&mut self) {
        self.folded.clear();
        self.dir = None;
    }

    /// The tree as it is drawn now.
    pub fn rows(&self, files: &[Leaf]) -> Vec<Row> {
        rows(files, &self.folded)
    }

    /// The row the cursor is on.
    ///
    /// On a folder when one was chosen, and otherwise on `file`. When that
    /// file is inside a folded folder the cursor rests on the innermost row
    /// still standing over it — the folded one — so folding never strands it.
    pub fn cursor(&self, rows: &[Row], files: &[Leaf], file: usize) -> usize {
        if let Some(dir) = &self.dir
            && let Some(i) = rows.iter().position(|r| r.entry == Entry::Dir(dir.clone()))
        {
            return i;
        }
        if let Some(i) = rows.iter().position(|r| r.entry == Entry::File(file)) {
            return i;
        }
        let path = files.get(file).map_or("", |f| f.path.as_str());
        // ancestors come in order from the top down, so the last is innermost
        rows.iter()
            .rposition(|r| match &r.entry {
                Entry::Dir(d) => path.starts_with(&format!("{d}/")),
                Entry::File(_) => false,
            })
            .unwrap_or(0)
    }

    /// Puts the cursor on row `i`. A file comes back, to become the one shown;
    /// a folder leaves the caller's file where it was.
    pub fn select(&mut self, files: &[Leaf], i: usize) -> Option<usize> {
        match self.rows(files).get(i).map(|r| r.entry.clone()) {
            Some(Entry::Dir(d)) => {
                self.dir = Some(d);
                None
            }
            Some(Entry::File(f)) => {
                self.dir = None;
                Some(f)
            }
            None => None,
        }
    }

    /// Moves the cursor `d` rows; a file landed on comes back.
    pub fn step(&mut self, files: &[Leaf], file: usize, d: i64) -> Option<usize> {
        let rows = self.rows(files);
        if rows.is_empty() {
            return None;
        }
        let at = self.cursor(&rows, files, file) as i64;
        let to = (at + d).clamp(0, rows.len() as i64 - 1) as usize;
        self.select(files, to)
    }

    /// The row under the cursor, when it is a folder.
    fn cursor_dir(&self, files: &[Leaf], file: usize) -> Option<Row> {
        let rows = self.rows(files);
        rows.get(self.cursor(&rows, files, file))
            .filter(|r| matches!(r.entry, Entry::Dir(_)))
            .cloned()
    }

    /// The folder the cursor is in: the one under it, or a file's own —
    /// the nearest row above it, which may be a joined one.
    fn cursor_folder(&self, files: &[Leaf], file: usize) -> Option<String> {
        if let Some(Row {
            entry: Entry::Dir(d),
            ..
        }) = self.cursor_dir(files, file)
        {
            return Some(d);
        }
        let rows = self.rows(files);
        let path = &files.get(file)?.path;
        above(&rows, path)
            .max_by_key(|d| d.len())
            .map(str::to_string)
    }

    fn toggle(&mut self, d: String) {
        if !self.folded.remove(&d) {
            self.folded.insert(d.clone());
            // folding the folder a file is in moves the cursor up to it
            self.dir = Some(d);
        }
    }

    /// `enter`: a folder under the cursor folds or opens, and `true` says so;
    /// `false` means the cursor is on a file, which the caller opens.
    pub fn enter(&mut self, files: &[Leaf], file: usize) -> bool {
        match self.cursor_dir(files, file) {
            Some(Row {
                entry: Entry::Dir(d),
                ..
            }) => {
                self.toggle(d);
                true
            }
            _ => false,
        }
    }

    /// `o`: folds or opens the folder the cursor is in.
    pub fn toggle_here(&mut self, files: &[Leaf], file: usize) {
        if let Some(d) = self.cursor_folder(files, file) {
            self.toggle(d);
        }
    }

    /// `h`. Folds the open folder under the cursor; on a file or a folded
    /// folder, folds the one it sits in. `false` at the top level, where there
    /// is nothing to fold and `h` keeps whatever else it means.
    pub fn fold_left(&mut self, files: &[Leaf], file: usize) -> bool {
        let here = self.cursor_dir(files, file);
        if let Some(Row {
            entry: Entry::Dir(d),
            folded: false,
            ..
        }) = &here
        {
            self.folded.insert(d.clone());
            self.dir = Some(d.clone());
            return true;
        }
        let up = match here {
            // the folder over this one, as the tree draws it
            Some(Row {
                entry: Entry::Dir(d),
                ..
            }) => {
                let rows = self.rows(files);
                above(&rows, &d).max_by_key(|p| p.len()).map(str::to_string)
            }
            _ => self.cursor_folder(files, file),
        };
        match up {
            Some(p) => {
                self.folded.insert(p.clone());
                self.dir = Some(p);
                true
            }
            None => false,
        }
    }

    /// `l`. Opens a folded folder under the cursor; `false` otherwise, where
    /// `l` keeps whatever else it means.
    pub fn unfold_right(&mut self, files: &[Leaf], file: usize) -> bool {
        match self.cursor_dir(files, file) {
            Some(Row {
                entry: Entry::Dir(d),
                folded: true,
                ..
            }) => {
                self.folded.remove(&d);
                true
            }
            _ => false,
        }
    }

    /// `O`: everything open if anything is folded, everything folded if not,
    /// with the cursor on the top-level folder `file` is in.
    pub fn toggle_all(&mut self, files: &[Leaf], file: usize) {
        if !self.folded.is_empty() {
            self.folded.clear();
            return;
        }
        let all: HashSet<String> = rows(files, &HashSet::new())
            .into_iter()
            .filter_map(|r| match r.entry {
                Entry::Dir(d) => Some(d),
                Entry::File(_) => None,
            })
            .collect();
        let top = files.get(file).and_then(|f| {
            let rows = rows(files, &all);
            above(&rows, &f.path).next().map(str::to_string)
        });
        self.folded = all;
        self.dir = top;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "assertions")]
mod tests {
    use super::*;

    fn leaf(path: &str, add: u32, del: u32) -> Leaf {
        Leaf {
            path: path.into(),
            add,
            del,
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
            leaf("README.md", 1, 0),
            leaf("src/main.rs", 2, 0),
            leaf("src/app/state.rs", 3, 0),
            leaf("Cargo.toml", 1, 1),
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
            leaf("BackEnd/internal/platform/oauth.go", 1, 0),
            leaf("BackEnd/internal/platform/server.go", 1, 0),
            leaf("BackEnd/go.mod", 1, 0),
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
            leaf("src/a.rs", 10, 2),
            leaf("src/deep/b.rs", 5, 1),
            leaf("x.rs", 1, 0),
        ];
        let folded: HashSet<String> = ["src".to_string()].into();
        let out = rows(&files, &folded);
        assert_eq!(labels(&out), ["src", "x.rs"]);
        assert!(out[0].folded);
        assert_eq!((out[0].add, out[0].del, out[0].files), (15, 3, 2));
    }

    /// `src/layout/` (mod.rs, solver.rs), `tests/` (layout.rs), CHANGELOG.md,
    /// in a list order that is not the tree's.
    fn fixture() -> Vec<Leaf> {
        vec![
            leaf("src/layout/solver.rs", 64, 18),
            leaf("src/layout/mod.rs", 12, 4),
            leaf("tests/layout.rs", 7, 0),
            leaf("CHANGELOG.md", 0, 0),
        ]
    }

    #[test]
    fn the_first_file_is_the_trees_and_not_the_lists() {
        assert_eq!(first(&fixture()), 1, "mod.rs, before solver.rs");
        assert_eq!(ordinal(&fixture(), 0), 1);
    }

    #[test]
    fn h_folds_the_folder_a_file_is_in_and_l_opens_it() {
        let files = fixture();
        let mut f = Folds::default();
        assert!(f.fold_left(&files, 1));
        assert_eq!(f.dir.as_deref(), Some("src/layout"));
        assert!(f.unfold_right(&files, 1));
        assert!(f.folded.is_empty());
    }

    #[test]
    fn a_file_at_the_top_has_nothing_to_fold() {
        let mut f = Folds::default();
        assert!(!f.fold_left(&fixture(), 3));
    }

    #[test]
    fn stepping_past_a_folded_folder_skips_what_is_inside_it() {
        let files = fixture();
        let mut f = Folds::default();
        f.fold_left(&files, 1);
        assert_eq!(f.step(&files, 1, 1), None, "onto tests/, a folder");
        assert_eq!(f.step(&files, 1, 1), Some(2), "then tests/layout.rs");
    }

    #[test]
    fn capital_o_folds_everything_then_opens_everything() {
        let files = fixture();
        let mut f = Folds::default();
        f.toggle_all(&files, 1);
        assert_eq!(
            labels(&f.rows(&files)),
            ["src/layout", "tests", "CHANGELOG.md"]
        );
        assert_eq!(f.dir.as_deref(), Some("src/layout"));
        f.toggle_all(&files, 1);
        assert!(f.folded.is_empty());
    }

    #[test]
    fn enter_on_a_folder_folds_it_and_on_a_file_says_so() {
        let files = fixture();
        let mut f = Folds {
            dir: Some("tests".into()),
            ..Folds::default()
        };
        assert!(f.enter(&files, 1));
        assert!(f.folded.contains("tests"));
        f.dir = None;
        assert!(!f.enter(&files, 1), "the cursor is on mod.rs");
    }
}
