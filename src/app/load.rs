//! Talking to the service thread: what the current view still needs, and what
//! to do with each response. This is the only place that turns a `Response`
//! into state.

use super::{App, Load, Prompt, View};
use crate::data::{Item, Kind};
use crate::service::{Request, Response};

impl App {
    fn ask(&mut self, req: Request) {
        if let Some(svc) = &self.service {
            svc.send(req);
        }
    }

    /// Requests whatever the current view still needs. Idempotent: each piece
    /// is marked `Loading` before being asked for, so nothing is duplicated.
    pub fn ensure(&mut self) {
        if !self.live() {
            return;
        }

        if self.accounts_state == Load::Idle {
            self.accounts_state = Load::Loading;
            self.ask(Request::Accounts);
            return;
        }

        let Some(login) = self.account().map(|a| a.login.clone()) else {
            return;
        };
        if self.repos_state.get(&login).unwrap_or(&Load::Idle) == &Load::Idle {
            self.repos_state.insert(login.clone(), Load::Loading);
            self.ask(Request::Repos { login });
            return;
        }
        if self.repo().is_none() {
            return;
        }

        let key = self.repo_key();
        let tab = self.tab;
        if self
            .lists_state
            .get(&(key.clone(), tab))
            .unwrap_or(&Load::Idle)
            == &Load::Idle
        {
            self.lists_state.insert((key.clone(), tab), Load::Loading);
            self.ask(Request::List { repo: key, tab });
            return;
        }

        if self.view == View::List {
            return;
        }

        // the diff is fetched whole: `gh pr diff` returns it in one go
        if self.view == View::Diff {
            if let Some(cur) = self.current() {
                let (num, kind) = (cur.num, cur.kind());
                let idle = self
                    .diff_state
                    .get(&(key.clone(), num))
                    .map(|s| *s == Load::Idle)
                    .unwrap_or(true);
                if kind == Kind::Pr && idle {
                    self.diff_state.insert((key.clone(), num), Load::Loading);
                    self.ask(Request::PrDiff { repo: key, num });
                }
            }
            return;
        }

        // detail: the selection's body, files and reviews
        let Some(cur) = self.current() else { return };
        let (kind, num, id, loaded) = (cur.kind(), cur.num, cur.id, cur.detail_loaded);
        if !loaded && kind != Kind::Run {
            if let Some(item) = self.current_item_mut() {
                item.detail_loaded = true; // avoids repeating the request
            }
            let repo = key.clone();
            match kind {
                Kind::Issue => self.ask(Request::IssueDetail { repo, num }),
                _ => self.ask(Request::PrDetail { repo, num }),
            }
        }

        if id == 0 {
            return;
        }
        if self
            .jobs_state
            .get(&(key.clone(), id))
            .unwrap_or(&Load::Idle)
            == &Load::Idle
        {
            self.jobs_state.insert((key.clone(), id), Load::Loading);
            self.ask(Request::RunJobs {
                repo: key.clone(),
                run_id: id,
            });
        }
        if self.view == View::Logs
            && self
                .logs_state
                .get(&(key.clone(), id))
                .unwrap_or(&Load::Idle)
                == &Load::Idle
        {
            self.logs_state.insert((key.clone(), id), Load::Loading);
            let finished = self
                .current()
                .map(|c| c.state.is_settled() && c.checks().is_settled())
                .unwrap_or(true);
            self.ask(Request::RunLog {
                repo: key,
                run_id: id,
                finished,
            });
        }
    }

    /// `r`: drops the active repo's caches so `ensure` asks for them again.
    pub fn refresh(&mut self) {
        if !self.live() {
            return;
        }
        let key = self.repo_key();
        for t in 0..3 {
            self.lists_state.insert((key.clone(), t), Load::Idle);
        }
        let id = self.run_id();
        if id > 0 {
            self.jobs_state.insert((key.clone(), id), Load::Idle);
            self.logs_state.insert((key.clone(), id), Load::Idle);
        }
        self.repos_state
            .insert(self.login().to_string(), Load::Idle);
        if let Some(cur) = self.current() {
            let num = cur.num;
            self.diff_state.insert((key.clone(), num), Load::Idle);
        }
        self.flash_ok("refreshing…");
    }

    pub fn current_item_mut(&mut self) -> Option<&mut Item> {
        let idx = self.current_index()?;
        let key = (self.repo_key(), self.tab);
        self.lists.get_mut(&key)?.get_mut(idx)
    }

    /// A status-bar message, suggesting a retry when the failure looks
    /// temporary.
    fn advice(&self, e: &crate::error::Error) -> String {
        if e.is_transient() {
            format!("{} · press r to retry", e.brief())
        } else {
            e.brief()
        }
    }

    /// Applies one response from the service thread.
    pub fn apply(&mut self, res: Response) {
        match res {
            Response::Accounts(Ok(accounts)) => {
                self.accounts = accounts;
                self.accounts_state = Load::Ready;
            }
            Response::Accounts(Err(e)) => {
                self.flash_warn(self.advice(&e));
                self.accounts_state = Load::Failed(e.brief());
            }

            Response::Repos { login, result } => match result {
                Ok(repos) => {
                    if let Some(a) = self.accounts.iter_mut().find(|a| a.login == login) {
                        a.repos = repos;
                    }
                    self.repos_state.insert(login, Load::Ready);
                }
                Err(e) => {
                    self.flash_warn(self.advice(&e));
                    self.repos_state.insert(login, Load::Failed(e.brief()));
                }
            },

            Response::List { repo, tab, result } => match result {
                Ok(items) => {
                    self.lists.insert((repo.clone(), tab), items);
                    self.lists_state.insert((repo, tab), Load::Ready);
                }
                Err(e) => {
                    self.flash_warn(self.advice(&e));
                    self.lists_state
                        .insert((repo, tab), Load::Failed(e.brief()));
                }
            },

            Response::IssueDetail { repo, num, result } => {
                if let Ok((body, comments)) = result
                    && let Some(items) = self.lists.get_mut(&(repo, 0))
                    && let Some(it) = items.iter_mut().find(|i| i.num == num)
                {
                    it.body = body;
                    if let Some(d) = it.as_issue_mut() {
                        d.comments = comments.len() as u32;
                        d.comment_list = comments;
                    }
                }
            }

            Response::PrDetail { repo, num, result } => {
                if let Ok((body, files, reviews)) = result
                    && let Some(items) = self.lists.get_mut(&(repo, 1))
                    && let Some(it) = items.iter_mut().find(|i| i.num == num)
                {
                    it.body = body;
                    if let Some(pr) = it.as_pr_mut() {
                        pr.file_list = files;
                        pr.reviews = reviews;
                    }
                }
            }

            Response::PrDiff { repo, num, result } => match result {
                Ok(files) => {
                    self.diff_state.insert((repo.clone(), num), Load::Ready);
                    // match the hunks against the already-loaded file list
                    if let Some(items) = self.lists.get_mut(&(repo, 1))
                        && let Some(it) = items.iter_mut().find(|i| i.num == num)
                    {
                        for f in it.as_pr_mut().into_iter().flat_map(|p| &mut p.file_list) {
                            if let Some((_, hunks)) = files.iter().find(|(p, _)| *p == f.path) {
                                f.hunks = hunks.clone();
                            }
                        }
                    }
                }
                Err(e) => {
                    self.diff_state.insert((repo, num), Load::Failed(e.brief()));
                    self.flash_warn(e.brief());
                }
            },

            Response::RunJobs {
                repo,
                run_id,
                result,
            } => match result {
                Ok(jobs) => {
                    self.jobs_by_run.insert((repo.clone(), run_id), jobs);
                    self.jobs_state.insert((repo, run_id), Load::Ready);
                }
                Err(e) => {
                    self.jobs_state
                        .insert((repo, run_id), Load::Failed(e.brief()));
                }
            },

            Response::RunLog {
                repo,
                run_id,
                result,
            } => match result {
                Ok(raw) => {
                    self.raw_logs.insert((repo.clone(), run_id), raw);
                    self.logs_state.insert((repo, run_id), Load::Ready);
                }
                Err(e) => {
                    self.logs_state
                        .insert((repo, run_id), Load::Failed(e.brief()));
                }
            },

            Response::Action {
                repo,
                num,
                result,
                merged_branch,
            } => {
                self.busy = false;
                match result {
                    Ok(msg) => {
                        self.flash_ok(msg);
                        // re-request the repo's lists and counters
                        for t in 0..3 {
                            self.lists_state.insert((repo.clone(), t), Load::Idle);
                        }
                        if let Some(a) = self
                            .accounts
                            .iter()
                            .position(|a| repo.starts_with(&format!("{}/", a.login)))
                        {
                            let login = self.accounts[a].login.clone();
                            self.repos_state.insert(login, Load::Idle);
                        }
                        // only a successful merge offers to delete the branch
                        if let Some(branch) = merged_branch {
                            self.prompt = Some(Prompt::DeleteBranch { num, branch });
                        }
                    }
                    Err(e) => self.flash_warn(e.brief()),
                }
            }
        }
    }
}
