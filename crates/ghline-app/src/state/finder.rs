//! The finder: one modal over four sources.
//!
//! Two of them behave quite differently and the difference is deliberate.
//! Repositories are already in memory, so they filter as you type with no
//! latency at all. Issues, pull requests and commits live on GitHub, so the
//! query goes to `gh search` once you stop typing — and what comes back is
//! shown as it is, since the server already ranked it.

use crate::data::Status;

/// One row of results.
#[derive(Clone)]
pub struct Hit {
    /// What the row is called: a repository name, an issue title, a commit
    /// subject.
    pub label: String,
    /// Where it lives, shown underneath.
    pub detail: String,
    /// `owner/repo`, so opening it knows where to go.
    pub repo: String,
    pub num: i64,
    pub state: Status,
    pub kind: HitKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitKind {
    Repo,
    Issue,
    Pr,
    Commit,
}
