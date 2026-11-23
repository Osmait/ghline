//! Pull request actions: merge, close, reopen and delete the branch.
//!
//! This layer is deliberately separate from the UI and the reducer: `App::apply`
//! is the single place that mutates state, so what a confirmation does can
//! change without touching the render.

use crate::app::App;
use crate::data::{Kind, MERGE_METHODS, Status};
use crate::service::Request;

/// A pending confirmation. While one is up, every other key is ignored.
///
/// Every variant pins its target repository and pull request when opened so a
/// background reload or selection shift before confirmation can never mutate a
/// different repository or PR.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Prompt {
    /// Merge a pull request with `MERGE_METHODS[method]`.
    Merge {
        repo: String,
        num: i64,
        branch: String,
        method: usize,
    },
    Close {
        repo: String,
        num: i64,
    },
    Reopen {
        repo: String,
        num: i64,
    },
    /// Carries the repository, branch and number explicitly: this prompt
    /// appears after the merge, once the list has already been reloaded, so it
    /// cannot depend on where the selection happens to be by then.
    DeleteBranch {
        repo: String,
        num: i64,
        branch: String,
    },
    /// Fetch a repository so a file in it can be opened.
    Clone {
        repo: String,
        dest: String,
    },
    /// Hand an issue to a coding agent. Unlike the others this is not about a
    /// pull request at all, and it carries everything it needs: by the time it
    /// is confirmed the selection may have been asked to move.
    Dispatch {
        /// Shown in the dialog: `claude in code/parser/error-check`.
        who: String,
        /// The herdr pane to address.
        pane: String,
        /// The rendered prompt, already filled in.
        text: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlashKind {
    Ok,
    Warn,
}

#[derive(Clone, Debug)]
pub struct Flash {
    pub text: String,
    pub kind: FlashKind,
    /// Heartbeats it has left to live.
    pub ttl: u8,
}

impl App {
    pub fn flash_ok(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            kind: FlashKind::Ok,
            ttl: 3,
        });
    }

    pub fn flash_warn(&mut self, text: impl Into<String>) {
        self.flash = Some(Flash {
            text: text.into(),
            kind: FlashKind::Warn,
            ttl: 3,
        });
    }

    /// Is there a selected PR to act on?
    pub fn actionable_pr(&self) -> bool {
        self.view != crate::app::View::Logs
            && self
                .current()
                .map(|c| c.kind() == Kind::Pr)
                .unwrap_or(false)
    }

    /// `m`: opens the merge confirmation if the PR allows it.
    pub fn ask_merge(&mut self) {
        let Some(cur) = self.current() else { return };
        let (state, num, branch) = (cur.state, cur.num, cur.branch().to_string());
        let repo = self.item_repo_key();
        match state {
            Status::Open => {
                self.prompt = Some(Prompt::Merge {
                    repo,
                    num,
                    branch,
                    method: 0,
                });
            }
            Status::Draft => {
                self.flash_warn("draft pull requests can't be merged — mark it ready first");
            }
            Status::Merged => self.flash_warn("this pull request is already merged"),
            _ => self.flash_warn("closed pull requests can't be merged — reopen it first"),
        }
    }

    /// `c`: closes the PR, or reopens it if it was already closed.
    pub fn ask_close(&mut self) {
        let Some(cur) = self.current() else { return };
        let (state, num) = (cur.state, cur.num);
        let repo = self.item_repo_key();
        if state.is_open() {
            self.prompt = Some(Prompt::Close { repo, num });
        } else if state == Status::Closed {
            self.prompt = Some(Prompt::Reopen { repo, num });
        } else {
            self.flash_warn("a merged pull request can't be closed");
        }
    }

    /// `D`: deletes the branch, only once the PR is resolved.
    pub fn ask_delete_branch(&mut self) {
        let Some(cur) = self.current() else { return };
        if cur.as_pr().is_some_and(|p| p.branch_deleted) {
            let branch = cur.branch().to_string();
            self.flash_warn(format!("branch {branch} is already deleted"));
            return;
        }
        let (state, num, branch) = (cur.state, cur.num, cur.branch().to_string());
        let repo = self.item_repo_key();
        if branch.is_empty() {
            self.flash_warn("no branch name to delete");
            return;
        }
        match state {
            Status::Merged | Status::Closed => {
                self.prompt = Some(Prompt::DeleteBranch { repo, num, branch });
            }
            _ => self.flash_warn("delete the branch after merging or closing the pull request"),
        }
    }

    /// Runs the pending confirmation against `gh`.
    pub fn confirm(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        match prompt {
            Prompt::Clone { repo, dest } => {
                self.busy = true;
                self.flash_ok(format!("cloning {repo} into {dest}…"));
                self.ask(Request::Clone { repo, dest });
            }
            // dispatching carries everything it needs and has no pull request
            // behind it
            Prompt::Dispatch { who, pane, text } => {
                self.busy = true;
                self.flash_ok(format!("sending to {who}…"));
                // A fresh worktree needs three calls chained; an agent that is
                // already there needs one. Which it is was decided in the picker.
                match self.pending_fresh.take() {
                    Some(f) => self.ask(Request::DispatchFresh {
                        repo_root: f.repo_root,
                        branch: f.branch,
                        label: f.label,
                        kind: f.kind,
                        text,
                    }),
                    None => self.ask(Request::Dispatch { pane, text }),
                }
            }
            Prompt::Merge {
                repo,
                num,
                branch,
                method,
            } => {
                let method = MERGE_METHODS
                    .get(method)
                    .copied()
                    .unwrap_or(MERGE_METHODS[0]);
                self.busy = true;
                self.flash_ok(format!("merging #{num} via {}…", method.short()));
                // the branch is offered when the response arrives, never before:
                // if the merge fails, nothing should be offered for deletion
                self.ask(Request::Merge {
                    repo,
                    num,
                    method,
                    branch,
                });
            }
            Prompt::Close { repo, num } => {
                self.busy = true;
                self.flash_ok(format!("closing #{num}…"));
                self.ask(Request::Close { repo, num });
            }
            Prompt::Reopen { repo, num } => {
                self.busy = true;
                self.flash_ok(format!("reopening #{num}…"));
                self.ask(Request::Reopen { repo, num });
            }
            Prompt::DeleteBranch { repo, branch, .. } => {
                if branch.is_empty() {
                    self.flash_warn("no branch name to delete");
                    return;
                }
                self.busy = true;
                self.flash_ok(format!("deleting {branch}…"));
                self.ask(Request::DeleteBranch { repo, branch });
            }
        }
    }

    pub fn cancel_prompt(&mut self) {
        // a plan for a worktree only outlives the question it was asked with
        self.pending_fresh = None;
        self.pending_clone_edit = None;
        if self.prompt.take().is_some() {
            self.flash_warn("cancelled");
        }
    }
}
