//! Pull request actions: merge, close, reopen and delete the branch.
//!
//! This layer is deliberately separate from the UI and the reducer: `App::apply`
//! is the single place that mutates state, so swapping the demo data for real
//! GitHub calls does not require touching the render.

use crate::app::App;
use crate::data::{Kind, MERGE_METHODS};
use crate::service::Request;

/// A pending confirmation. While one is up, every other key is ignored.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Prompt {
    /// Index into `MERGE_METHODS`.
    Merge(usize),
    Close,
    Reopen,
    /// Carries the branch and number explicitly: in live mode this prompt
    /// appears after the merge, once the list has already been reloaded, so it
    /// cannot depend on where the selection happens to be by then.
    DeleteBranch {
        num: i64,
        branch: String,
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
            && self.current().map(|c| c.kind == Kind::Pr).unwrap_or(false)
    }

    /// `m`: opens the merge confirmation if the PR allows it.
    pub fn ask_merge(&mut self) {
        let Some(cur) = self.current() else { return };
        match cur.state.as_str() {
            "open" => self.prompt = Some(Prompt::Merge(0)),
            "draft" => self.flash_warn("draft pull requests can't be merged — mark it ready first"),
            "merged" => self.flash_warn("this pull request is already merged"),
            _ => self.flash_warn("closed pull requests can't be merged — reopen it first"),
        }
    }

    /// `c`: closes the PR, or reopens it if it was already closed.
    pub fn ask_close(&mut self) {
        let Some(cur) = self.current() else { return };
        match cur.state.as_str() {
            "open" | "draft" => self.prompt = Some(Prompt::Close),
            "closed" => self.prompt = Some(Prompt::Reopen),
            _ => self.flash_warn("a merged pull request can't be closed"),
        }
    }

    /// `D`: deletes the branch, only once the PR is resolved.
    pub fn ask_delete_branch(&mut self) {
        let Some(cur) = self.current() else { return };
        if cur.branch_deleted {
            let branch = cur.branch.clone();
            self.flash_warn(format!("branch {branch} is already deleted"));
            return;
        }
        let (num, branch) = (cur.num, cur.branch.clone());
        match cur.state.as_str() {
            "merged" | "closed" => self.prompt = Some(Prompt::DeleteBranch { num, branch }),
            _ => self.flash_warn("delete the branch after merging or closing the pull request"),
        }
    }

    /// Runs the pending confirmation: against `gh` in live mode, or against the
    /// in-memory data in demo mode.
    pub fn confirm(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        // deleting a branch carries its own data; everything else acts on the
        // PR selected right now
        let (num, branch) = match &prompt {
            Prompt::DeleteBranch { num, branch } => (*num, branch.clone()),
            _ => match self.current() {
                Some(cur) => (cur.num, cur.branch.clone()),
                None => return,
            },
        };

        if self.live() {
            self.dispatch(&prompt, num, branch);
        } else {
            self.apply_local(&prompt, num, branch);
        }
    }

    /// Live mode: the action goes to the service thread and the list is
    /// refreshed from what GitHub reports, not from a local guess.
    fn dispatch(&mut self, prompt: &Prompt, num: i64, branch: String) {
        let repo = self.repo_key();
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
        }
    }

    /// Demo mode: the in-memory copy is mutated.
    fn apply_local(&mut self, prompt: &Prompt, num: i64, branch: String) {
        let Some(idx) = self.current_index() else {
            return;
        };
        let key = (self.repo_key(), self.tab);
        let Some(item) = self.lists.get_mut(&key).and_then(|v| v.get_mut(idx)) else {
            return;
        };

        match prompt {
            Prompt::Merge(m) => {
                let method = MERGE_METHODS[*m];
                item.state = "merged".into();
                item.merged_with = Some(method.short().into());
                self.bump_open_prs(-1);
                self.flash_ok(format!("#{num} merged into main via {}", method.short()));
                if !branch.is_empty() {
                    self.prompt = Some(Prompt::DeleteBranch { num, branch });
                }
            }
            Prompt::Close => {
                item.state = "closed".into();
                self.bump_open_prs(-1);
                self.flash_ok(format!("#{num} closed"));
            }
            Prompt::Reopen => {
                item.state = "open".into();
                self.bump_open_prs(1);
                self.flash_ok(format!("#{num} reopened"));
            }
            Prompt::DeleteBranch { .. } => {
                item.branch_deleted = true;
                self.flash_ok(format!("deleted branch {branch}"));
            }
        }
    }

    pub fn cancel_prompt(&mut self) {
        if self.prompt.take().is_some() {
            self.flash_warn("cancelled");
        }
    }

    fn send(&self, req: Request) {
        if let Some(svc) = &self.service {
            svc.send(req);
        }
    }

    /// Keeps the active repo's open-PR count consistent (demo only).
    fn bump_open_prs(&mut self, delta: i32) {
        let repo = self.repo_idx();
        let Some(r) = self
            .accounts
            .get_mut(self.acc)
            .and_then(|a| a.repos.get_mut(repo))
        else {
            return;
        };
        r.prs = (r.prs as i32 + delta).max(0) as u32;
    }
}
