//! Asking git what changed.
//!
//! Everything here blocks and runs on the service thread. `git` is invoked
//! with `-c core.pager=cat` throughout: the reader may well have delta set as
//! their pager — this program recommends it — and a pager would either hang
//! waiting for a terminal or hand back ANSI escapes instead of a diff.

use std::process::Command;

use super::model::{ChangedFile, Kind, Row, Scope, Status};
use crate::error::{Error, Result as Res};

/// Runs git in `repo` and returns its stdout.
fn run(repo: &str, args: &[&str]) -> Res<String> {
    let label = std::iter::once("git")
        .chain(args.iter().take(2).copied())
        .collect::<Vec<_>>()
        .join(" ");
    let out = Command::new("git")
        .args(["-C", repo, "-c", "core.pager=cat", "--no-pager"])
        .args(args)
        .output()
        .map_err(|source| Error::Spawn {
            program: "git",
            source,
        })?;

    if !out.status.success() {
        return Err(Error::Command {
            args: label,
            status: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// git, the version control this was written against.
///
/// A unit struct: a call is a process, and every bit of state lives in the
/// repository rather than here.
pub struct Git;

impl super::vcs::Vcs for Git {
    fn name(&self) -> &'static str {
        "git"
    }

    fn is_repo(&self, dir: &str) -> bool {
        is_repo(dir)
    }

    fn head_branch(&self, repo: &str) -> Option<String> {
        head_branch(repo)
    }

    fn base_branch(&self, repo: &str) -> String {
        base_branch(repo)
    }

    fn changed_files(&self, repo: &str, scope: &Scope) -> Res<Vec<ChangedFile>> {
        changed_files(repo, scope)
    }

    fn file_diff(&self, repo: &str, scope: &Scope, path: &str, context: u32) -> Res<Vec<Row>> {
        file_diff(repo, scope, path, context)
    }

    // `git blame` is exactly this question, which is why the trait has it at
    // all — a backend that could not answer would say so here instead.
    fn has_blame(&self) -> bool {
        true
    }

    fn blame(&self, repo: &str, path: &str) -> Res<Vec<String>> {
        blame(repo, path)
    }
}

/// Is this a git repository at all?
pub fn is_repo(dir: &str) -> bool {
    run(dir, &["rev-parse", "--git-dir"]).is_ok()
}

/// The branch that is checked out, or `None` on a detached HEAD.
pub fn head_branch(repo: &str) -> Option<String> {
    let out = run(repo, &["symbolic-ref", "--short", "HEAD"]).ok()?;
    let name = out.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The branch a review would land on: the first of the usual names that
/// actually exists, since a repository has one or the other and not both.
pub fn base_branch(repo: &str) -> String {
    for name in ["main", "master", "develop"] {
        if run(repo, &["rev-parse", "--verify", "--quiet", name]).is_ok() {
            return name.to_string();
        }
    }
    "main".into()
}

/// The files a scope touches, with their counts.
///
/// Two calls rather than one: `--numstat` has the counts and `--name-status`
/// has the letter, and there is no format that carries both. They are joined
/// on the path, which is the only thing they agree on.
pub fn changed_files(repo: &str, scope: &Scope) -> Res<Vec<ChangedFile>> {
    // bound first: the Vec<String> has to outlive the borrows taken from it
    let owned = scope.args();
    let sel: Vec<&str> = owned.iter().map(String::as_str).collect();

    let mut args = vec!["diff", "--numstat", "--no-color"];
    args.extend_from_slice(&sel);
    let numstat = run(repo, &args)?;

    let mut args = vec!["diff", "--name-status", "--no-color"];
    args.extend_from_slice(&sel);
    let names = run(repo, &args)?;

    Ok(join_stats(&numstat, &names))
}

/// Pairs `--numstat` rows with `--name-status` rows.
fn join_stats(numstat: &str, names: &str) -> Vec<ChangedFile> {
    let status_of = |path: &str| -> Status {
        names
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .find(|(_, rest)| {
                // a rename is `R096\told\tnew`; the path we want is the last
                rest.rsplit('\t').next() == Some(path)
            })
            .map_or(Status::Modified, |(mark, _)| Status::from(mark))
    };

    numstat
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let add = parts.next()?;
            let del = parts.next()?;
            // `next_back` rather than `last`: a rename writes both paths and
            // the one that matters is the new one, at the end
            let path = parts.next_back()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(ChangedFile {
                status: status_of(path),
                // git writes `-` for a binary file, which is not zero changes
                // but no countable ones
                add: add.parse().unwrap_or(0),
                del: del.parse().unwrap_or(0),
                path: path.to_string(),
            })
        })
        .collect()
}

/// One file's diff, at `context` lines either side.
pub fn file_diff(repo: &str, scope: &Scope, path: &str, context: u32) -> Res<Vec<Row>> {
    let unified = format!("-U{context}");
    let owned = scope.args();
    let sel: Vec<&str> = owned.iter().map(String::as_str).collect();

    let mut args = vec!["diff", "--no-color", &unified];
    args.extend_from_slice(&sel);
    args.push("--");
    args.push(path);

    Ok(parse_unified(&run(repo, &args)?))
}

/// Turns `git diff` output into rows, numbering both sides as it goes.
///
/// Everything before the first `@@` is dropped: the `diff --git`, `index`,
/// `---` and `+++` preamble is about the file, and the file is already named
/// by the pane's header.
pub fn parse_unified(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0u32, 0u32);
    let mut started = false;

    for line in text.lines() {
        if line.starts_with("@@") {
            started = true;
            let (o, n) = parse_hunk_header(line);
            old = o;
            new = n;
            out.push(Row {
                kind: Kind::Header,
                old: None,
                new: None,
                // `@@ … @@` carries trailing context copied out of the file,
                // so it is as capable of holding a tab as any other line.
                text: crate::text::expand_tabs(line).into_owned(),
            });
            continue;
        }
        if !started {
            continue;
        }
        // `\ No newline at end of file` is a note about the line above, not a
        // line of its own.
        if line.starts_with('\\') {
            continue;
        }

        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (Kind::Added, &line[1..]),
            Some(b'-') => (Kind::Deleted, &line[1..]),
            Some(b' ') => (Kind::Context, &line[1..]),
            // A completely empty line in the body is a context line whose
            // content is empty; git writes it without the leading space.
            None => (Kind::Context, ""),
            _ => continue,
        };

        let (o, n) = match kind {
            Kind::Added => {
                new += 1;
                (None, Some(new))
            }
            Kind::Deleted => {
                old += 1;
                (Some(old), None)
            }
            _ => {
                old += 1;
                new += 1;
                (Some(old), Some(new))
            }
        };
        out.push(Row {
            kind,
            old: o,
            new: n,
            // Expanded here rather than at the point of drawing, so that the
            // colour spans and the comment anchors are offsets into the same
            // string the pane shows.
            text: crate::text::expand_tabs(text).into_owned(),
        });
    }
    out
}

/// The starting line numbers out of `@@ -14,7 +16,9 @@`.
///
/// Returns the number *before* the first line, so the counters can be
/// incremented as each row is emitted rather than after.
fn parse_hunk_header(line: &str) -> (u32, u32) {
    let mut old = 0;
    let mut new = 0;
    for tok in line.split_whitespace() {
        let (sign, rest) = match tok.as_bytes().first() {
            Some(b'-') => ('-', &tok[1..]),
            Some(b'+') => ('+', &tok[1..]),
            _ => continue,
        };
        let n: u32 = rest
            .split(',')
            .next()
            .and_then(|d| d.parse().ok())
            .unwrap_or(1);
        // a zero start means the side is empty; the first line is still 1
        let start = n.saturating_sub(1);
        if sign == '-' {
            old = start;
        } else {
            new = start;
        }
    }
    (old, new)
}

/// Who last touched each line of `path`, as `sha author when` strings indexed
/// by line number.
///
/// `--porcelain` because the human format wraps and truncates, and this has to
/// fit a fixed column.
pub fn blame(repo: &str, path: &str) -> Res<Vec<String>> {
    let out = run(repo, &["blame", "--porcelain", "--", path])?;
    Ok(parse_blame(&out))
}

fn parse_blame(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut sha = String::new();
    let mut author = String::new();
    let mut when = 0i64;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("author-time ") {
            when = rest.trim().parse().unwrap_or(0);
        } else if line.starts_with('\t') {
            // the content line closes an entry
            out.push(format!(
                "{} {} {}",
                &sha[..sha.len().min(7)],
                author,
                crate::ago::since(when)
            ));
        } else if let Some(first) = line.split(' ').next()
            && first.len() == 40
            && first.chars().all(|c| c.is_ascii_hexdigit())
        {
            sha = first.to_string();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_row_reaches_the_renderer_holding_a_tab() {
        // A tab is measured as one column by the code that lays the pane out
        // and drawn as up to four by the terminal, so a row carrying one is
        // painted past the edge of its pane and over the one beside it.
        let rows =
            parse_unified("@@ -1,2 +1,3 @@ fn main() {\n \tcontext();\n-\told();\n+\t\tnew();\n");
        assert!(!rows.is_empty(), "the fixture should parse");
        for r in &rows {
            assert!(
                !r.text.contains('\t'),
                "{:?} still holds a tab: {:?}",
                r.kind,
                r.text
            );
        }
        // and the expansion is the indentation, not a blanket four spaces
        let added = rows.iter().find(|r| r.kind == Kind::Added);
        assert_eq!(added.map(|r| r.text.as_str()), Some("        new();"));
    }

    const DIFF: &str = "\
diff --git a/src/a.rs b/src/a.rs
index e438487..1ec8869 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -14,7 +14,9 @@ fn main() {
 use std::io;
-use old::Thing;
+use new::Thing;
+use new::Other;

 fn main() {
";

    // --- the unified parser ---

    #[test]
    fn the_preamble_is_dropped() {
        let rows = parse_unified(DIFF);
        assert_eq!(rows[0].kind, Kind::Header, "the first row is the @@ line");
        assert!(
            !rows.iter().any(|r| r.text.starts_with("diff --git")),
            "the file is already named by the pane"
        );
    }

    #[test]
    fn both_sides_are_numbered_from_the_hunk_header() {
        let rows = parse_unified(DIFF);
        let code: Vec<&Row> = rows.iter().filter(|r| r.kind.is_code()).collect();

        // `@@ -14,7 +14,9 @@` — the first line of each side is 14
        assert_eq!((code[0].old, code[0].new), (Some(14), Some(14)));
    }

    #[test]
    fn a_deleted_line_advances_only_the_old_side() {
        let rows = parse_unified(DIFF);
        let del = rows.iter().find(|r| r.kind == Kind::Deleted).unwrap();
        assert_eq!(del.old, Some(15));
        assert_eq!(del.new, None, "it will not be in the new file");
    }

    #[test]
    fn an_added_line_advances_only_the_new_side() {
        let rows = parse_unified(DIFF);
        let adds: Vec<&Row> = rows.iter().filter(|r| r.kind == Kind::Added).collect();
        assert_eq!(adds.len(), 2);
        assert_eq!((adds[0].old, adds[0].new), (None, Some(15)));
        assert_eq!((adds[1].old, adds[1].new), (None, Some(16)));
    }

    #[test]
    fn the_sides_stay_in_step_after_a_change() {
        // the context line following +2/-1 should be old 16, new 17
        let rows = parse_unified(DIFF);
        let after = rows
            .iter()
            .filter(|r| r.kind == Kind::Context)
            .nth(1)
            .unwrap();
        assert_eq!((after.old, after.new), (Some(16), Some(17)));
    }

    #[test]
    fn the_leading_marker_is_not_part_of_the_line() {
        let rows = parse_unified(DIFF);
        let add = rows.iter().find(|r| r.kind == Kind::Added).unwrap();
        assert_eq!(add.text, "use new::Thing;");
        assert_eq!(add.sign(), "+", "the sign is drawn separately");
    }

    #[test]
    fn a_blank_context_line_survives() {
        // git writes an empty context line with no leading space at all
        let rows = parse_unified("@@ -1,2 +1,2 @@\n a\n\n b\n");
        assert_eq!(rows.len(), 4, "header plus three lines");
        assert!(rows.iter().all(|r| r.kind != Kind::Deleted));
    }

    #[test]
    fn the_no_newline_note_is_not_a_line() {
        let rows = parse_unified("@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n");
        assert_eq!(rows.iter().filter(|r| r.kind.is_code()).count(), 2);
    }

    #[test]
    fn a_new_file_starts_at_line_one() {
        // `@@ -0,0 +1,2 @@` — nothing on the old side
        let rows = parse_unified("@@ -0,0 +1,2 @@\n+first\n+second\n");
        let adds: Vec<&Row> = rows.iter().filter(|r| r.kind == Kind::Added).collect();
        assert_eq!(adds[0].new, Some(1));
        assert_eq!(adds[1].new, Some(2));
    }

    #[test]
    fn several_hunks_each_restart_the_numbering() {
        let rows = parse_unified("@@ -1,1 +1,1 @@\n a\n@@ -80,1 +90,1 @@\n b\n");
        let ctx: Vec<&Row> = rows.iter().filter(|r| r.kind == Kind::Context).collect();
        assert_eq!((ctx[0].old, ctx[0].new), (Some(1), Some(1)));
        assert_eq!((ctx[1].old, ctx[1].new), (Some(80), Some(90)));
    }

    #[test]
    fn an_empty_diff_is_no_rows_rather_than_a_panic() {
        assert!(parse_unified("").is_empty());
        assert!(parse_unified("diff --git a/x b/x\nindex 1..2\n").is_empty());
    }

    #[test]
    fn a_hunk_header_without_counts_still_parses() {
        // `@@ -1 +1 @@` is legal when the hunk is one line
        let rows = parse_unified("@@ -1 +1 @@\n-a\n+b\n");
        let del = rows.iter().find(|r| r.kind == Kind::Deleted).unwrap();
        assert_eq!(del.old, Some(1));
    }

    // --- joining the two stat formats ---

    #[test]
    fn counts_and_letters_are_joined_on_the_path() {
        let files = join_stats(
            "24\t6\tsrc/router.ts\n48\t0\tsrc/new.ts\n0\t31\tsrc/old.ts\n",
            "M\tsrc/router.ts\nA\tsrc/new.ts\nD\tsrc/old.ts\n",
        );
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/router.ts");
        assert_eq!((files[0].add, files[0].del), (24, 6));
        assert_eq!(files[1].status, Status::Added);
        assert_eq!(files[2].status, Status::Deleted);
    }

    #[test]
    fn a_binary_file_counts_as_nothing_rather_than_failing() {
        // git writes `-` where it cannot count lines
        let files = join_stats("-\t-\tlogo.png\n", "M\tlogo.png\n");
        assert_eq!(files.len(), 1);
        assert_eq!((files[0].add, files[0].del), (0, 0));
    }

    #[test]
    fn a_rename_is_joined_on_its_new_path() {
        // numstat writes `old\tnew`, name-status writes `R096\told\tnew`
        let files = join_stats(
            "2\t1\tsrc/old.rs\tsrc/new.rs\n",
            "R096\tsrc/old.rs\tsrc/new.rs\n",
        );
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/new.rs");
        assert_eq!(files[0].status, Status::Renamed);
    }

    #[test]
    fn a_file_with_no_letter_is_assumed_modified() {
        let files = join_stats("1\t1\tx.rs\n", "");
        assert_eq!(files[0].status, Status::Modified);
    }

    #[test]
    fn nothing_changed_is_no_files() {
        assert!(join_stats("", "").is_empty());
    }

    // --- blame ---

    const BLAME: &str = "\
a3f19c2000000000000000000000000000000000 1 1 2
author Maria Okonkwo
author-time 1700000000
author-tz +0000
summary first
filename src/a.rs
\tuse std::io;
a3f19c2000000000000000000000000000000000 2 2
\t
7be0114000000000000000000000000000000000 3 3 1
author Luis Serrano
author-time 1780000000
summary second
filename src/a.rs
\tfn main() {
";

    #[test]
    fn blame_gives_one_entry_per_line() {
        let out = parse_blame(BLAME);
        assert_eq!(out.len(), 3, "including the line that repeats a commit");
    }

    #[test]
    fn a_blame_entry_carries_the_sha_the_author_and_the_age() {
        let out = parse_blame(BLAME);
        assert!(out[0].starts_with("a3f19c2 Maria Okonkwo "), "{}", out[0]);
        assert!(out[2].starts_with("7be0114 Luis Serrano "), "{}", out[2]);
    }

    #[test]
    fn a_repeated_commit_keeps_the_author_from_its_first_mention() {
        // git omits the header block when the same commit is seen again
        let out = parse_blame(BLAME);
        assert!(out[1].starts_with("a3f19c2 Maria Okonkwo"), "{}", out[1]);
    }

    #[test]
    fn blame_of_nothing_is_no_entries() {
        assert!(parse_blame("").is_empty());
    }

    // --- against a real repository ---
    //
    // The parser tests above use canned text, which proves nothing about
    // whether the command lines are right. These build a repository and ask
    // git for real.

    struct Repo(std::path::PathBuf);

    impl Repo {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("diffline-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let r = Self(dir);
            r.git(&["init", "-q", "-b", "main"]);
            r.git(&["config", "user.email", "t@example.com"]);
            r.git(&["config", "user.name", "Tester"]);
            r
        }

        fn git(&self, args: &[&str]) -> String {
            run(self.path(), args).unwrap_or_default()
        }

        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }

        fn write(&self, name: &str, body: &str) {
            std::fs::write(self.0.join(name), body).unwrap();
        }

        fn commit(&self, msg: &str) {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", msg]);
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_directory_that_is_not_a_repository_says_so() {
        assert!(!is_repo("/"), "the filesystem root is not a checkout");
    }

    #[test]
    fn the_working_tree_scope_sees_staged_and_unstaged_alike() {
        let r = Repo::new("worktree");
        r.write("a.txt", "one\ntwo\n");
        r.commit("first");

        r.write("a.txt", "one\nCHANGED\n");
        r.write("b.txt", "new file\n");
        r.git(&["add", "b.txt"]); // staged, and still expected to show

        let files = changed_files(r.path(), &Scope::WorkingTree).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"a.txt"), "unstaged edit: {paths:?}");
        assert!(paths.contains(&"b.txt"), "staged addition: {paths:?}");
    }

    #[test]
    fn a_file_added_and_one_deleted_are_told_apart() {
        let r = Repo::new("status");
        r.write("keep.txt", "x\n");
        r.write("gone.txt", "y\n");
        r.commit("first");

        std::fs::remove_file(r.0.join("gone.txt")).unwrap();
        r.write("fresh.txt", "z\n");
        r.git(&["add", "-A"]);

        let files = changed_files(r.path(), &Scope::WorkingTree).unwrap();
        let by = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.status);
        assert_eq!(by("gone.txt"), Some(Status::Deleted));
        assert_eq!(by("fresh.txt"), Some(Status::Added));
    }

    #[test]
    fn a_branch_scope_shows_this_branch_and_not_the_base() {
        // The reason for three dots. `base` moves on while the branch works;
        // two dots would report base's commit as this branch's doing.
        let r = Repo::new("branch");
        r.write("a.txt", "start\n");
        r.commit("first");

        r.git(&["checkout", "-q", "-b", "feature"]);
        r.write("mine.txt", "branch work\n");
        r.commit("mine");

        r.git(&["checkout", "-q", "main"]);
        r.write("theirs.txt", "base work\n");
        r.commit("theirs");
        r.git(&["checkout", "-q", "feature"]);

        let scope = Scope::Branch {
            base: "main".into(),
        };
        let files = changed_files(r.path(), &scope).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

        assert_eq!(paths, vec!["mine.txt"], "only this branch's work");
    }

    #[test]
    fn a_commit_scope_shows_that_commit_alone() {
        let r = Repo::new("commit");
        r.write("a.txt", "one\n");
        r.commit("first");
        r.write("b.txt", "two\n");
        r.commit("second");

        let sha = r.git(&["rev-parse", "HEAD"]).trim().to_string();
        let files = changed_files(r.path(), &Scope::Commit { sha }).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["b.txt"], "not the commit before it");
    }

    #[test]
    fn a_real_diff_numbers_its_lines() {
        let r = Repo::new("diff");
        r.write("a.txt", "one\ntwo\nthree\n");
        r.commit("first");
        r.write("a.txt", "one\nCHANGED\nthree\n");

        let rows = file_diff(r.path(), &Scope::WorkingTree, "a.txt", 3).unwrap();
        let del = rows.iter().find(|x| x.kind == Kind::Deleted).unwrap();
        let add = rows.iter().find(|x| x.kind == Kind::Added).unwrap();

        assert_eq!(del.text, "two");
        assert_eq!(del.old, Some(2));
        assert_eq!(add.text, "CHANGED");
        assert_eq!(add.new, Some(2));
    }

    #[test]
    fn more_context_really_asks_git_for_more() {
        let r = Repo::new("context");
        let body: String = (1..=40).map(|i| format!("line {i}\n")).collect();
        r.write("a.txt", &body);
        r.commit("first");
        r.write("a.txt", &body.replace("line 20\n", "CHANGED\n"));

        let narrow = file_diff(r.path(), &Scope::WorkingTree, "a.txt", 3).unwrap();
        let wide = file_diff(r.path(), &Scope::WorkingTree, "a.txt", 9).unwrap();
        assert!(
            wide.len() > narrow.len(),
            "±9 should show more than ±3: {} vs {}",
            wide.len(),
            narrow.len()
        );
    }

    #[test]
    fn blame_against_a_real_repository_names_the_author() {
        let r = Repo::new("blame");
        r.write("a.txt", "one\ntwo\n");
        r.commit("first");

        let lines = blame(r.path(), "a.txt").unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Tester"), "{}", lines[0]);
    }

    #[test]
    fn the_base_branch_is_the_one_that_exists() {
        let r = Repo::new("base");
        r.write("a.txt", "x\n");
        r.commit("first");
        assert_eq!(base_branch(r.path()), "main");
        assert_eq!(head_branch(r.path()).as_deref(), Some("main"));
    }

    #[test]
    fn asking_about_a_path_that_is_not_there_is_an_error_not_a_panic() {
        let r = Repo::new("missing");
        r.write("a.txt", "x\n");
        r.commit("first");
        // no such revision
        let scope = Scope::Commit {
            sha: "deadbeefdeadbeef".into(),
        };
        assert!(changed_files(r.path(), &scope).is_err());
    }
}
