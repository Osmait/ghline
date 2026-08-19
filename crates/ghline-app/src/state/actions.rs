//! Pull request actions: merge, close, reopen and delete the branch.
//!
//! This layer is deliberately separate from the UI and the reducer: `App::apply`
//! is the single place that mutates state, so what a confirmation does can
//! change without touching the render.

use crate::app::App;
use crate::data::{Kind, MERGE_METHODS, Status};
use crate::service::Request;

/// A pending confirmation. While one is up, every other key is ignored.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Prompt {
    /// Index into `MERGE_METHODS`.
    Merge(usize),
    Close,
    Reopen,
    /// Carries the branch and number explicitly: this prompt appears after
    /// the merge, once the list has already been reloaded, so it cannot
    /// depend on where the selection happens to be by then.
    DeleteBranch {
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
        /// Shown in the dialog: `claude in orca/sbql/error-check`.
        who: String,
        /// The herdr pane to address.
        pane: String,
        /// The rendered prompt, already filled in.
        text: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlashKind {
    Ok,
    Warn,
}

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
        match cur.state {
            Status::Open => self.prompt = Some(Prompt::Merge(0)),
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
        if cur.state.is_open() {
            self.prompt = Some(Prompt::Close);
        } else if cur.state == Status::Closed {
            self.prompt = Some(Prompt::Reopen);
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
        let (num, branch) = (cur.num, cur.branch().to_string());
        match cur.state {
            Status::Merged | Status::Closed => {
                self.prompt = Some(Prompt::DeleteBranch { num, branch });
            }
            _ => self.flash_warn("delete the branch after merging or closing the pull request"),
        }
    }

    /// Runs the pending confirmation against `gh`.
    pub fn confirm(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        if let Prompt::Clone { repo, dest } = prompt {
            self.busy = true;
            self.flash_ok(format!("cloning {repo} into {dest}…"));
            self.send(Request::Clone { repo, dest });
            return;
        }

        // dispatching carries everything it needs and has no pull request
        // behind it, so it never reaches the extraction below
        if let Prompt::Dispatch { who, pane, text } = prompt {
            self.busy = true;
            self.flash_ok(format!("sending to {who}…"));
            // A fresh worktree needs three calls chained; an agent that is
            // already there needs one. Which it is was decided in the picker.
            match self.pending_fresh.take() {
                Some(f) => self.send(Request::DispatchFresh {
                    repo_root: f.repo_root,
                    branch: f.branch,
                    label: f.label,
                    kind: f.kind,
                    text,
                }),
                None => self.send(Request::Dispatch { pane, text }),
            }
            return;
        }

        // deleting a branch carries its own data; everything else acts on the
        // PR selected right now
        let (num, branch) = match &prompt {
            Prompt::DeleteBranch { num, branch } => (*num, branch.clone()),
            _ => match self.current() {
                Some(cur) => (cur.num, cur.branch().to_string()),
                None => return,
            },
        };

        self.dispatch(&prompt, num, branch);
    }

    /// The action goes to the service thread and the list is refreshed from
    /// what GitHub reports, not from a local guess.
    fn dispatch(&mut self, prompt: &Prompt, num: i64, branch: String) {
        let repo = self.item_repo_key();
        self.busy = true;
        match prompt {
            Prompt::Merge(m) => {
                let method = MERGE_METHODS[*m];
                self.flash_ok(format!("merging #{num} via {}…", method.short()));
                // the branch is offered when the response arrives, never before:
                // if the merge fails, nothing should be offered for deletion
                self.send(Request::Merge {
                    repo,
                    num,
                    method,
                    branch,
                });
            }
            Prompt::Close => {
                self.flash_ok(format!("closing #{num}…"));
                self.send(Request::Close { repo, num });
            }
            Prompt::Reopen => {
                self.flash_ok(format!("reopening #{num}…"));
                self.send(Request::Reopen { repo, num });
            }
            Prompt::DeleteBranch { .. } => {
                self.flash_ok(format!("deleting {branch}…"));
                self.send(Request::DeleteBranch { repo, branch });
            }
            // both handled before the pull request data is gathered
            Prompt::Dispatch { .. } | Prompt::Clone { .. } => {}
        }
    }

    pub fn cancel_prompt(&mut self) {
        // a plan for a worktree only outlives the question it was asked with
        self.pending_fresh = None;
        if self.prompt.take().is_some() {
            self.flash_warn("cancelled");
        }
    }

    fn send(&self, req: Request) {
        if let Some(svc) = &self.service {
            svc.send(req);
        }
    }
}
