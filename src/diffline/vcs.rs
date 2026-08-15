//! Where the diff comes from.
//!
//! git is what this was written against, and for now it is the only one — but
//! the questions diffline asks are not git's questions. "Which files did this
//! change touch", "what does this file's diff look like at three lines of
//! context", "who last wrote each line": jujutsu answers all three, and so
//! would a library that never spawns a process.
//!
//! So the questions are a trait and git is one answer to them. A pane, a
//! revision and a branch are opaque strings here; nothing above this layer
//! parses one.
//!
//! ## What not every backend can do
//!
//! Blame is the one that varies. git has `git blame`; jj spells it
//! `jj file annotate` and gives different columns; a backend reading a plain
//! diff off disk has nothing to offer at all. Rather than return an empty
//! list and have the interface draw thirty columns of nothing, a backend says
//! whether it can — the same shape as `Multiplexer::detects_status`, and for
//! the same reason: not knowing is not the same as knowing there is nobody.

use super::model::{ChangedFile, Row, Scope};
use crate::error::Result as Res;

/// Something that can be asked what changed.
///
/// `Sync + 'static` because the worker thread is what asks, and there is one
/// for the life of the process.
pub trait Vcs: Sync + 'static {
    /// What to call it, in a message that has to name it.
    fn name(&self) -> &'static str;

    /// Is `dir` a repository this backend understands?
    ///
    /// The question that picks the backend, so it has to be cheap and it has
    /// to be honest: a backend that says yes to everything takes repositories
    /// away from one that could actually read them.
    fn is_repo(&self, dir: &str) -> bool;

    /// The branch, bookmark or revision that is checked out, or `None` when
    /// the answer is not a name — a detached HEAD, an anonymous revision.
    fn head_branch(&self, repo: &str) -> Option<String>;

    /// What a review would land on.
    fn base_branch(&self, repo: &str) -> String;

    /// The files a scope touches, with their counts.
    fn changed_files(&self, repo: &str, scope: &Scope) -> Res<Vec<ChangedFile>>;

    /// One file's diff, at `context` lines either side, as rows.
    fn file_diff(&self, repo: &str, scope: &Scope, path: &str, context: u32) -> Res<Vec<Row>>;

    // --- provided ---

    /// Whether `blame` is worth asking for.
    ///
    /// A backend that cannot answer says so here rather than returning an
    /// empty list: the difference between "nobody wrote these lines" and "I
    /// cannot tell you who did" is the difference between a bug and a
    /// limitation, and only one of them should take thirty columns of the
    /// pane to display.
    fn has_blame(&self) -> bool {
        true
    }

    /// Who last touched each line of `path`, one entry per line of the new
    /// side. Only asked when `has_blame` is true.
    fn blame(&self, _repo: &str, _path: &str) -> Res<Vec<String>> {
        Ok(Vec::new())
    }
}

/// The backends that exist.
pub fn all() -> &'static [&'static dyn Vcs] {
    &[&super::git::Git]
}

/// The backend for `dir` — the first that recognises it.
pub fn of(dir: &str) -> Option<&'static dyn Vcs> {
    all().iter().find(|v| v.is_repo(dir)).copied()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;

    /// A backend that cannot do blame, which is the whole point of it.
    struct NoBlame;

    impl Vcs for NoBlame {
        fn name(&self) -> &'static str {
            "no-blame"
        }
        fn is_repo(&self, _dir: &str) -> bool {
            true
        }
        fn head_branch(&self, _repo: &str) -> Option<String> {
            None
        }
        fn base_branch(&self, _repo: &str) -> String {
            "main".into()
        }
        fn changed_files(&self, _repo: &str, _scope: &Scope) -> Res<Vec<ChangedFile>> {
            Ok(Vec::new())
        }
        fn file_diff(&self, _r: &str, _s: &Scope, _p: &str, _c: u32) -> Res<Vec<Row>> {
            Ok(Vec::new())
        }
        fn has_blame(&self) -> bool {
            false
        }
    }

    #[test]
    fn a_backend_needs_six_methods_and_no_more() {
        // `NoBlame` above is everything a second backend has to write. If
        // adding one ever needs more than this, the file stops compiling.
        let v = NoBlame;
        assert_eq!(v.name(), "no-blame");
        assert!(
            v.changed_files("/tmp", &Scope::WorkingTree)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_backend_without_blame_says_so_rather_than_returning_nobody() {
        // "nobody wrote these lines" and "I cannot tell you who did" are
        // different answers, and only one of them should cost thirty columns.
        let v = NoBlame;
        assert!(!v.has_blame());
        assert!(
            v.blame("/tmp", "a.rs").unwrap().is_empty(),
            "and the default is harmless when it is asked anyway"
        );
    }

    #[test]
    fn git_is_a_backend_and_answers_for_this_repository() {
        // The real one, against the repository the tests are running in.
        let here = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let v = of(&here).expect("this is a repository, and something should own it");
        assert_eq!(v.name(), "git");
        assert!(v.has_blame());
        assert!(!v.base_branch(&here).is_empty());
    }

    #[test]
    fn nothing_owns_somewhere_that_is_not_a_repository() {
        assert!(of("/").is_none() || of("/").is_some_and(|v| v.name() == "git"));
        assert!(of("/nonexistent-path-for-a-test").is_none());
    }
}
