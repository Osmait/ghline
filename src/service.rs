//! Worker thread: runs the `gh` calls off the render loop.
//!
//! The main loop sends a `Request` and calls `try_recv` each pass, so the
//! interface never blocks waiting on the network.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::data::MergeMethod;
use crate::data::{Account, Comment, FileChange, Hunk, Item, Job, RawLog, Repo, Review};
use crate::error::Error;
use crate::finder::Source as FinderSource;
use crate::gh;

pub enum Request {
    Accounts,
    Repos {
        login: String,
    },
    List {
        repo: String,
        tab: usize,
    },
    /// Workflow runs gathered from several repositories at once.
    ///
    /// Separate from `List` because there is no cross-repository Actions API:
    /// this really is one call per repository, so the caller passes only the
    /// ones with any workflows and the answer is filed under `key` as if it
    /// had been one list all along.
    AllRuns {
        key: String,
        repos: Vec<String>,
    },
    IssueDetail {
        repo: String,
        num: i64,
    },
    PrDetail {
        repo: String,
        num: i64,
    },
    PrDiff {
        repo: String,
        num: i64,
    },
    Search {
        owner: String,
        query: String,
        source: FinderSource,
    },
    RunJobs {
        repo: String,
        run_id: i64,
    },
    RunLog {
        repo: String,
        run_id: i64,
        finished: bool,
    },
    Merge {
        repo: String,
        num: i64,
        method: MergeMethod,
        branch: String,
    },
    Close {
        repo: String,
        num: i64,
    },
    Reopen {
        repo: String,
        num: i64,
    },
    DeleteBranch {
        repo: String,
        branch: String,
    },
}

pub enum Response {
    Accounts(Result<Vec<Account>, Error>),
    Repos {
        login: String,
        result: Result<Vec<Repo>, Error>,
    },
    List {
        repo: String,
        tab: usize,
        result: Result<Vec<Item>, Error>,
    },
    IssueDetail {
        repo: String,
        num: i64,
        result: Result<(String, Vec<Comment>), Error>,
    },
    PrDetail {
        repo: String,
        num: i64,
        result: Result<(String, Vec<FileChange>, Vec<Review>), Error>,
    },
    PrDiff {
        repo: String,
        num: i64,
        result: Result<Vec<(String, Vec<Hunk>)>, Error>,
    },
    Search {
        query: String,
        source: FinderSource,
        result: Result<Vec<gh::SearchHit>, Error>,
    },
    RunJobs {
        repo: String,
        run_id: i64,
        result: Result<Vec<Job>, Error>,
    },
    RunLog {
        repo: String,
        run_id: i64,
        result: Result<Vec<RawLog>, Error>,
    },
    /// An action finished: a status-bar message and which PR it affected.
    Action {
        repo: String,
        num: i64,
        result: Result<String, Error>,
        /// Branch to offer for deletion if the action was a successful merge.
        merged_branch: Option<String>,
    },
}

pub struct Service {
    tx: Sender<Request>,
    rx: Receiver<Response>,
}

impl Service {
    /// Starts the thread. Requests are served in arrival order.
    pub fn spawn() -> Self {
        let (tx, req_rx) = channel::<Request>();
        let (res_tx, rx) = channel::<Response>();

        thread::spawn(move || {
            while let Ok(req) = req_rx.recv() {
                let res = handle(req);
                if res_tx.send(res).is_err() {
                    break; // the interface is gone
                }
            }
        });

        Self { tx, rx }
    }

    pub fn send(&self, req: Request) {
        let _ = self.tx.send(req);
    }

    pub fn poll(&self) -> Option<Response> {
        self.rx.try_recv().ok()
    }
}

fn handle(req: Request) -> Response {
    match req {
        Request::Accounts => Response::Accounts(gh::accounts()),

        Request::Repos { login } => {
            let result = gh::repos(&login);
            Response::Repos { login, result }
        }

        Request::List { repo, tab } => {
            // `owner/*` is the pseudo-repository that gathers all of them. It
            // is answered here rather than by the caller so the whole thing
            // stays one request on one thread, skeleton and all.
            let all = repo.strip_suffix('*').and_then(|o| o.strip_suffix('/'));
            let result = match (all, tab) {
                (Some(owner), 0) => gh::all_issues(owner),
                (Some(owner), 1) => gh::all_prs(owner),
                // Runs never reach here: there is no cross-repository
                // Actions API, so they travel as their own request carrying
                // the repositories worth asking.
                (Some(_), _) => Ok(Vec::new()),
                (None, 0) => gh::issues(&repo),
                (None, 1) => gh::prs(&repo),
                (None, _) => gh::runs(&repo),
            };
            Response::List { repo, tab, result }
        }

        Request::AllRuns { key, repos } => Response::List {
            repo: key,
            tab: 2,
            result: gh::all_runs(&repos),
        },

        Request::IssueDetail { repo, num } => {
            let result = gh::issue_detail(&repo, num);
            Response::IssueDetail { repo, num, result }
        }

        Request::PrDetail { repo, num } => {
            let result = gh::pr_detail(&repo, num);
            Response::PrDetail { repo, num, result }
        }

        Request::PrDiff { repo, num } => {
            let result = gh::pr_diff(&repo, num);
            Response::PrDiff { repo, num, result }
        }

        Request::Search {
            owner,
            query,
            source,
        } => {
            let result = match source {
                FinderSource::Commits => gh::search_commits(&owner, &query),
                FinderSource::Prs => gh::search_issues(&owner, &query, true),
                _ => gh::search_issues(&owner, &query, false),
            };
            Response::Search {
                query,
                source,
                result,
            }
        }

        Request::RunJobs { repo, run_id } => {
            let result = gh::run_jobs(&repo, run_id);
            Response::RunJobs {
                repo,
                run_id,
                result,
            }
        }

        Request::RunLog {
            repo,
            run_id,
            finished,
        } => {
            let result = gh::run_log(&repo, run_id, finished);
            Response::RunLog {
                repo,
                run_id,
                result,
            }
        }

        Request::Merge {
            repo,
            num,
            method,
            branch,
        } => {
            let result = gh::merge(&repo, num, method.short())
                .map(|_| format!("#{num} merged via {}", method.short()));
            let merged_branch = result.is_ok().then_some(branch).filter(|b| !b.is_empty());
            Response::Action {
                repo,
                num,
                result,
                merged_branch,
            }
        }

        Request::Close { repo, num } => {
            let result = gh::close(&repo, num).map(|_| format!("#{num} closed"));
            Response::Action {
                repo,
                num,
                result,
                merged_branch: None,
            }
        }

        Request::Reopen { repo, num } => {
            let result = gh::reopen(&repo, num).map(|_| format!("#{num} reopened"));
            Response::Action {
                repo,
                num,
                result,
                merged_branch: None,
            }
        }

        Request::DeleteBranch { repo, branch } => {
            let result =
                gh::delete_branch(&repo, &branch).map(|_| format!("deleted branch {branch}"));
            Response::Action {
                repo,
                num: 0,
                result,
                merged_branch: None,
            }
        }
    }
}
