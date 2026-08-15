//! Which repositories are on this disk, and where.
//!
//! An agent needs a checkout. This program browses far more repositories than
//! any machine holds — a hundred and forty on GitHub against twenty here — so
//! "where is it" is a real question with three answers, and the interesting
//! one is "nowhere".
//!
//! Indexed by git remote rather than by directory name. A clone of
//! `Osmait/sbql` in a folder called `sbql-experiment` is still that repository,
//! and a folder called `sbql` that is a clone of someone else's fork is not.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `owner/repo` → the checkout it lives in.
pub type Index = HashMap<String, PathBuf>;

/// How far below a root to look. Two levels covers `~/code/project` and
/// `~/work/client/project`, which is where clones actually sit; going deeper
/// mostly finds `node_modules`.
const DEPTH: usize = 3;

/// A ceiling on directories visited, so a root pointed at something enormous
/// degrades into an incomplete index rather than a hung service thread.
const MAX_DIRS: usize = 4000;

/// Where to look when nothing is configured.
///
/// The conventional places people keep code, plus the home directory itself.
/// Whichever do not exist cost one failed `read_dir` each.
const DEFAULT_ROOTS: &[&str] = &[
    "src", "code", "Code", "projects", "Projects", "repos", "dev", "git", "work", "orca",
];

/// The roots to scan, from config or from convention.
pub fn roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    match crate::shared::settings::current().get("clone-roots") {
        Some(list) if !list.trim().is_empty() => list
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| {
                p.strip_prefix("~/")
                    .map_or_else(|| PathBuf::from(p), |rest| home.join(rest))
            })
            .collect(),
        _ => DEFAULT_ROOTS.iter().map(|d| home.join(d)).collect(),
    }
}

/// Walks the roots and indexes every checkout found.
pub fn scan() -> Index {
    let mut index = Index::new();
    let mut budget = MAX_DIRS;
    for root in roots() {
        walk(&root, DEPTH, &mut budget, &mut index);
    }
    index
}

fn walk(dir: &Path, depth: usize, budget: &mut usize, out: &mut Index) {
    if depth == 0 || *budget == 0 {
        return;
    }
    *budget -= 1;

    // A checkout is a leaf: its subdirectories are its contents, not more
    // repositories. Its own worktrees live elsewhere and have `.git` as a
    // file, which `remote_of` declines.
    if let Some(slug) = remote_of(dir) {
        out.entry(slug).or_insert_with(|| dir.to_path_buf());
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if hidden || !path.is_dir() {
            continue;
        }
        walk(&path, depth - 1, budget, out);
    }
}

/// The `owner/repo` this directory is a checkout of, if it is one.
///
/// `.git` as a *file* means a linked worktree, which points back at a checkout
/// already indexed; returning it too would make the same repository resolve to
/// whichever the walk happened to reach first.
fn remote_of(dir: &Path) -> Option<String> {
    let git = dir.join(".git");
    if !git.is_dir() {
        return None;
    }
    let config = std::fs::read_to_string(git.join("config")).ok()?;
    origin_url(&config).as_deref().and_then(slug_of)
}

/// The `url` of `[remote "origin"]`, ignoring every other remote's.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = line.replace(char::is_whitespace, "") == r#"[remote"origin"]"#;
            continue;
        }
        if in_origin
            && let Some(rest) = line.strip_prefix("url")
            && let Some(url) = rest.trim_start().strip_prefix('=')
        {
            return Some(url.trim().to_string());
        }
    }
    None
}

/// `owner/repo` out of any of the shapes a GitHub remote is written in.
pub fn slug_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("git://github.com/"))?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let (owner, repo) = rest.split_once('/')?;
    // a URL with a path below the repository is not a clone URL
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

/// The branch a checkout currently has out.
///
/// Read rather than assumed because "work in the checkout" means working on
/// whatever is there — usually `main`, sometimes not, and the difference is
/// the reader's to know before they agree to it.
pub fn head_branch(repo_root: &str) -> Option<String> {
    let head = std::fs::read_to_string(Path::new(repo_root).join(".git").join("HEAD")).ok()?;
    branch_of_head(&head)
}

/// `ref: refs/heads/main` → `main`. A detached HEAD is a sha, which is not a
/// branch and is reported as none.
fn branch_of_head(head: &str) -> Option<String> {
    let name = head.trim().strip_prefix("ref:")?.trim();
    let name = name.strip_prefix("refs/heads/")?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Where a repository that is not here yet should be cloned to.
///
/// The first configured root that exists, so a clone lands beside the others
/// rather than wherever the program happened to be started from.
pub fn clone_dir() -> Option<PathBuf> {
    roots().into_iter().find(|r| r.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- reading a remote ---

    #[test]
    fn every_shape_of_github_remote_gives_the_same_slug() {
        for url in [
            "git@github.com:Osmait/sbql.git",
            "git@github.com:Osmait/sbql",
            "https://github.com/Osmait/sbql.git",
            "https://github.com/Osmait/sbql",
            "https://github.com/Osmait/sbql/",
            "ssh://git@github.com/Osmait/sbql.git",
            "git://github.com/Osmait/sbql.git",
        ] {
            assert_eq!(slug_of(url).as_deref(), Some("Osmait/sbql"), "{url}");
        }
    }

    #[test]
    fn a_remote_that_is_not_github_is_not_indexed() {
        assert_eq!(slug_of("git@gitlab.com:Osmait/sbql.git"), None);
        assert_eq!(slug_of("https://example.com/Osmait/sbql"), None);
        assert_eq!(slug_of(""), None);
    }

    #[test]
    fn a_url_deeper_than_a_repository_is_not_a_clone_url() {
        // an issue link is not somewhere you can start an agent
        assert_eq!(slug_of("https://github.com/Osmait/sbql/issues/14"), None);
    }

    #[test]
    fn a_repository_named_like_an_extension_keeps_its_name() {
        assert_eq!(
            slug_of("https://github.com/a/b.github.io").as_deref(),
            Some("a/b.github.io")
        );
    }

    // --- reading a git config ---

    const CONFIG: &str = r#"
[core]
	bare = false
[remote "upstream"]
	url = https://github.com/other/sbql.git
[remote "origin"]
	url = git@github.com:Osmait/sbql.git
	fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
	remote = origin
"#;

    #[test]
    fn the_origin_is_read_and_the_other_remotes_are_not() {
        assert_eq!(
            origin_url(CONFIG).as_deref(),
            Some("git@github.com:Osmait/sbql.git"),
            "upstream comes first in the file and must not win"
        );
    }

    #[test]
    fn a_config_with_no_origin_yields_nothing() {
        assert_eq!(origin_url("[core]\n\tbare = false\n"), None);
    }

    #[test]
    fn a_url_key_outside_any_remote_is_not_an_origin() {
        assert_eq!(origin_url("url = git@github.com:a/b.git\n"), None);
    }

    #[test]
    fn spacing_in_the_section_header_does_not_matter() {
        assert!(origin_url("[remote \"origin\"]\nurl=git@github.com:a/b\n").is_some());
        assert!(origin_url("[ remote \"origin\" ]\nurl = git@github.com:a/b\n").is_some());
    }

    // --- which branch is out ---

    #[test]
    fn a_head_pointing_at_a_branch_names_it() {
        assert_eq!(
            branch_of_head("ref: refs/heads/main\n").as_deref(),
            Some("main")
        );
        assert_eq!(
            branch_of_head("ref: refs/heads/feature/a-b\n").as_deref(),
            Some("feature/a-b")
        );
    }

    #[test]
    fn a_detached_head_is_not_a_branch() {
        assert_eq!(branch_of_head("9a8b7c6d5e4f3a2b1c0d\n"), None);
    }

    #[test]
    fn a_head_that_makes_no_sense_is_not_a_branch() {
        assert_eq!(branch_of_head(""), None);
        assert_eq!(branch_of_head("ref: refs/tags/v1\n"), None);
    }

    // --- the walk, against a real directory tree ---

    struct Tree(PathBuf);

    impl Tree {
        fn new(tag: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("gh-tui-clones-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }

        /// A checkout of `slug` at `rel`.
        fn repo(&self, rel: &str, slug: &str) -> PathBuf {
            let dir = self.0.join(rel);
            std::fs::create_dir_all(dir.join(".git")).unwrap();
            std::fs::write(
                dir.join(".git").join("config"),
                format!("[remote \"origin\"]\n\turl = git@github.com:{slug}.git\n"),
            )
            .unwrap();
            dir
        }

        fn plain(&self, rel: &str) -> PathBuf {
            let dir = self.0.join(rel);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn index(&self) -> Index {
            let mut out = Index::new();
            let mut budget = MAX_DIRS;
            walk(&self.0, DEPTH, &mut budget, &mut out);
            out
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_checkout_is_indexed_by_its_remote_not_its_folder() {
        let t = Tree::new("remote");
        let dir = t.repo("some-other-name", "Osmait/sbql");
        assert_eq!(t.index().get("Osmait/sbql"), Some(&dir));
    }

    #[test]
    fn a_directory_that_is_not_a_checkout_is_walked_through() {
        let t = Tree::new("nested");
        let dir = t.repo("client/thing", "Osmait/thing");
        assert_eq!(t.index().get("Osmait/thing"), Some(&dir));
    }

    #[test]
    fn the_walk_has_a_floor_and_it_is_two_levels_below_a_root() {
        // Documented rather than incidental: `~/work/client/project` is the
        // deepest arrangement anyone actually uses, and going further mostly
        // finds vendored copies.
        let t = Tree::new("toodeep");
        t.repo("a/b/c/thing", "Osmait/buried");
        assert!(t.index().is_empty());
    }

    #[test]
    fn the_walk_does_not_descend_into_a_checkout() {
        // a vendored copy inside a repository is not a repository you work in
        let t = Tree::new("vendored");
        t.repo("outer", "Osmait/outer");
        t.repo("outer/vendor/inner", "Someone/inner");

        let index = t.index();
        assert!(index.contains_key("Osmait/outer"));
        assert!(
            !index.contains_key("Someone/inner"),
            "the walk stopped at the outer checkout"
        );
    }

    #[test]
    fn hidden_directories_are_skipped() {
        let t = Tree::new("hidden");
        t.repo(".cache/thing", "Osmait/cached");
        assert!(t.index().is_empty());
    }

    #[test]
    fn a_linked_worktree_is_not_mistaken_for_the_checkout() {
        // git writes `.git` as a file in a worktree; indexing it too would make
        // the repository resolve to whichever the walk reached first
        let t = Tree::new("worktree");
        let real = t.repo("sbql", "Osmait/sbql");
        let wt = t.plain("sbql-worktrees/feature");
        std::fs::write(
            wt.join(".git"),
            "gitdir: /somewhere/.git/worktrees/feature\n",
        )
        .unwrap();

        assert_eq!(t.index().get("Osmait/sbql"), Some(&real));
    }

    #[test]
    fn a_checkout_with_no_origin_is_simply_not_indexed() {
        let t = Tree::new("noremote");
        let dir = t.plain("local-only");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join(".git").join("config"), "[core]\n\tbare = false\n").unwrap();
        assert!(t.index().is_empty());
    }

    #[test]
    fn the_walk_stops_at_its_budget_rather_than_running_away() {
        let t = Tree::new("budget");
        t.repo("a", "x/a");
        t.repo("b", "x/b");

        let mut out = Index::new();
        let mut budget = 1; // enough for the root and nothing under it
        walk(&t.0, DEPTH, &mut budget, &mut out);
        assert!(out.len() < 2, "an incomplete index beats a hung thread");
    }

    #[test]
    fn a_root_that_does_not_exist_is_not_an_error() {
        let mut out = Index::new();
        let mut budget = MAX_DIRS;
        walk(Path::new("/nowhere/at/all"), DEPTH, &mut budget, &mut out);
        assert!(out.is_empty());
    }
}
