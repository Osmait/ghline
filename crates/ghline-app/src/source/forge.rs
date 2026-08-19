//! Where the issues, pull requests and runs come from.
//!
//! `gh` is the only one there is, and the worker called it by name. The name
//! is the dependency: every question this program asks a forge is asked
//! through a module that can only be the GitHub CLI, so a second forge —
//! GitLab, a Gitea somewhere, the REST API without the CLI in front of it —
//! would be a rewrite of the layer above rather than a second file here.
//!
//! Twenty-three methods is a large trait, and it is large because this
//! program asks twenty-three questions. Narrowing it would mean asking
//! fewer, which is a different change.

use crate::data::SearchHit;
use crate::data::{
    Account, Comment, FileChange, Hunk, Item, Job, Kind, RawLog, Repo, Review, TreeEntry,
};
use crate::gh;
use crate::shared::error::Result as Res;

/// Somewhere issues and pull requests live.
pub trait Forge: Send + Sync + 'static {
    /// What to call it, for a message that has to name it.
    fn name(&self) -> &'static str;

    fn accounts(&self) -> Res<Vec<Account>>;

    fn repos(&self, login: &str) -> Res<Vec<Repo>>;

    fn issues(&self, repo: &str) -> Res<Vec<Item>>;

    fn prs(&self, repo: &str) -> Res<Vec<Item>>;

    fn runs(&self, repo: &str) -> Res<Vec<Item>>;

    fn pr_detail(&self, repo: &str, num: i64) -> Res<(String, Vec<FileChange>, Vec<Review>)>;

    fn issue_detail(&self, repo: &str, num: i64) -> Res<(String, Vec<Comment>)>;

    fn run_jobs(&self, repo: &str, run_id: i64) -> Res<Vec<Job>>;

    fn run_log(&self, repo: &str, run_id: i64, finished: bool) -> Res<Vec<RawLog>>;

    fn pr_diff(&self, repo: &str, num: i64) -> Res<Vec<(String, Vec<Hunk>)>>;

    fn search_issues(&self, owner: &str, query: &str, want: Kind) -> Res<Vec<SearchHit>>;

    fn search_commits(&self, owner: &str, query: &str) -> Res<Vec<SearchHit>>;

    fn merge(&self, repo: &str, num: i64, method: &str) -> Res<()>;

    fn close(&self, repo: &str, num: i64) -> Res<()>;

    fn reopen(&self, repo: &str, num: i64) -> Res<()>;

    fn delete_branch(&self, repo: &str, branch: &str) -> Res<()>;

    fn all_issues(&self, owner: &str) -> Res<Vec<Item>>;

    fn all_prs(&self, owner: &str) -> Res<Vec<Item>>;

    fn all_runs(&self, repos: &[String]) -> Res<Vec<Item>>;

    fn clone(&self, repo: &str, dest: &str) -> Res<String>;

    fn repo_tree(&self, repo: &str) -> Res<Vec<TreeEntry>>;

    fn file_content(&self, repo: &str, path: &str) -> Res<String>;
}

/// The GitHub CLI, which is what all of this was written against.
///
/// A unit struct: a call is a process, and every bit of state lives in `gh`'s
/// own configuration rather than here.
pub struct Cli;

impl Forge for Cli {
    fn name(&self) -> &'static str {
        "gh"
    }

    fn accounts(&self) -> Res<Vec<Account>> {
        gh::accounts()
    }

    fn repos(&self, login: &str) -> Res<Vec<Repo>> {
        gh::repos(login)
    }

    fn issues(&self, repo: &str) -> Res<Vec<Item>> {
        gh::issues(repo)
    }

    fn prs(&self, repo: &str) -> Res<Vec<Item>> {
        gh::prs(repo)
    }

    fn runs(&self, repo: &str) -> Res<Vec<Item>> {
        gh::runs(repo)
    }

    fn pr_detail(&self, repo: &str, num: i64) -> Res<(String, Vec<FileChange>, Vec<Review>)> {
        gh::pr_detail(repo, num)
    }

    fn issue_detail(&self, repo: &str, num: i64) -> Res<(String, Vec<Comment>)> {
        gh::issue_detail(repo, num)
    }

    fn run_jobs(&self, repo: &str, run_id: i64) -> Res<Vec<Job>> {
        gh::run_jobs(repo, run_id)
    }

    fn run_log(&self, repo: &str, run_id: i64, finished: bool) -> Res<Vec<RawLog>> {
        gh::run_log(repo, run_id, finished)
    }

    fn pr_diff(&self, repo: &str, num: i64) -> Res<Vec<(String, Vec<Hunk>)>> {
        gh::pr_diff(repo, num)
    }

    fn search_issues(&self, owner: &str, query: &str, want: Kind) -> Res<Vec<SearchHit>> {
        gh::search_issues(owner, query, want)
    }

    fn search_commits(&self, owner: &str, query: &str) -> Res<Vec<SearchHit>> {
        gh::search_commits(owner, query)
    }

    fn merge(&self, repo: &str, num: i64, method: &str) -> Res<()> {
        gh::merge(repo, num, method)
    }

    fn close(&self, repo: &str, num: i64) -> Res<()> {
        gh::close(repo, num)
    }

    fn reopen(&self, repo: &str, num: i64) -> Res<()> {
        gh::reopen(repo, num)
    }

    fn delete_branch(&self, repo: &str, branch: &str) -> Res<()> {
        gh::delete_branch(repo, branch)
    }

    fn all_issues(&self, owner: &str) -> Res<Vec<Item>> {
        gh::all_issues(owner)
    }

    fn all_prs(&self, owner: &str) -> Res<Vec<Item>> {
        gh::all_prs(owner)
    }

    fn all_runs(&self, repos: &[String]) -> Res<Vec<Item>> {
        gh::all_runs(repos)
    }

    fn clone(&self, repo: &str, dest: &str) -> Res<String> {
        gh::clone(repo, dest)
    }

    fn repo_tree(&self, repo: &str) -> Res<Vec<TreeEntry>> {
        gh::repo_tree(repo)
    }

    fn file_content(&self, repo: &str, path: &str) -> Res<String> {
        gh::file_content(repo, path)
    }
}

static CHOSEN: std::sync::OnceLock<Box<dyn Forge>> = std::sync::OnceLock::new();

/// The forge in use.
pub fn current() -> &'static dyn Forge {
    CHOSEN.get_or_init(|| Box::new(Cli)).as_ref()
}
