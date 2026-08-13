//! State and reducer: a 1:1 port of the design's `Component` class.

use std::collections::{HashMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Flash, Prompt};
use crate::data::{self, Account, Item, Job, Kind, LogLine, RawLog, Repo, TABS};
use crate::demo;
use crate::service::{Request, Response, Service};

/// Where the data comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The design's fake data, with no network.
    Demo,
    /// The `gh` CLI against real GitHub.
    Live,
}

/// Load state of one piece of data.
#[derive(Clone, PartialEq, Eq)]
pub enum Load {
    Idle,
    Loading,
    Ready,
    Failed(String),
}

impl Load {
    pub fn is_loading(&self) -> bool {
        *self == Self::Loading
    }
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    List,
    Detail,
    /// A PR's changed files and their diff.
    Diff,
    Logs,
}

/// The navigable panes. `h`/`l` move between the ones the current view has,
/// and `j`/`k` always act on whichever one has focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The repository sidebar.
    Repos,
    /// The issue / PR / run list.
    List,
    /// The issue body or the PR description.
    Body,
    /// The checks pane of a PR or a run.
    Checks,
    /// The jobs and steps tree.
    Tree,
    /// The log output.
    Log,
    /// The changed-files list.
    Files,
    /// The diff contents.
    DiffBody,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    Colon,
    Slash,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Job,
    Step,
}

pub struct TreeNode {
    pub kind: NodeKind,
    pub ji: usize,
    pub name: String,
    pub status: String,
    pub dur: String,
}

pub struct LogRow {
    pub n: usize,
    /// Real HH:MM:SS when the log comes from GitHub; synthetic in demo mode.
    pub time: String,
    pub text: String,
    pub kind: &'static str,
}

pub struct App {
    pub source: Source,
    pub service: Option<Service>,
    pub accounts: Vec<Account>,
    pub accounts_state: Load,
    /// Load state of each account's repos, keyed by login.
    pub repos_state: HashMap<String, Load>,
    /// Lists keyed by `owner/repo` and tab; this is what the actions mutate.
    pub lists: HashMap<(String, usize), Vec<Item>>,
    pub lists_state: HashMap<(String, usize), Load>,
    /// A workflow run's jobs, keyed by `owner/repo` and run id.
    pub jobs_by_run: HashMap<(String, i64), Vec<Job>>,
    pub jobs_state: HashMap<(String, i64), Load>,
    /// A run's full log, not yet split per job.
    pub raw_logs: HashMap<(String, i64), Vec<RawLog>>,
    pub logs_state: HashMap<(String, i64), Load>,
    /// Load state of each PR's diff.
    pub diff_state: HashMap<(String, i64), Load>,
    /// A write action is in flight.
    pub busy: bool,
    pub prompt: Option<Prompt>,
    pub flash: Option<Flash>,
    pub acc: usize,
    pub repo: usize,
    pub tab: usize,
    pub pane: Pane,
    pub item: usize,
    pub view: View,
    pub check: usize,
    pub tree_sel: usize,
    pub collapsed: HashSet<usize>,
    pub follow: bool,
    pub extra_lines: usize,
    pub accounts_open: bool,
    pub acc_sel: usize,
    pub help_open: bool,
    pub cmd: Option<Cmd>,
    pub cmd_text: String,
    pub filter: String,
    pub log_filter: String,
    pub tick: u64,
    pub blink: bool,
    pub should_quit: bool,
    // scroll offsets of the scrollable panes
    pub repo_scroll: usize,
    pub item_scroll: usize,
    pub tree_scroll: usize,
    pub log_scroll: usize,
    /// Selected file in the diff view.
    pub file_idx: usize,
    /// Two-column diff instead of unified (`s`).
    pub split: bool,
    /// Ignore whitespace-only changes (`w`).
    pub ws: bool,
    pub diff_scroll: usize,
    /// Scroll offset of the issue body / PR description.
    pub detail_scroll: usize,
    /// True height of the scrollable pane, which only the render knows.
    pub detail_height: u16,
}

impl App {
    pub fn new(source: Source) -> Self {
        let accounts = if source == Source::Demo {
            demo::accounts()
        } else {
            Vec::new()
        };
        let mut lists = HashMap::new();
        let mut lists_state = HashMap::new();
        if source == Source::Demo {
            for a in &accounts {
                for (r, repo) in a.repos.iter().enumerate() {
                    let key = format!("{}/{}", a.login, repo.name);
                    lists.insert((key.clone(), 0), demo::issues(r));
                    lists.insert((key.clone(), 1), demo::prs(r));
                    lists.insert((key.clone(), 2), demo::runs(r));
                    for t in 0..3 {
                        lists_state.insert((key.clone(), t), Load::Ready);
                    }
                }
            }
        }
        Self {
            source,
            service: (source == Source::Live).then(Service::spawn),
            accounts_state: if source == Source::Demo {
                Load::Ready
            } else {
                Load::Idle
            },
            repos_state: HashMap::new(),
            lists,
            lists_state,
            jobs_by_run: HashMap::new(),
            jobs_state: HashMap::new(),
            raw_logs: HashMap::new(),
            logs_state: HashMap::new(),
            diff_state: HashMap::new(),
            busy: false,
            accounts,
            prompt: None,
            flash: None,
            acc: 0,
            // the design starts on the third repo; with real data, on the first
            repo: if source == Source::Demo { 2 } else { 0 },
            tab: 1, // 'prs'
            pane: Pane::List,
            item: 0,
            view: View::List,
            check: 0,
            tree_sel: 0,
            collapsed: HashSet::new(),
            follow: true,
            extra_lines: 0,
            accounts_open: false,
            acc_sel: 0,
            help_open: false,
            cmd: None,
            cmd_text: String::new(),
            filter: String::new(),
            log_filter: String::new(),
            tick: 0,
            blink: true,
            should_quit: false,
            repo_scroll: 0,
            item_scroll: 0,
            tree_scroll: 0,
            log_scroll: 0,
            file_idx: 0,
            split: false,
            ws: false,
            diff_scroll: 0,
            detail_scroll: 0,
            detail_height: 10,
        }
    }

    pub fn live(&self) -> bool {
        self.source == Source::Live
    }

    pub fn poll_service(&self) -> Option<Response> {
        self.service
            .as_ref()
            .and_then(super::service::Service::poll)
    }

    /// Is anything requested and still unanswered? The loop waits less if so.
    pub fn waiting(&self) -> bool {
        self.busy
            || self.accounts_state.is_loading()
            || self.repos_state.values().any(Load::is_loading)
            || self.lists_state.values().any(Load::is_loading)
            || self.jobs_state.values().any(Load::is_loading)
            || self.logs_state.values().any(Load::is_loading)
            || self.diff_state.values().any(Load::is_loading)
    }

    // --- selectores ---

    pub fn account(&self) -> Option<&Account> {
        self.accounts.get(self.acc)
    }

    pub fn login(&self) -> &str {
        self.account().map(|a| a.login.as_str()).unwrap_or("—")
    }

    pub fn repos(&self) -> &[Repo] {
        self.account().map(|a| a.repos.as_slice()).unwrap_or(&[])
    }

    pub fn repo_idx(&self) -> usize {
        self.repo.min(self.repos().len().saturating_sub(1))
    }

    pub fn repo(&self) -> Option<&Repo> {
        self.repos().get(self.repo_idx())
    }

    pub fn repo_name(&self) -> &str {
        self.repo().map(|r| r.name.as_str()).unwrap_or("—")
    }

    /// The `owner/repo` key that indexes lists, jobs and logs.
    pub fn repo_key(&self) -> String {
        format!("{}/{}", self.login(), self.repo_name())
    }

    fn matches(&self, t: &str) -> bool {
        let f = self.filter.trim().to_lowercase();
        f.is_empty() || t.to_lowercase().contains(&f)
    }

    /// The full, unfiltered list for the active repo and tab.
    pub fn list(&self) -> &[Item] {
        self.lists
            .get(&(self.repo_key(), self.tab))
            .map(std::vec::Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn list_state(&self) -> Load {
        self.lists_state
            .get(&(self.repo_key(), self.tab))
            .cloned()
            .unwrap_or(Load::Idle)
    }

    /// Indices into `list()` that pass the filter, in order.
    pub fn visible(&self) -> Vec<usize> {
        self.list()
            .iter()
            .enumerate()
            .filter(|(_, i)| self.matches(&i.title))
            .map(|(n, _)| n)
            .collect()
    }

    pub fn item_idx(&self, len: usize) -> usize {
        self.item.min(len.saturating_sub(1))
    }

    /// Position in `list()` of the selected item.
    pub fn current_index(&self) -> Option<usize> {
        let vis = self.visible();
        vis.get(self.item_idx(vis.len())).copied()
    }

    pub fn current(&self) -> Option<&Item> {
        self.current_index().map(|i| &self.list()[i])
    }

    /// Changed files of the selected PR.
    pub fn diff_files(&self) -> &[crate::data::FileChange] {
        self.current()
            .map(|c| c.file_list.as_slice())
            .unwrap_or(&[])
    }

    pub fn file_idx(&self) -> usize {
        self.file_idx.min(self.diff_files().len().saturating_sub(1))
    }

    /// Load state of the selected PR's diff.
    pub fn diff_status(&self) -> Load {
        let Some(num) = self.current().map(|c| c.num) else {
            return Load::Ready;
        };
        self.diff_state
            .get(&(self.repo_key(), num))
            .cloned()
            .unwrap_or(Load::Ready)
    }

    pub fn diff_file(&self) -> Option<&crate::data::FileChange> {
        self.diff_files().get(self.file_idx())
    }

    /// Lines of the selected file, numbered and with `w` already applied.
    pub fn diff_rows(&self) -> Vec<crate::data::DiffRow> {
        let Some(f) = self.diff_file() else {
            return Vec::new();
        };
        let hunks: Vec<crate::data::Hunk> = if self.ws {
            f.hunks.iter().map(strip_ws_only).collect()
        } else {
            f.hunks.clone()
        };
        crate::data::Hunk::rows(&hunks)
    }

    /// Opens the PR's diff at the given file.
    pub fn open_diff(&mut self, idx: usize) {
        if self.current().map(|c| c.kind != Kind::Pr).unwrap_or(true) {
            self.flash_warn("the diff is only available for pull requests");
            return;
        }
        self.view = View::Diff;
        self.file_idx = idx;
        self.diff_scroll = 0;
        self.pane = Pane::Files;
    }

    /// Id of the workflow run tied to the selection (0 if there is none).
    pub fn run_id(&self) -> i64 {
        self.current().map(|c| c.id).unwrap_or(0)
    }

    pub fn jobs(&self) -> Vec<Job> {
        if self.live() {
            let id = self.run_id();
            return self
                .jobs_by_run
                .get(&(self.repo_key(), id))
                .cloned()
                .unwrap_or_default();
        }
        let all = demo::job_templates();
        let only_success = match self.current() {
            Some(c) if c.kind == Kind::Run && c.state == "success" => true,
            Some(c) if c.kind == Kind::Pr && c.checks == "success" => true,
            _ => false,
        };
        if only_success {
            all.into_iter().filter(|j| j.status == "success").collect()
        } else {
            all
        }
    }

    pub fn jobs_status(&self) -> Load {
        self.jobs_state
            .get(&(self.repo_key(), self.run_id()))
            .cloned()
            .unwrap_or(Load::Ready)
    }

    pub fn logs_status(&self) -> Load {
        self.logs_state
            .get(&(self.repo_key(), self.run_id()))
            .cloned()
            .unwrap_or(Load::Ready)
    }

    pub fn flat_tree(&self) -> Vec<TreeNode> {
        let mut out = Vec::new();
        for (ji, j) in self.jobs().iter().enumerate() {
            out.push(TreeNode {
                kind: NodeKind::Job,
                ji,
                name: j.name.to_string(),
                status: j.status.clone(),
                dur: j.dur.clone(),
            });
            if !self.collapsed.contains(&ji) {
                for s in &j.steps {
                    out.push(TreeNode {
                        kind: NodeKind::Step,
                        ji,
                        name: s.name.to_string(),
                        status: s.status.clone(),
                        dur: s.dur.clone(),
                    });
                }
            }
        }
        out
    }

    pub fn tree_sel_idx(&self, len: usize) -> usize {
        self.tree_sel.min(len.saturating_sub(1))
    }

    pub fn log_lines(&self) -> Vec<LogRow> {
        if self.live() {
            return self.live_log_lines();
        }
        let tree = self.flat_tree();
        let idx = self.tree_sel_idx(tree.len());
        let (status, name) = match tree.get(idx) {
            Some(n) => (n.status.clone(), n.name.clone()),
            None => ("pending".to_string(), String::new()),
        };
        let is_step = tree
            .get(idx)
            .map(|n| n.kind == NodeKind::Step)
            .unwrap_or(false);

        let step_specific = if is_step && status != "failure" && status != "running" {
            demo::step_log(&name)
        } else {
            None
        };

        let mut lines: Vec<(String, &'static str)> = match step_specific {
            Some(v) => v,
            None => demo::logs_for(&status)
                .iter()
                .map(|(t, k)| (t.to_string(), *k))
                .collect(),
        };

        if status == "running" {
            lines.extend(
                demo::STREAM
                    .iter()
                    .take(self.extra_lines)
                    .map(|(t, k)| (t.to_string(), *k)),
            );
        }

        let f = self.log_filter.trim().to_lowercase();
        lines
            .into_iter()
            .enumerate()
            .map(|(i, (text, kind))| LogRow {
                n: i + 1,
                time: format!("10:4{}:0{}", (i + 1) % 10, (i + 1) % 6),
                text,
                kind,
            })
            .filter(|l| f.is_empty() || l.text.to_lowercase().contains(&f))
            .collect()
    }

    /// Real log: the run's dump is narrowed to the selected job (and step).
    fn live_log_lines(&self) -> Vec<LogRow> {
        let tree = self.flat_tree();
        let idx = self.tree_sel_idx(tree.len());
        let Some(node) = tree.get(idx) else {
            return Vec::new();
        };
        let jobs = self.jobs();
        let Some(job) = jobs.get(node.ji) else {
            return Vec::new();
        };
        let Some(raw) = self.raw_logs.get(&(self.repo_key(), self.run_id())) else {
            return Vec::new();
        };

        let step = (node.kind == NodeKind::Step).then_some(node.name.as_str());
        let lines: Vec<LogLine> = data::filter_log(raw, &job.name, step);

        let f = self.log_filter.trim().to_lowercase();
        lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| LogRow {
                n: i + 1,
                time: l.time,
                text: l.text,
                kind: l.kind,
            })
            .filter(|l| f.is_empty() || l.text.to_lowercase().contains(&f))
            .collect()
    }

    // --- data loading ---

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
                let (num, kind) = (cur.num, cur.kind);
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
        let (kind, num, id, loaded) = (cur.kind, cur.num, cur.id, cur.detail_loaded);
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
                .map(|c| c.state != "running" && c.checks != "running")
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
                    it.comments = comments.len() as u32;
                    it.comment_list = comments;
                }
            }

            Response::PrDetail { repo, num, result } => {
                if let Ok((body, files, reviews)) = result
                    && let Some(items) = self.lists.get_mut(&(repo, 1))
                    && let Some(it) = items.iter_mut().find(|i| i.num == num)
                {
                    it.body = body;
                    it.file_list = files;
                    it.reviews = reviews;
                }
            }

            Response::PrDiff { repo, num, result } => match result {
                Ok(files) => {
                    self.diff_state.insert((repo.clone(), num), Load::Ready);
                    // match the hunks against the already-loaded file list
                    if let Some(items) = self.lists.get_mut(&(repo, 1))
                        && let Some(it) = items.iter_mut().find(|i| i.num == num)
                    {
                        for f in &mut it.file_list {
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

    // --- reducer ---

    /// Scrolls the detail body. The real limit is applied by the render, which
    /// is what knows how many lines the content takes at this width.
    pub fn scroll_detail(&mut self, d: i64) {
        self.detail_scroll = (self.detail_scroll as i64 + d).max(0) as usize;
    }

    /// The current view's panes, left to right. This is what `h` and `l` walk.
    pub fn panes(&self) -> Vec<Pane> {
        match self.view {
            View::Logs => vec![Pane::Tree, Pane::Log],
            View::Diff => vec![Pane::Files, Pane::DiffBody],
            View::List => vec![Pane::Repos, Pane::List],
            View::Detail => {
                let issue = self
                    .current()
                    .map(|c| c.kind == Kind::Issue)
                    .unwrap_or(true);
                if issue {
                    vec![Pane::Repos, Pane::Body]
                } else {
                    vec![Pane::Repos, Pane::Body, Pane::Checks]
                }
            }
        }
    }

    /// Moves focus one pane left (`-1`) or right (`1`). `h`/`l` stop at the
    /// edges; `tab` wraps around.
    fn focus_by(&mut self, d: i64, wrap: bool) {
        let panes = self.panes();
        let n = panes.len() as i64;
        let i = panes.iter().position(|p| *p == self.pane).unwrap_or(0) as i64;
        let j = if wrap {
            (i + d).rem_euclid(n)
        } else {
            (i + d).clamp(0, n - 1)
        };
        self.pane = panes[j as usize];
    }

    /// `g` / `G`: to the start or the end of the focused pane.
    fn goto(&mut self, top: bool) {
        match self.pane {
            Pane::Body | Pane::Log | Pane::DiffBody if top => {
                if self.pane == Pane::Log {
                    self.follow = false;
                }
                self.detail_scroll = 0;
                self.log_scroll = 0;
                self.diff_scroll = 0;
            }
            Pane::DiffBody => self.diff_scroll = usize::MAX,
            // the real limit is applied by the render, which knows the length
            Pane::Body => self.detail_scroll = usize::MAX,
            Pane::Log => {
                self.follow = false;
                self.log_scroll = usize::MAX;
            }
            _ => self.move_by(if top {
                -i64::from(u32::MAX)
            } else {
                i64::from(u32::MAX)
            }),
        }
    }

    /// Leaves focus on a pane that the current view actually has.
    fn settle_pane(&mut self) {
        let panes = self.panes();
        if !panes.contains(&self.pane) {
            self.pane = *panes.last().unwrap_or(&Pane::List);
        }
    }

    /// `j`/`k`: always on the focused pane.
    fn move_by(&mut self, d: i64) {
        match self.pane {
            Pane::Repos => {
                let n = self.repos().len() as i64;
                self.repo = (self.repo as i64 + d).clamp(0, (n - 1).max(0)) as usize;
                self.item = 0;
                self.item_scroll = 0;
                self.view = View::List;
            }
            Pane::List => {
                let n = self.visible().len() as i64;
                self.item = (self.item as i64 + d).clamp(0, (n - 1).max(0)) as usize;
                self.detail_scroll = 0;
            }
            Pane::Body => self.scroll_detail(d),
            Pane::Checks => {
                let n = self.jobs().len() as i64;
                self.check = (self.check as i64 + d).clamp(0, (n - 1).max(0)) as usize;
            }
            Pane::Tree => {
                let len = self.flat_tree().len() as i64;
                self.tree_sel = (self.tree_sel as i64 + d).clamp(0, (len - 1).max(0)) as usize;
                self.extra_lines = 0;
                self.log_scroll = 0;
            }
            Pane::Log => {
                // moving through the log by hand takes over from follow mode
                self.follow = false;
                self.log_scroll = (self.log_scroll as i64 + d).max(0) as usize;
            }
            Pane::Files => {
                let n = self.diff_files().len() as i64;
                self.file_idx = (self.file_idx as i64 + d).clamp(0, (n - 1).max(0)) as usize;
                self.diff_scroll = 0;
            }
            Pane::DiffBody => {
                self.diff_scroll = (self.diff_scroll as i64 + d).max(0) as usize;
            }
        }
    }

    /// Half a page or a whole one, on the panes that scroll.
    fn page_by(&mut self, pages: i64) {
        let h = self.detail_height.max(1) as i64;
        match self.pane {
            Pane::Body | Pane::Log | Pane::DiffBody => self.move_by(pages * h),
            _ => self.move_by(pages * (h / 2).max(1)),
        }
    }

    fn tree_index_for_job(&self, ji: usize) -> usize {
        self.flat_tree()
            .iter()
            .position(|n| n.kind == NodeKind::Job && n.ji == ji)
            .unwrap_or(0)
    }

    /// `enter`: drills into the focused pane.
    fn enter(&mut self) {
        if self.accounts_open {
            return;
        }
        match self.pane {
            Pane::Repos => {
                self.view = View::List;
                self.item = 0;
                self.pane = Pane::List;
            }
            Pane::List => {
                if self.current().is_none() {
                    return;
                }
                self.view = View::Detail;
                self.check = 0;
                self.detail_scroll = 0;
                // land on the body: that is what you want to read first
                self.pane = Pane::Body;
            }
            Pane::Checks => {
                self.view = View::Logs;
                self.tree_sel = self.tree_index_for_job(self.check);
                self.extra_lines = 0;
                self.log_scroll = 0;
                self.pane = Pane::Tree;
            }
            // the tree leads to the output and the file list to the diff
            // itself; the body opens the PR's diff
            Pane::Tree => self.pane = Pane::Log,
            Pane::Files => self.pane = Pane::DiffBody,
            Pane::Body => self.open_diff(0),
            Pane::Log | Pane::DiffBody => {}
        }
    }

    /// `esc` / `q`: leaves the pane, and the view once on the first one.
    fn back(&mut self) {
        if self.cmd.is_some() {
            self.cmd = None;
            self.cmd_text.clear();
            return;
        }
        if self.accounts_open || self.help_open {
            self.accounts_open = false;
            self.help_open = false;
            return;
        }
        match self.view {
            View::Logs => {
                if self.pane == Pane::Log {
                    self.pane = Pane::Tree;
                } else {
                    self.view = View::Detail;
                    self.pane = Pane::Checks;
                    self.settle_pane();
                }
            }
            View::Diff => {
                if self.pane == Pane::DiffBody {
                    self.pane = Pane::Files;
                } else {
                    self.view = View::Detail;
                    self.pane = Pane::Body;
                }
            }
            View::Detail => {
                self.view = View::List;
                self.pane = Pane::List;
            }
            View::List => {
                if self.pane == Pane::List {
                    self.pane = Pane::Repos;
                }
            }
        }
    }

    fn run_cmd(&mut self, raw: &str) {
        let c = raw.trim().trim_start_matches(':').to_string();
        match c.as_str() {
            "account" | "accounts" => {
                self.accounts_open = true;
                self.acc_sel = self.acc;
            }
            "issues" | "prs" | "actions" => {
                self.tab = TABS.iter().position(|t| t.id == c).unwrap_or(0);
                self.view = View::List;
                self.item = 0;
                self.pane = Pane::List;
            }
            "logs" => {
                self.view = View::Logs;
                self.pane = Pane::Tree;
            }
            "diff" | "files" => self.open_diff(0),
            "help" | "h" => self.help_open = true,
            "q" | "quit" => {
                self.view = View::List;
                self.accounts_open = false;
                self.help_open = false;
            }
            _ => {}
        }
        self.cmd = None;
        self.cmd_text.clear();
    }

    fn pick_account(&mut self, i: usize) {
        self.acc = i;
        self.repo = 0;
        self.item = 0;
        self.view = View::List;
        self.pane = Pane::Repos;
        self.accounts_open = false;
        self.filter.clear();
        self.repo_scroll = 0;
        self.item_scroll = 0;
    }

    pub fn on_key(&mut self, ev: KeyEvent) {
        // half a page up/down in the focused pane, vim style
        if ev.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(self.pane, Pane::Body | Pane::Log)
        {
            let half = (self.detail_height / 2).max(1) as i64;
            match ev.code {
                KeyCode::Char('d') => return self.move_by(half),
                KeyCode::Char('u') => return self.move_by(-half),
                _ => {}
            }
        }

        // Actually quitting the program (the design lives in a browser).
        if ev.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(ev.code, KeyCode::Char('c' | 'd'))
        {
            self.should_quit = true;
            return;
        }

        // A pending confirmation swallows every key.
        if let Some(prompt) = self.prompt.clone() {
            match ev.code {
                KeyCode::Enter | KeyCode::Char('y') => self.confirm(),
                KeyCode::Esc | KeyCode::Char('n' | 'q') => self.cancel_prompt(),
                KeyCode::Char('j') | KeyCode::Down => {
                    if let Prompt::Merge(m) = prompt {
                        self.prompt = Some(Prompt::Merge((m + 1).min(2)));
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if let Prompt::Merge(m) = prompt {
                        self.prompt = Some(Prompt::Merge(m.saturating_sub(1)));
                    }
                }
                KeyCode::Char(c @ '1'..='3') => {
                    if matches!(prompt, Prompt::Merge(_)) {
                        self.prompt = Some(Prompt::Merge(c as usize - '1' as usize));
                    }
                }
                _ => {}
            }
            return;
        }

        if let Some(mode) = self.cmd {
            match ev.code {
                KeyCode::Esc => {
                    self.cmd = None;
                    self.cmd_text.clear();
                }
                KeyCode::Enter => {
                    if mode == Cmd::Colon {
                        let t = self.cmd_text.clone();
                        self.run_cmd(&t);
                    } else {
                        self.cmd = None;
                    }
                }
                KeyCode::Backspace => {
                    self.cmd_text.pop();
                    self.sync_filter(mode);
                }
                KeyCode::Char(c) => {
                    self.cmd_text.push(c);
                    self.sync_filter(mode);
                }
                _ => {}
            }
            return;
        }

        if self.accounts_open {
            match ev.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.acc_sel = (self.acc_sel + 1).min(self.accounts.len() - 1);
                }
                KeyCode::Char('k') | KeyCode::Up => self.acc_sel = self.acc_sel.saturating_sub(1),
                KeyCode::Enter => self.pick_account(self.acc_sel),
                KeyCode::Esc | KeyCode::Char('q' | 'a') => {
                    self.accounts_open = false;
                }
                _ => {}
            }
            return;
        }

        if self.help_open {
            match ev.code {
                KeyCode::Esc | KeyCode::Char('q') => self.help_open = false,
                KeyCode::Char('?') => self.help_open = false,
                _ => {}
            }
            return;
        }

        match ev.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_by(-1),
            KeyCode::Char('h') | KeyCode::Left => self.focus_by(-1, false),
            KeyCode::Char('l') | KeyCode::Right => self.focus_by(1, false),
            KeyCode::Tab => self.focus_by(1, true),
            KeyCode::BackTab => self.focus_by(-1, true),
            KeyCode::Char('g') => self.goto(true),
            KeyCode::Char('G') => self.goto(false),
            KeyCode::Enter => self.enter(),
            KeyCode::Esc | KeyCode::Char('q') => self.back(),
            KeyCode::Char('a') => {
                self.accounts_open = true;
                self.acc_sel = self.acc;
            }
            KeyCode::Char('?') => self.help_open = true,
            KeyCode::Char(':') => {
                self.cmd = Some(Cmd::Colon);
                self.cmd_text.clear();
            }
            KeyCode::Char('/') => {
                self.cmd = Some(Cmd::Slash);
                self.cmd_text = if self.view == View::Logs {
                    self.log_filter.clone()
                } else {
                    self.filter.clone()
                };
            }
            KeyCode::PageDown => self.page_by(1),
            KeyCode::PageUp => self.page_by(-1),
            KeyCode::Char('d') if self.actionable_pr() && self.view != View::Diff => {
                self.open_diff(0);
            }
            KeyCode::Char('s') if self.view == View::Diff => {
                self.split = !self.split;
                self.diff_scroll = 0;
            }
            KeyCode::Char('w') if self.view == View::Diff => {
                self.ws = !self.ws;
                self.diff_scroll = 0;
            }
            KeyCode::Char('f') => self.follow = !self.follow,
            KeyCode::Char('r') => {
                self.tick += 1;
                self.extra_lines = 0;
                self.refresh();
            }
            KeyCode::Char(c @ '1'..='3') => {
                self.tab = c as usize - '1' as usize;
                self.view = View::List;
                self.item = 0;
                self.item_scroll = 0;
                self.pane = Pane::List;
                self.check = 0;
            }
            KeyCode::Char('o') if self.view == View::Logs => {
                let tree = self.flat_tree();
                if let Some(node) = tree.get(self.tree_sel_idx(tree.len())) {
                    let ji = node.ji;
                    if !self.collapsed.remove(&ji) {
                        self.collapsed.insert(ji);
                    }
                }
            }
            KeyCode::Char('e') if self.view == View::Logs => {
                if let Some(i) = self.log_lines().iter().position(|l| l.kind == "red") {
                    self.log_scroll = i.saturating_sub(3);
                    self.follow = false;
                }
            }
            // --- actions on the selected pull request
            KeyCode::Char('m') if self.actionable_pr() => self.ask_merge(),
            KeyCode::Char('c') if self.actionable_pr() => self.ask_close(),
            // `d` opens the diff (as it does in the design), so deleting a
            // branch, which is destructive, lives on the shifted key
            KeyCode::Char('D') if self.actionable_pr() => self.ask_delete_branch(),
            KeyCode::Char(k @ ('m' | 'c' | 'D')) => {
                let what = match k {
                    'm' => "merge",
                    'c' => "close",
                    _ => "branch deletion",
                };
                self.flash_warn(format!("{what} only applies to pull requests"));
            }
            _ => {}
        }
        self.settle_pane();
    }

    fn sync_filter(&mut self, mode: Cmd) {
        if mode != Cmd::Slash {
            return;
        }
        let t = self.cmd_text.clone();
        if self.view == View::Logs {
            self.log_filter = t;
            self.log_scroll = 0;
        } else {
            self.filter = t;
            self.item = 0;
            self.item_scroll = 0;
        }
    }

    /// 1400 ms heartbeat: advances the log stream like the design's `setInterval`.
    pub fn tick(&mut self) {
        if self.view == View::Logs && self.extra_lines < demo::STREAM.len() {
            self.extra_lines += 1;
        }
        if let Some(f) = &mut self.flash {
            f.ttl = f.ttl.saturating_sub(1);
            if f.ttl == 0 {
                self.flash = None;
            }
        }
        self.tick = self.tick.wrapping_add(1);
    }
}

/// Drops the +/- pairs whose contents differ only in whitespace, which is what
/// "ignore whitespace" is expected to do.
fn strip_ws_only(h: &crate::data::Hunk) -> crate::data::Hunk {
    let mut lines: Vec<(char, String)> = Vec::new();
    let mut i = 0;
    while i < h.lines.len() {
        let (sign, text) = &h.lines[i];
        if *sign == '-' {
            // look for the matching additions within the same block
            let dels: Vec<&(char, String)> =
                h.lines[i..].iter().take_while(|(s, _)| *s == '-').collect();
            let adds: Vec<&(char, String)> = h.lines[i + dels.len()..]
                .iter()
                .take_while(|(s, _)| *s == '+')
                .collect();
            if dels.len() == adds.len()
                && dels
                    .iter()
                    .zip(&adds)
                    .all(|(d, a)| squeeze(&d.1) == squeeze(&a.1))
            {
                // same content bar whitespace: keep it as context only
                for d in &dels {
                    lines.push((' ', d.1.clone()));
                }
                i += dels.len() + adds.len();
                continue;
            }
        }
        lines.push((*sign, text.clone()));
        i += 1;
    }
    crate::data::Hunk {
        hdr: h.hdr.clone(),
        lines,
    }
}

/// The text with no whitespace, so it can be compared ignoring it.
fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, reason = "assertions")]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn demo() -> App {
        App::new(Source::Demo)
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
    }

    fn ch(app: &mut App, c: char) {
        press(app, KeyCode::Char(c));
    }

    // --- panes and focus ---

    #[test]
    fn each_view_exposes_its_own_panes() {
        let mut app = demo();
        assert_eq!(app.panes(), vec![Pane::Repos, Pane::List]);

        // a PR has a checks pane; an issue does not
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body, Pane::Checks]);

        let mut app = demo();
        ch(&mut app, '1'); // issues tab
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.panes(), vec![Pane::Repos, Pane::Body]);
    }

    #[test]
    fn h_and_l_stop_at_the_edges() {
        let mut app = demo();
        app.pane = Pane::Repos;
        ch(&mut app, 'h');
        assert_eq!(app.pane, Pane::Repos, "h at the leftmost pane stays put");

        app.pane = Pane::List;
        ch(&mut app, 'l');
        assert_eq!(app.pane, Pane::List, "l at the rightmost pane stays put");
    }

    #[test]
    fn tab_cycles_all_the_way_around() {
        let mut app = demo();
        press(&mut app, KeyCode::Enter); // PR detail: three panes
        app.pane = Pane::Repos;
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.pane, Pane::Body);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.pane, Pane::Checks);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.pane, Pane::Repos, "tab wraps around");
        press(&mut app, KeyCode::BackTab);
        assert_eq!(app.pane, Pane::Checks, "shift-tab wraps the other way");
    }

    #[test]
    fn enter_and_esc_walk_the_same_path_in_reverse() {
        let mut app = demo();
        app.pane = Pane::Repos;

        press(&mut app, KeyCode::Enter); // repos -> list
        assert!(app.view == View::List && app.pane == Pane::List);
        press(&mut app, KeyCode::Enter); // list -> detail, landing on the body
        assert!(app.view == View::Detail && app.pane == Pane::Body);
        ch(&mut app, 'l'); // body -> checks
        press(&mut app, KeyCode::Enter); // checks -> logs
        assert!(app.view == View::Logs && app.pane == Pane::Tree);
        press(&mut app, KeyCode::Enter); // tree -> log output
        assert_eq!(app.pane, Pane::Log);

        press(&mut app, KeyCode::Esc);
        assert!(app.view == View::Logs && app.pane == Pane::Tree);
        press(&mut app, KeyCode::Esc);
        assert!(app.view == View::Detail && app.pane == Pane::Checks);
        press(&mut app, KeyCode::Esc);
        assert!(app.view == View::List && app.pane == Pane::List);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.pane, Pane::Repos);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.pane, Pane::Repos, "there is nothing left to go back to");
    }

    #[test]
    fn the_focused_pane_is_always_one_the_view_has() {
        let mut app = demo();
        // land on the checks pane, then jump to the issues tab, which has none
        press(&mut app, KeyCode::Enter);
        ch(&mut app, 'l');
        assert_eq!(app.pane, Pane::Checks);
        ch(&mut app, '1');
        assert!(app.panes().contains(&app.pane));
    }

    // --- movement ---

    #[test]
    fn j_and_k_clamp_instead_of_wrapping() {
        let mut app = demo();
        app.pane = Pane::List;
        let last = app.visible().len() - 1;
        for _ in 0..50 {
            ch(&mut app, 'j');
        }
        assert_eq!(app.item, last, "j stops at the last item");
        for _ in 0..50 {
            ch(&mut app, 'k');
        }
        assert_eq!(app.item, 0, "k stops at the first");
    }

    #[test]
    fn g_and_shift_g_reach_the_ends() {
        let mut app = demo();
        app.pane = Pane::List;
        ch(&mut app, 'G');
        assert_eq!(app.item, app.visible().len() - 1);
        ch(&mut app, 'g');
        assert_eq!(app.item, 0);
    }

    #[test]
    fn scrolling_the_log_by_hand_drops_follow_mode() {
        let mut app = demo();
        app.view = View::Logs;
        app.pane = Pane::Log;
        assert!(app.follow);
        ch(&mut app, 'j');
        assert!(!app.follow, "manual movement takes over from follow");
    }

    #[test]
    fn moving_between_items_resets_the_body_scroll() {
        let mut app = demo();
        app.pane = Pane::List;
        app.detail_scroll = 42;
        ch(&mut app, 'j');
        assert_eq!(app.detail_scroll, 0);
    }

    // --- empty and degenerate states ---

    #[test]
    fn an_app_with_no_accounts_does_not_panic() {
        // this is what live mode looks like before the first response lands
        let app = App::new(Source::Live);
        assert_eq!(app.repo_idx(), 0);
        assert_eq!(app.login(), "—");
        assert_eq!(app.repo_name(), "—");
        assert!(app.repo().is_none());
        assert!(app.current().is_none());
        assert!(app.list().is_empty());
        assert!(app.visible().is_empty());
        assert!(app.diff_files().is_empty());
        assert_eq!(app.file_idx(), 0);
        assert!(app.diff_rows().is_empty());
    }

    #[test]
    fn a_filter_that_matches_nothing_leaves_no_selection() {
        let mut app = demo();
        app.filter = "zzzzzzzz".into();
        assert!(app.visible().is_empty());
        assert!(app.current().is_none());
        assert!(app.current_index().is_none());

        // and the actions that need a selection simply do nothing
        app.ask_merge();
        assert!(app.prompt.is_none());
        app.confirm();
        assert!(app.prompt.is_none());
    }

    #[test]
    fn navigating_an_empty_list_stays_at_zero() {
        let mut app = demo();
        app.filter = "zzzzzzzz".into();
        app.pane = Pane::List;
        ch(&mut app, 'j');
        ch(&mut app, 'G');
        assert_eq!(app.item, 0);
    }

    // --- pull request actions ---

    #[test]
    fn merge_is_refused_for_everything_but_an_open_pr() {
        let mut app = demo();
        app.pane = Pane::List;

        // the draft PR of the demo data
        app.item = 3;
        assert_eq!(app.current().unwrap().state, "draft");
        app.ask_merge();
        assert!(app.prompt.is_none(), "a draft cannot be merged");

        // and the already merged one
        app.item = 4;
        assert_eq!(app.current().unwrap().state, "merged");
        app.ask_merge();
        assert!(app.prompt.is_none());
    }

    #[test]
    fn a_merge_updates_the_pr_and_offers_the_branch() {
        let mut app = demo();
        app.pane = Pane::List;
        app.item = 0;
        let open_prs = app.repo().unwrap().prs;

        app.ask_merge();
        assert!(matches!(app.prompt, Some(Prompt::Merge(0))));
        app.confirm();

        let pr = app.current().unwrap();
        assert_eq!(pr.state, "merged");
        assert_eq!(pr.merged_with.as_deref(), Some("merge commit"));
        assert_eq!(app.repo().unwrap().prs, open_prs - 1, "one less open PR");
        // GitHub offers to delete the branch right after
        assert!(matches!(app.prompt, Some(Prompt::DeleteBranch { .. })));

        app.confirm();
        assert!(app.current().unwrap().branch_deleted);
    }

    #[test]
    fn closing_and_reopening_a_pr_round_trips() {
        let mut app = demo();
        app.pane = Pane::List;
        let open_prs = app.repo().unwrap().prs;

        app.ask_close();
        app.confirm();
        assert_eq!(app.current().unwrap().state, "closed");
        assert_eq!(app.repo().unwrap().prs, open_prs - 1);

        app.ask_close(); // now it reopens
        assert!(matches!(app.prompt, Some(Prompt::Reopen)));
        app.confirm();
        assert_eq!(app.current().unwrap().state, "open");
        assert_eq!(app.repo().unwrap().prs, open_prs);
    }

    #[test]
    fn the_branch_prompt_remembers_which_branch_it_asked_about() {
        let mut app = demo();
        app.pane = Pane::List;
        app.ask_merge();
        app.confirm();

        let Some(Prompt::DeleteBranch { num, branch }) = app.prompt.clone() else {
            panic!("expected a delete-branch prompt");
        };
        // moving the selection must not change what gets deleted
        let expected = app.current().unwrap().num;
        assert_eq!(num, expected);
        assert!(!branch.is_empty());
    }

    #[test]
    fn a_branch_cannot_be_deleted_while_the_pr_is_open() {
        let mut app = demo();
        app.pane = Pane::List;
        app.ask_delete_branch();
        assert!(app.prompt.is_none());
    }

    #[test]
    fn cancelling_a_prompt_changes_nothing() {
        let mut app = demo();
        app.pane = Pane::List;
        let before = app.current().unwrap().state.clone();
        app.ask_merge();
        app.cancel_prompt();
        assert!(app.prompt.is_none());
        assert_eq!(app.current().unwrap().state, before);
    }

    // --- diff view ---

    #[test]
    fn the_diff_only_opens_on_a_pull_request() {
        let mut app = demo();
        ch(&mut app, '1'); // issues
        app.pane = Pane::List;
        ch(&mut app, 'd');
        assert_ne!(app.view, View::Diff);

        ch(&mut app, '2'); // pull requests
        ch(&mut app, 'd');
        assert_eq!(app.view, View::Diff);
        assert_eq!(app.pane, Pane::Files);
    }

    #[test]
    fn split_and_whitespace_toggles_only_bite_inside_the_diff() {
        let mut app = demo();
        app.pane = Pane::List;
        ch(&mut app, 's');
        assert!(!app.split, "s does nothing outside the diff view");

        ch(&mut app, 'd');
        ch(&mut app, 's');
        assert!(app.split);
        ch(&mut app, 'w');
        assert!(app.ws);
    }

    #[test]
    fn a_file_with_no_hunks_yields_no_rows() {
        let mut app = demo();
        app.pane = Pane::List;
        ch(&mut app, 'd');
        // CHANGELOG.md is last in the demo data and has no textual changes
        let last = app.diff_files().len() - 1;
        app.file_idx = last;
        assert_eq!(app.diff_file().unwrap().path, "CHANGELOG.md");
        assert!(app.diff_rows().is_empty());
    }

    // --- ignore whitespace ---

    fn hunk(lines: &[(char, &str)]) -> crate::data::Hunk {
        crate::data::Hunk {
            hdr: "@@ -1,1 +1,1 @@".into(),
            lines: lines.iter().map(|(c, t)| (*c, t.to_string())).collect(),
        }
    }

    #[test]
    fn whitespace_only_changes_collapse_into_context() {
        let h = hunk(&[('-', "let x = 1;"), ('+', "let  x  =  1;")]);
        let out = strip_ws_only(&h);
        assert_eq!(out.lines.len(), 1);
        assert_eq!(out.lines[0].0, ' ', "it becomes a context line");
    }

    #[test]
    fn a_real_change_survives_the_whitespace_filter() {
        let h = hunk(&[('-', "let x = 1;"), ('+', "let x = 2;")]);
        assert_eq!(strip_ws_only(&h).lines.len(), 2);
    }

    #[test]
    fn unbalanced_blocks_are_left_alone() {
        // one deletion, two additions: not a whitespace-only rewrite
        let h = hunk(&[('-', "a"), ('+', "a"), ('+', "b")]);
        assert_eq!(strip_ws_only(&h).lines.len(), 3);
    }

    #[test]
    fn context_only_hunks_pass_through_untouched() {
        let h = hunk(&[(' ', "a"), (' ', "b")]);
        assert_eq!(strip_ws_only(&h).lines.len(), 2);
    }

    // --- command line ---

    #[test]
    fn a_slash_filter_updates_as_you_type_and_esc_keeps_it() {
        let mut app = demo();
        ch(&mut app, '/');
        for c in "clamp".chars() {
            ch(&mut app, c);
        }
        assert_eq!(app.filter, "clamp");
        assert_eq!(app.visible().len(), 1);

        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.filter, "clam");

        // esc closes the prompt but leaves the filter applied, as in the design
        press(&mut app, KeyCode::Esc);
        assert!(app.cmd.is_none());
        assert_eq!(app.filter, "clam");
    }

    #[test]
    fn unknown_commands_are_ignored_without_leaving_the_prompt_open() {
        let mut app = demo();
        ch(&mut app, ':');
        for c in "nonsense".chars() {
            ch(&mut app, c);
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.cmd.is_none());
        assert!(app.cmd_text.is_empty());
        assert_eq!(app.view, View::List);
    }

    #[test]
    fn commands_reach_every_view() {
        for (cmd, view) in [("issues", View::List), ("logs", View::Logs)] {
            let mut app = demo();
            ch(&mut app, ':');
            for c in cmd.chars() {
                ch(&mut app, c);
            }
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.view, view, "`:{cmd}` should switch view");
        }
    }

    // --- flash messages ---

    #[test]
    fn a_flash_fades_after_a_few_ticks() {
        let mut app = demo();
        app.flash_ok("done");
        assert!(app.flash.is_some());
        for _ in 0..3 {
            app.tick();
        }
        assert!(app.flash.is_none());
    }
}
