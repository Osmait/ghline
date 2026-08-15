//! Talking to the service thread: what the current view still needs, and what
//! to do with each response. This is the only place that turns a `Response`
//! into state.

use super::{App, Load, Prompt, View};
use crate::github::data::{Item, Kind};
use crate::github::service::{Request, Response};

impl App {
    /// The row an answer about `repo` / `num` belongs to.
    ///
    /// A detail is asked for under the item's own repository, but the list
    /// holding that item may be the gathered one, filed under `owner/*`. Both
    /// are checked, or a body arriving in an all-repositories list would land
    /// nowhere and the pane would keep saying there is nothing to read.
    ///
    /// The repository is matched as well as the number because a gathered list
    /// really does hold two different `#14`s, and writing one's body onto the
    /// other would be quiet and wrong.
    fn item_mut(&mut self, repo: &str, tab: usize, num: i64) -> Option<&mut Item> {
        let gathered = repo.split_once('/').map(|(owner, _)| format!("{owner}/*"));
        let key = [Some(repo.to_string()), gathered]
            .into_iter()
            .flatten()
            .find(|k| {
                self.lists
                    .get(&(k.clone(), tab))
                    .is_some_and(|items| items.iter().any(|i| is_it(i, repo, num)))
            })?;
        self.lists
            .get_mut(&(key, tab))?
            .iter_mut()
            .find(|i| is_it(i, repo, num))
    }

    /// Hands a request to the worker. `false` means it never got there — a
    /// thread that has died would otherwise leave the pane that asked marked
    /// `Loading` forever, animating a skeleton over data that is never coming.
    /// A loader that cannot finish is worse than an error: it looks like
    /// progress.
    /// Hands a request to the worker.
    ///
    /// A request that never arrives would leave whichever pane asked for it
    /// marked `Loading` forever, animating a skeleton over data that is not
    /// coming — a loader that cannot finish is worse than an error, because it
    /// looks like progress. The thread only dies when the program is being
    /// torn down, so this is a guard rather than a path anyone walks; the
    /// flag is enough to stop the animation and say why.
    fn ask(&mut self, req: Request) {
        if self.service.is_some() && !self.service.as_ref().is_some_and(|w| w.send(req)) {
            self.worker_gone = true;
            self.busy = false;
            self.flash_warn("the worker thread is gone — restart the program");
        }
    }

    /// Requests whatever the current view still needs. Idempotent: each piece
    /// is marked `Loading` before being asked for, so nothing is duplicated.
    pub fn ensure(&mut self) {
        if self.worker_gone {
            return;
        }
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

        // Agents are about this machine, not about a repository, so they are
        // answered before anything keyed by `owner/repo` gets a look. The
        // picker needs them just as much as the tab does — it is a decision
        // about which agent is free right now.
        // The disk is walked once, the first time somewhere to send an issue
        // is asked for. Doing it at startup would be work nobody asked for.
        // Asked for by anything that needs to know where a repository is —
        // the picker, and `E`. Gating it on the picker alone was a deadlock:
        // `E` reported that it was looking while nothing had been asked.
        if (self.dispatch_open || self.wants_edit) && self.clones_state == Load::Idle {
            self.clones_state = Load::Loading;
            self.ask(Request::Scan);
        }

        let wants_agents = self.tab == crate::github::data::AGENTS_TAB || self.dispatch_open;
        if wants_agents && self.agents_state == Load::Idle {
            self.agents_state = Load::Loading;
            self.ask(Request::Agents);
        }
        if self.tab == crate::github::data::AGENTS_TAB {
            return;
        }

        // The file tree is per repository, and the gathering row has no tree
        // of its own — there is no such thing as the files of everything.
        if self.tab == crate::github::data::FILES_TAB {
            if self.is_all() {
                return;
            }
            let key = self.repo_key();
            if self.trees_state.get(&key).unwrap_or(&Load::Idle) == &Load::Idle {
                self.trees_state.insert(key.clone(), Load::Loading);
                self.ask(Request::Tree { repo: key });
                return;
            }
            // and the file under the cursor, once there is one to ask for
            if let Some(path) = self.fs_selected_file() {
                let k = (key.clone(), path.clone());
                if self.file_state.get(&k).unwrap_or(&Load::Idle) == &Load::Idle {
                    self.file_state.insert(k, Load::Loading);
                    self.ask(Request::FileText { repo: key, path });
                }
            }
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
            // Runs are the one thing that cannot be gathered in a single call,
            // so the repositories worth asking travel with the request — the
            // ones that have any workflows, which the repository query already
            // told us.
            if self.is_all() && tab == 2 {
                let repos = self.workflow_repos();
                self.ask(Request::AllRuns { key, repos });
            } else {
                self.ask(Request::List { repo: key, tab });
            }
            return;
        }

        if self.view == View::List {
            return;
        }

        // From here on it is the selection that is being fetched, and in an
        // all-repositories list that lives somewhere other than `key`.
        let key = self.item_repo_key();

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

        // detail: the selection's body, files and reviews, which the list does
        // not carry
        let Some(cur) = self.current() else { return };
        let (kind, num, id) = (cur.kind(), cur.num, cur.id);
        let idle = self
            .detail_state
            .get(&(key.clone(), num))
            .is_none_or(|s| *s == Load::Idle);
        if idle && kind != Kind::Run {
            self.detail_state.insert((key.clone(), num), Load::Loading);
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

    /// Sends the finder's query, if it is a remote source and the query has
    /// changed since the last one. Called on a short beat from the main loop
    /// so that typing does not fire a request per keystroke.
    pub fn finder_tick(&mut self) {
        if !self.finder_open || self.finder_source.is_local() || !self.live() {
            return;
        }
        if self.finder_query == self.finder_sent {
            return;
        }
        if self.finder_source.needs_query() && self.finder_query.trim().is_empty() {
            self.finder_hits.clear();
            self.finder_state = Load::Ready;
            self.finder_sent.clone_from(&self.finder_query);
            return;
        }
        self.finder_sent.clone_from(&self.finder_query);
        self.finder_state = Load::Loading;
        let owner = self.login().to_string();
        let query = self.finder_query.clone();
        let source = self.finder_source;
        self.ask(Request::Search {
            owner,
            query,
            source,
        });
    }

    /// Puts every pane of the current view into its loading state, so the
    /// skeletons can be inspected without a network round trip.
    pub fn hold_loading(&mut self, frame: u64) {
        let key = self.repo_key();
        self.anim = frame;
        self.accounts_state = Load::Loading;
        self.repos_state
            .insert(self.login().to_string(), Load::Loading);
        for t in 0..3 {
            self.lists_state.insert((key.clone(), t), Load::Loading);
        }
        // only the list view wants its rows gone; the other views need the
        // selection to stay so their own panes have something to be about
        if self.view == View::List {
            for t in 0..3 {
                self.lists.remove(&(key.clone(), t));
            }
        }
        // the selection's caches hang off its own repository
        let key = self.item_repo_key();
        let id = self.run_id();
        self.jobs_state.insert((key.clone(), id), Load::Loading);
        self.logs_state.insert((key.clone(), id), Load::Loading);
        if let Some(num) = self.current().map(|c| c.num) {
            self.diff_state.insert((key.clone(), num), Load::Loading);
            self.detail_state.insert((key, num), Load::Loading);
        }
    }

    /// Agents change state while you watch them, so the tab re-asks on the
    /// heartbeat — but only while it is the tab being looked at, and never
    /// while an answer is still on its way.
    pub fn poll_agents(&mut self) {
        if self.live()
            && self.tab == crate::github::data::AGENTS_TAB
            && self.agents_state == Load::Ready
        {
            self.agents_state = Load::Idle;
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
        // the selection's caches hang off its own repository
        let key = self.item_repo_key();
        let id = self.run_id();
        if id > 0 {
            self.jobs_state.insert((key.clone(), id), Load::Idle);
            self.logs_state.insert((key.clone(), id), Load::Idle);
        }
        self.repos_state
            .insert(self.login().to_string(), Load::Idle);
        self.agents_state = Load::Idle;
        if let Some(cur) = self.current() {
            let num = cur.num;
            self.diff_state.insert((key.clone(), num), Load::Idle);
            self.detail_state.insert((key.clone(), num), Load::Idle);
        }
        self.flash_ok("refreshing…");
    }

    /// A status-bar message, suggesting a retry when the failure looks
    /// temporary.
    fn advice(&self, e: &crate::shared::error::Error) -> String {
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
                self.accounts_state = Load::from(e);
            }

            Response::Dispatched { result } => {
                self.busy = false;
                match result {
                    // The agent will take a while; the tab is where you watch
                    // it, so its next poll should see the new state.
                    Ok(()) => {
                        self.flash_ok("sent");
                        self.agents_state = Load::Idle;
                    }
                    Err(e) => self.flash_warn(self.advice(&e)),
                }
            }

            Response::Cloned { repo, result } => {
                self.busy = false;
                match result {
                    Ok(path) => {
                        self.flash_ok(format!("cloned into {path}"));
                        self.clones.insert(repo, std::path::PathBuf::from(path));
                        // the reader asked to edit; now they can
                        self.open_in_editor();
                    }
                    Err(e) => self.flash_warn(self.advice(&e)),
                }
            }

            Response::Tree { repo, result } => match result {
                Ok(entries) => {
                    self.trees.insert(repo.clone(), entries);
                    self.trees_state.insert(repo, Load::Ready);
                }
                Err(e) => {
                    self.trees_state.insert(repo, Load::from(e));
                }
            },

            Response::FileText { repo, path, result } => match result {
                Ok((text, spans)) => {
                    if !spans.is_empty() {
                        self.file_spans.insert((repo.clone(), path.clone()), spans);
                    }
                    self.file_text.insert((repo.clone(), path.clone()), text);
                    self.file_state.insert((repo, path), Load::Ready);
                }
                Err(e) => {
                    self.file_state.insert((repo, path), Load::from(e));
                }
            },

            Response::Scanned { index, branches } => {
                self.clones = index;
                self.head_branches = branches;
                self.clones_state = Load::Ready;
                // somebody pressed `E` while this was still on its way
                if self.wants_edit {
                    self.open_in_editor();
                }
            }

            Response::Agents { result } => match result {
                Ok(agents) => {
                    self.agents = agents;
                    self.agents_state = Load::Ready;
                }
                Err(e) => {
                    self.agents_state = Load::from(e);
                }
            },

            Response::Repos { login, result } => match result {
                Ok(mut repos) => {
                    // The row that gathers them all goes first, and is where a
                    // session starts: with a hundred repositories, "what is
                    // going on" is a better opening question than "in which
                    // one". Live only — the demo is the design's fixture and
                    // has no cross-repository data to gather.
                    if !repos.is_empty() {
                        repos.insert(0, crate::github::data::Repo::all(&repos));
                    }
                    if let Some(a) = self.accounts.iter_mut().find(|a| a.login == login) {
                        a.repos = repos;
                    }
                    self.repos_state.insert(login, Load::Ready);
                }
                Err(e) => {
                    self.flash_warn(self.advice(&e));
                    self.repos_state.insert(login, Load::from(e));
                }
            },

            Response::List { repo, tab, result } => match result {
                Ok(items) => {
                    self.lists.insert((repo.clone(), tab), items);
                    self.lists_state.insert((repo, tab), Load::Ready);
                }
                Err(e) => {
                    self.flash_warn(self.advice(&e));
                    self.lists_state.insert((repo, tab), Load::from(e));
                }
            },

            Response::IssueDetail { repo, num, result } => {
                // Split rather than borrowed: the state wants to *own* the
                // error now, and the body is still needed below.
                let (state, got) = match result {
                    Ok(v) => (Load::Ready, Some(v)),
                    Err(e) => (Load::from(e), None),
                };
                self.detail_state.insert((repo.clone(), num), state);
                if let Some((body, comments)) = got
                    && let Some(it) = self.item_mut(&repo, 0, num)
                {
                    it.body = body;
                    if let Some(d) = it.as_issue_mut() {
                        d.comments = comments.len() as u32;
                        d.comment_list = comments;
                    }
                }
            }

            Response::PrDetail { repo, num, result } => {
                let (state, got) = match result {
                    Ok(v) => (Load::Ready, Some(v)),
                    Err(e) => (Load::from(e), None),
                };
                self.detail_state.insert((repo.clone(), num), state);
                if let Some((body, files, reviews)) = got
                    && let Some(it) = self.item_mut(&repo, 1, num)
                {
                    it.body = body;
                    if let Some(pr) = it.as_pr_mut() {
                        pr.file_list = files;
                        pr.reviews = reviews;
                    }
                }
            }

            Response::Search {
                query,
                source,
                result,
            } => {
                // an answer to a query that is no longer typed is not an answer
                if query != self.finder_query || source != self.finder_source {
                    return;
                }
                match result {
                    Ok(hits) => {
                        self.finder_hits = hits
                            .into_iter()
                            .map(|h| crate::github::finder::Hit {
                                label: h.title,
                                detail: if h.sha.is_empty() {
                                    format!("{} #{} · {}", h.repo, h.num, h.when)
                                } else {
                                    format!("{} · {} · {}", h.repo, h.sha, h.when)
                                },
                                repo: h.repo,
                                num: h.num,
                                state: h.state,
                                kind: match source {
                                    crate::github::data::Source::Commits => {
                                        crate::github::finder::HitKind::Commit
                                    }
                                    crate::github::data::Source::Prs => {
                                        crate::github::finder::HitKind::Pr
                                    }
                                    _ => crate::github::finder::HitKind::Issue,
                                },
                            })
                            .collect();
                        self.finder_sel = 0;
                        self.finder_scroll = 0;
                        self.finder_state = Load::Ready;
                    }
                    Err(e) => {
                        self.finder_state = Load::from(e);
                        self.finder_hits.clear();
                    }
                }
            }

            Response::PrDiff { repo, num, result } => match result {
                Ok(files) => {
                    self.diff_state.insert((repo.clone(), num), Load::Ready);
                    // match the hunks against the already-loaded file list
                    if let Some(it) = self.item_mut(&repo, 1, num) {
                        for f in it.as_pr_mut().into_iter().flat_map(|p| &mut p.file_list) {
                            if let Some((_, hunks)) = files.iter().find(|(p, _)| *p == f.path) {
                                f.hunks = hunks.clone();
                            }
                        }
                    }
                }
                Err(e) => {
                    self.flash_warn(e.brief());
                    self.diff_state.insert((repo, num), Load::from(e));
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
                    self.jobs_state.insert((repo, run_id), Load::from(e));
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
                    self.logs_state.insert((repo, run_id), Load::from(e));
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

/// Is this the row an answer about `repo` / `num` is about? An item with no
/// repository of its own came from a single-repository list, where the number
/// alone is unambiguous.
fn is_it(item: &Item, repo: &str, num: i64) -> bool {
    item.num == num && (item.repo.is_empty() || item.repo == repo)
}
