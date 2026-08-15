//! Read-only queries over the state. Nothing here mutates; the view and the
//! reducer both go through these so the indexing rules live in one place.

use super::input::strip_ws_only;
use super::{App, Load, LogRow, NodeKind, Pane, TreeNode, View};
use crate::github::data::{self, Account, Item, Job, Kind, LogLine, Repo, Status};
use crate::github::demo;

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
        self.repo()
            .map(crate::github::data::Repo::label)
            .unwrap_or("—")
    }

    /// The `owner/repo` key that indexes lists, jobs and logs.
    pub fn repo_key(&self) -> String {
        format!("{}/{}", self.login(), self.repo_name())
    }

    /// Is the pseudo-repository that gathers every repository selected?
    pub fn is_all(&self) -> bool {
        self.repo().is_some_and(crate::github::data::Repo::is_all)
    }

    // --- the file explorer ---

    pub fn repo_tree(&self) -> &[crate::github::data::TreeEntry] {
        self.trees
            .get(&self.repo_key())
            .map(std::vec::Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn tree_state(&self) -> Load {
        self.trees_state
            .get(&self.repo_key())
            .cloned()
            .unwrap_or(Load::Idle)
    }

    /// The entries on screen: everything whose directories have all been
    /// opened, or everything matching the filter when there is one.
    ///
    /// A filter flattens the tree on purpose. Someone typing `parser` wants
    /// the file, not the three directories they would have to open to reach
    /// it, and the path is shown in full so the location is not lost.
    pub fn fs_rows(&self) -> Vec<&crate::github::data::TreeEntry> {
        let filtering = !self.filter.trim().is_empty();
        self.repo_tree()
            .iter()
            .filter(|e| {
                if filtering {
                    !e.is_dir && self.matches(&e.path)
                } else {
                    e.ancestors().iter().all(|a| self.fs_open.contains(*a))
                }
            })
            .collect()
    }

    pub fn fs_idx(&self) -> usize {
        self.fs_sel.min(self.fs_rows().len().saturating_sub(1))
    }

    pub fn fs_current(&self) -> Option<&crate::github::data::TreeEntry> {
        let rows = self.fs_rows();
        rows.get(self.fs_idx()).copied()
    }

    /// The path under the cursor, if it is a file worth fetching.
    pub fn fs_selected_file(&self) -> Option<String> {
        let e = self.fs_current()?;
        (!e.is_dir && e.size <= crate::github::gh::FILE_LIMIT).then(|| e.path.clone())
    }

    /// Colour spans for the open file, if its language is one we know.
    pub fn file_spans(&self) -> Option<&Vec<Vec<crate::shared::syntax::Span>>> {
        let e = self.fs_current()?;
        self.file_spans.get(&(self.repo_key(), e.path.clone()))
    }

    /// How many lines the open file has.
    pub fn file_lines(&self) -> usize {
        self.file_body().map(|t| t.lines().count()).unwrap_or(0)
    }

    /// The selected file's contents, or why they are not here.
    pub fn file_body(&self) -> Result<&str, Load> {
        let Some(e) = self.fs_current() else {
            return Err(Load::Ready);
        };
        if e.is_dir {
            return Err(Load::Ready);
        }
        if e.size > crate::github::gh::FILE_LIMIT {
            // A refusal, not an error: nothing ran and failed, this program
            // declined — and it will decline the same way next time, which is
            // what `is_transient` now says without being asked to guess.
            return Err(Load::refused(format!(
                "{} is {} KB — too large to open here",
                e.name(),
                e.size / 1024
            )));
        }
        let key = (self.repo_key(), e.path.clone());
        match self.file_text.get(&key) {
            Some(text) => Ok(text),
            None => Err(self.file_state.get(&key).cloned().unwrap_or(Load::Idle)),
        }
    }

    /// What `x` would send from here.
    ///
    /// Decided by where the reader is, not chosen from a menu: `x` acts on the
    /// pane you are in, the same as every other key. Standing in a log means
    /// the log, standing in a diff means that file.
    pub fn dispatch_subject(&self) -> Option<crate::github::subject::Subject> {
        use crate::github::subject::Subject;
        // Checked before the selection, because the explorer has no issue or
        // pull request behind it — the file itself is the subject.
        if self.tab == crate::github::data::FILES_TAB && self.view == View::List {
            return self.fs_selected_file().map(|_| Subject::File);
        }
        let cur = self.current()?;
        Some(match self.view {
            View::Logs => Subject::Run,
            View::Diff => Subject::FileDiff,
            _ => match cur.kind() {
                Kind::Issue => Subject::Issue,
                Kind::Pr => Subject::Pr,
                Kind::Run => Subject::Run,
            },
        })
    }

    /// The body of what is being sent: the part that is not the title, the
    /// number or the link.
    pub fn dispatch_context(&self, subject: crate::github::subject::Subject) -> String {
        use crate::github::subject::Subject;
        match subject {
            Subject::Issue => self
                .current()
                .map(|c| c.body_text().to_string())
                .unwrap_or_default(),

            Subject::Pr => {
                let files: Vec<(String, String, String)> = self
                    .diff_files()
                    .iter()
                    .map(|f| (f.path.clone(), f.add.clone(), f.del.clone()))
                    .collect();
                let body = self
                    .current()
                    .map(crate::github::data::Item::body_text)
                    .unwrap_or_default();
                let summary = crate::github::subject::files_summary(&files);
                if body.trim().is_empty() {
                    summary
                } else {
                    format!("{body}\n\n---\n\n{summary}")
                }
            }

            Subject::Run => {
                let rows = self.log_lines();
                let lines: Vec<crate::github::subject::Line<'_>> = rows
                    .iter()
                    .map(|r| crate::github::subject::Line {
                        text: &r.text,
                        is_error: r.kind == crate::github::data::LogKind::Error,
                    })
                    .collect();
                let excerpt = crate::github::subject::log_excerpt(&lines);
                match self.log_job_label() {
                    Some(job) => format!("job: {job}\n\n{excerpt}"),
                    None => excerpt,
                }
            }

            Subject::File => match (self.fs_current(), self.file_body()) {
                (Some(e), Ok(text)) => {
                    let lines: Vec<&str> = text.lines().collect();
                    let mut out = format!("{}\n\n", e.path);
                    for l in lines.iter().take(600) {
                        out.push_str(l);
                        out.push('\n');
                    }
                    if lines.len() > 600 {
                        out.push_str(&format!("\n… 600 of {} lines shown\n", lines.len()));
                    }
                    out
                }
                _ => "(the file has not loaded yet)".to_string(),
            },

            Subject::FileDiff => match self.diff_file() {
                Some(f) => {
                    let rows = self.diff_rows();
                    let mut out = format!("{}  {}/{}\n\n", f.path, f.add, f.del);
                    for r in rows.iter().take(400) {
                        out.push_str(&r.text);
                        out.push('\n');
                    }
                    if rows.len() > 400 {
                        out.push_str(&format!("\n… {} of {} lines shown\n", 400, rows.len()));
                    }
                    out
                }
                None => "(no file selected)".to_string(),
            },
        }
    }

    /// The job, and step, whose log is on screen.
    fn log_job_label(&self) -> Option<String> {
        let tree = self.flat_tree();
        let node = tree.get(self.tree_sel_idx(tree.len()))?;
        let job = self.jobs().get(node.ji)?.name.clone();
        Some(match node.kind {
            NodeKind::Step => format!("{job} › {}", node.name),
            NodeKind::Job => job,
        })
    }

    /// Everywhere the selected issue could go, running agents first.
    ///
    /// Ordered by what you are most likely to want: an agent that is free, an
    /// agent that is busy, then the fresh worktrees. The refused ones stay in
    /// the list because knowing *why* nothing is available beats an empty box.
    pub fn dispatch_dests(&self) -> Vec<crate::github::app::Dest> {
        use crate::github::app::Dest;

        let mut out: Vec<Dest> = self.agents.iter().cloned().map(Dest::Running).collect();
        out.sort_by_key(|d| u8::from(d.refusal().is_some()));

        // Then the fresh worktrees, which need somewhere to branch from. The
        // three outcomes of the plan are exactly these two arms plus the
        // agents above: open in herdr, cloned here, or nowhere at all.
        let repo = self.item_repo_key();
        if let Some(root) = self.clone_path(&repo) {
            let kinds = crate::shared::config::agent_kinds();
            for kind in &kinds {
                out.push(Dest::Fresh {
                    kind: kind.clone(),
                    repo_root: root.clone(),
                    in_place: None,
                });
            }
            // Working in the checkout comes last: it is the one that can
            // collide with whatever the reader has open, so it should be
            // chosen rather than landed on.
            if let Some(branch) = crate::shared::clones::head_branch(&root) {
                for kind in &kinds {
                    out.push(Dest::Fresh {
                        kind: kind.clone(),
                        repo_root: root.clone(),
                        in_place: Some(branch.clone()),
                    });
                }
            }
        } else if self.clones_state == Load::Ready {
            out.push(Dest::NotCloned(repo));
        }
        // otherwise the disk is still being walked: offering nothing beats
        // offering a lie about what is here

        out
    }

    /// Where a repository is checked out, if it is.
    pub fn clone_path(&self, repo: &str) -> Option<String> {
        self.clones
            .get(repo)
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// The agents worth showing under the current selection.
    ///
    /// On a single repository, only the agents working inside it — an agent in
    /// some other project is not what you came to this repository to see. Under
    /// the gathering row, all of them. Matched on the path ending in the
    /// repository's name, which is what a checkout looks like and also what a
    /// worktree of it looks like.
    pub fn agents_visible(&self) -> Vec<&crate::shared::mux::Agent> {
        let all = self.is_all();
        let name = self.repo_name().to_string();
        self.agents
            .iter()
            .filter(|a| all || a.cwd.split('/').any(|seg| seg == name))
            .filter(|a| self.matches(&a.title) || self.matches(&a.cwd) || self.matches(&a.kind))
            .collect()
    }

    pub fn agent_idx(&self) -> usize {
        self.agent_sel
            .min(self.agents_visible().len().saturating_sub(1))
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
    pub fn diff_files(&self) -> &[crate::github::data::FileChange] {
        self.current().map_or(&[], Item::files)
    }

    pub fn file_idx(&self) -> usize {
        self.file_idx.min(self.diff_files().len().saturating_sub(1))
    }

    /// The rows the finder is showing. Repositories are ranked here; the rest
    /// arrive already ranked by GitHub.
    pub fn finder_results(&self) -> Vec<crate::github::finder::Hit> {
        if !self.finder_source.is_local() {
            return self.finder_hits.clone();
        }
        let repos = self.repos();
        crate::shared::fuzzy::rank(&self.finder_query, repos, |r| r.label())
            .into_iter()
            .map(|(i, _)| {
                let r = &repos[i];
                crate::github::finder::Hit {
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
                    kind: crate::github::finder::HitKind::Repo,
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

    pub fn diff_file(&self) -> Option<&crate::github::data::FileChange> {
        self.diff_files().get(self.file_idx())
    }

    /// Lines of the selected file, numbered and with `w` already applied.
    pub fn diff_rows(&self) -> Vec<crate::github::data::DiffRow> {
        let Some(f) = self.diff_file() else {
            return Vec::new();
        };
        let hunks: Vec<crate::github::data::Hunk> = if self.ws {
            f.hunks.iter().map(strip_ws_only).collect()
        } else {
            f.hunks.clone()
        };
        crate::github::data::Hunk::rows(&hunks)
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

        let mut lines: Vec<(String, crate::github::data::LogKind)> = match step_specific {
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
