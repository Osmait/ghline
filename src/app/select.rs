//! Read-only queries over the state. Nothing here mutates; the view and the
//! reducer both go through these so the indexing rules live in one place.

use super::input::strip_ws_only;
use super::{App, Load, LogRow, NodeKind, Pane, TreeNode, View};
use crate::data::{self, Account, Item, Job, Kind, LogLine, Repo, Status};
use crate::demo;

impl App {
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

    /// What to print for the selected repository, which for the pseudo-repo is
    /// not its name.
    pub fn repo_label(&self) -> &str {
        self.repo().map(crate::data::Repo::label).unwrap_or("—")
    }

    /// The `owner/repo` key that indexes lists, jobs and logs.
    pub fn repo_key(&self) -> String {
        format!("{}/{}", self.login(), self.repo_name())
    }

    /// Is the pseudo-repository that gathers every repository selected?
    pub fn is_all(&self) -> bool {
        self.repo().is_some_and(crate::data::Repo::is_all)
    }

    /// The `owner/repo` of every repository that has any workflows, which is
    /// the only set worth asking for runs.
    pub fn workflow_repos(&self) -> Vec<String> {
        let login = self.login();
        self.repos()
            .iter()
            .filter(|r| !r.is_all() && r.has_workflows)
            .map(|r| format!("{login}/{}", r.name))
            .collect()
    }

    /// The `owner/repo` of the *selected item*, which in a list that spans
    /// repositories is not the one the list is filed under.
    ///
    /// Everything downstream of the selection — the body, the diff, the
    /// checks, the logs, and merging or closing it — has to follow the item
    /// rather than the pane it happened to be listed in. An item with no
    /// repository of its own came from a single-repository list, where the two
    /// are the same thing.
    pub fn item_repo_key(&self) -> String {
        match self.current() {
            Some(c) if !c.repo.is_empty() => c.repo.clone(),
            _ => self.repo_key(),
        }
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
            // a row's repository is part of what is on screen, so `/` has to
            // filter on it too — otherwise "show me only sbql" would not work
            // in the one list where it is worth asking
            .filter(|(_, i)| self.matches(&i.title) || self.matches(&i.repo))
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
        self.current().map_or(&[], Item::files)
    }

    pub fn file_idx(&self) -> usize {
        self.file_idx.min(self.diff_files().len().saturating_sub(1))
    }

    /// The rows the finder is showing. Repositories are ranked here; the rest
    /// arrive already ranked by GitHub.
    pub fn finder_results(&self) -> Vec<crate::finder::Hit> {
        if !self.finder_source.is_local() {
            return self.finder_hits.clone();
        }
        let repos = self.repos();
        crate::fuzzy::rank(&self.finder_query, repos, |r| r.label())
            .into_iter()
            .map(|(i, _)| {
                let r = &repos[i];
                crate::finder::Hit {
                    label: r.label().to_string(),
                    detail: format!(
                        "{} · {} open issues · {} open PRs",
                        if r.lang.is_empty() { "—" } else { &r.lang },
                        r.issues,
                        r.prs
                    ),
                    repo: format!("{}/{}", self.login(), r.name),
                    num: 0,
                    state: Status::Unknown,
                    kind: crate::finder::HitKind::Repo,
                }
            })
            .collect()
    }

    pub fn finder_len(&self) -> usize {
        if self.finder_source.is_local() {
            self.finder_results().len()
        } else {
            self.finder_hits.len()
        }
    }

    /// Load state of the selected item's body, files and reviews.
    pub fn detail_status(&self) -> Load {
        let Some(num) = self.current().map(|c| c.num) else {
            return Load::Ready;
        };
        self.detail_state
            .get(&(self.item_repo_key(), num))
            .cloned()
            .unwrap_or(Load::Ready)
    }

    /// Load state of the selected PR's diff.
    pub fn diff_status(&self) -> Load {
        let Some(num) = self.current().map(|c| c.num) else {
            return Load::Ready;
        };
        self.diff_state
            .get(&(self.item_repo_key(), num))
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
        if self.current().map(|c| c.kind() != Kind::Pr).unwrap_or(true) {
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
                .get(&(self.item_repo_key(), id))
                .cloned()
                .unwrap_or_default();
        }
        let all = demo::job_templates();
        let only_success = match self.current() {
            Some(c) if c.kind() == Kind::Run && c.state == Status::Success => true,
            Some(c) if c.kind() == Kind::Pr && c.checks() == Status::Success => true,
            _ => false,
        };
        if only_success {
            all.into_iter()
                .filter(|j| j.status == Status::Success)
                .collect()
        } else {
            all
        }
    }

    pub fn jobs_status(&self) -> Load {
        self.jobs_state
            .get(&(self.item_repo_key(), self.run_id()))
            .cloned()
            .unwrap_or(Load::Ready)
    }

    pub fn logs_status(&self) -> Load {
        self.logs_state
            .get(&(self.item_repo_key(), self.run_id()))
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
                status: j.status,
                dur: j.dur.clone(),
            });
            if !self.collapsed.contains(&ji) {
                for s in &j.steps {
                    out.push(TreeNode {
                        kind: NodeKind::Step,
                        ji,
                        name: s.name.to_string(),
                        status: s.status,
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
            Some(n) => (n.status, n.name.clone()),
            None => (Status::Pending, String::new()),
        };
        let is_step = tree
            .get(idx)
            .map(|n| n.kind == NodeKind::Step)
            .unwrap_or(false);

        let step_specific = if is_step && status != Status::Failure && status != Status::Running {
            demo::step_log(&name)
        } else {
            None
        };

        let mut lines: Vec<(String, &'static str)> = match step_specific {
            Some(v) => v,
            None => demo::logs_for(status)
                .iter()
                .map(|(t, k)| (t.to_string(), *k))
                .collect(),
        };

        if status == Status::Running {
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
        let Some(raw) = self.raw_logs.get(&(self.item_repo_key(), self.run_id())) else {
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
}
