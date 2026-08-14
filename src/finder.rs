//! The finder: one modal over four sources.
//!
//! Two of them behave quite differently and the difference is deliberate.
//! Repositories are already in memory, so they filter as you type with no
//! latency at all. Issues, pull requests and commits live on GitHub, so the
//! query goes to `gh search` once you stop typing — and what comes back is
//! shown as it is, since the server already ranked it.

use crate::data::Status;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Repos,
    Issues,
    Prs,
    Commits,
}

impl Source {
    pub const ALL: [Self; 4] = [Self::Repos, Self::Issues, Self::Prs, Self::Commits];

    pub fn label(self) -> &'static str {
        match self {
            Self::Repos => "repos",
            Self::Issues => "issues",
            Self::Prs => "pull requests",
            Self::Commits => "commits",
        }
    }

    /// Repositories are filtered here; the rest are searched on GitHub.
    pub fn is_local(self) -> bool {
        self == Self::Repos
    }

    /// GitHub refuses a commit search with no text — qualifiers alone are not
    /// allowed — so that source has nothing to show until something is typed.
    pub fn needs_query(self) -> bool {
        self == Self::Commits
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Repos => "filter repositories",
            Self::Issues => "search issues in your repositories",
            Self::Prs => "search pull requests in your repositories",
            Self::Commits => "type to search commits — GitHub needs the text",
        }
    }
}

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
