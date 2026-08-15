//! The reducer: pane focus, movement, and the key map. Every state change a
//! keystroke can cause starts here.

use crate::shared::key::{Key, Press};

use super::{App, Cmd, Load, NodeKind, Pane, Prompt, View};
use crate::github::data::{Kind, TABS};
use crate::github::demo;
use crate::shared::nav::{Dir, Place};

impl App {
    /// Scrolls the detail body. The real limit is applied by the render, which
    /// is what knows how many lines the content takes at this width.
    pub fn scroll_detail(&mut self, d: i64) {
        self.detail_scroll = (self.detail_scroll as i64 + d).max(0) as usize;
    }

    /// The current view's panes, left to right. This is what `h` and `l` walk.
    pub fn panes(&self) -> Vec<Pane> {
        let mut panes = match self.view {
            View::Logs => vec![Pane::Tree, Pane::Log],
            View::Diff => vec![Pane::Files, Pane::DiffBody],
            View::List if self.tab == crate::github::data::AGENTS_TAB => {
                vec![Pane::Repos, Pane::Agents]
            }
            View::List if self.tab == crate::github::data::FILES_TAB => {
                vec![Pane::Repos, Pane::FileTree, Pane::FileView]
            }
            View::List => vec![Pane::Repos, Pane::List],
            View::Detail => {
                let issue = self
                    .current()
                    .map(|c| c.kind() == Kind::Issue)
                    .unwrap_or(true);
                if issue {
                    vec![Pane::Repos, Pane::Body]
                } else {
                    vec![Pane::Repos, Pane::Body, Pane::Checks]
                }
            }
        };
        // a pane that is not on screen is not a pane you can walk to
        if !self.sidebar_shown {
            panes.retain(|p| *p != Pane::Repos);
        }
        panes
    }

    /// Opens the finder. It starts on repositories, which need no network.
    pub fn open_finder(&mut self) {
        self.finder_open = true;
        self.finder_query.clear();
        self.finder_sent = "\u{0}".into(); // forces the first fetch
        self.finder_sel = 0;
        self.finder_scroll = 0;
        self.finder_hits.clear();
        self.finder_state = Load::Ready;
    }

    /// The finder owns every key while it is up: the letters are the query.
    fn finder_key(&mut self, press: Press) {
        let ctrl = press.ctrl;
        let len = self.finder_len();
        match press.key {
            Key::Esc => self.finder_open = false,
            Key::Enter => self.finder_accept(),
            Key::Tab => self.finder_source_by(Dir::Next),
            Key::BackTab => self.finder_source_by(Dir::Prev),
            Key::Down => self.finder_move(1, len),
            Key::Up => self.finder_move(-1, len),
            Key::Char('n') if ctrl => self.finder_move(1, len),
            Key::Char('p') if ctrl => self.finder_move(-1, len),
            Key::Backspace => {
                self.finder_query.pop();
                self.finder_sel = 0;
                self.finder_scroll = 0;
            }
            Key::Char(c) if !ctrl => {
                self.finder_query.push(c);
                self.finder_sel = 0;
                self.finder_scroll = 0;
            }
            _ => {}
        }
    }

    fn finder_move(&mut self, d: i64, len: usize) {
        if len == 0 {
            return;
        }
        self.finder_sel = (self.finder_sel as i64 + d).rem_euclid(len as i64) as usize;
    }

    fn finder_source_by(&mut self, d: Dir) {
        let all = crate::github::data::Source::ALL;
        let i = all
            .iter()
            .position(|s| *s == self.finder_source)
            .unwrap_or(0) as i64;
        self.finder_source = all[(i + d.step()).rem_euclid(all.len() as i64) as usize];
        self.finder_sel = 0;
        self.finder_scroll = 0;
        self.finder_hits.clear();
        // the query stays: switching source usually means "the same words,
        // somewhere else"
        self.finder_sent = "\u{0}".into();
        self.finder_state = Load::Ready;
    }

    /// Goes wherever the highlighted row lives.
    pub(super) fn finder_accept(&mut self) {
        let Some(hit) = self.finder_results().get(self.finder_sel).cloned() else {
            return;
        };
        self.finder_open = false;

        // move to the repository the hit belongs to, if it is one we know
        if let Some(i) = self
            .repos()
            .iter()
            .position(|r| format!("{}/{}", self.login(), r.name) == hit.repo)
        {
            self.repo = i;
            self.item = 0;
            self.item_scroll = 0;
        }

        match hit.kind {
            crate::github::finder::HitKind::Repo => {
                self.view = View::List;
                self.pane = Pane::List;
            }
            crate::github::finder::HitKind::Commit => {
                // there is no commit view; the repository plus the sha is the
                // most useful place to land
                self.view = View::List;
                self.pane = Pane::List;
                self.flash_ok(format!("{} · {}", hit.repo, hit.detail));
            }
            crate::github::finder::HitKind::Issue | crate::github::finder::HitKind::Pr => {
                self.tab = usize::from(hit.kind == crate::github::finder::HitKind::Pr);
                self.view = View::List;
                self.pane = Pane::List;
                // the list may still be on its way; remember what to select
                if let Some(i) = self
                    .visible()
                    .iter()
                    .position(|&i| self.list()[i].num == hit.num)
                {
                    self.item = i;
                    self.view = View::Detail;
                    self.pane = Pane::Body;
                } else {
                    self.flash_ok(format!("{} #{}", hit.repo, hit.num));
                }
            }
        }
        self.settle_pane();
    }

    /// `[` / `]`: the previous or next repository, without needing the pane
    /// open. Everything the view is about follows the change.
    pub fn step_repo(&mut self, d: Dir) {
        let n = self.repos().len() as i64;
        if n == 0 {
            return;
        }
        let next = (self.repo_idx() as i64 + d.step()).rem_euclid(n) as usize;
        self.repo = next;
        self.item = 0;
        self.item_scroll = 0;
        self.detail_scroll = 0;
        self.view = View::List;
        self.settle_pane();
        self.flash_ok(format!(
            "{}  ({}/{})",
            self.repo_name(),
            next + 1,
            self.repos().len()
        ));
    }

    /// `b`: hides or shows the repository pane. The extra width goes to the
    /// content, which is the point on a narrow terminal.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar = !self.sidebar;
        if !self.sidebar && self.pane == Pane::Repos {
            self.pane = Pane::List;
        }
        self.flash_ok(if self.sidebar {
            "repositories shown [b]"
        } else {
            "repositories hidden [b]"
        });
    }

    /// Moves focus one pane left (`-1`) or right (`1`). `h`/`l` stop at the
    /// edges; `tab` wraps around.
    fn focus_by(&mut self, d: Dir, wrap: bool) {
        let panes = self.panes();
        let n = panes.len() as i64;
        let i = panes.iter().position(|p| *p == self.pane).unwrap_or(0) as i64;
        let j = if wrap {
            (i + d.step()).rem_euclid(n)
        } else {
            (i + d.step()).clamp(0, n - 1)
        };
        self.pane = panes[j as usize];
    }

    /// `x`: opens the dispatch picker over the selected issue or PR.
    pub fn open_dispatch(&mut self) {
        if !self.live() {
            self.flash_warn("dispatching needs live mode — demo data has no issues to send");
            return;
        }
        // Whether there is anything to send is the subject's question, not the
        // item list's: standing in the file explorer there is no item at all.
        if self.dispatch_subject().is_none() {
            self.flash_warn("nothing here to send");
            return;
        }
        // The list is only as fresh as the last look at it, and this is a
        // decision about which agent is free right now.
        self.agents_state = Load::Idle;
        self.dispatch_open = true;
        self.dispatch_note.clear();
        self.dispatch_sel = 0;
        self.dispatch_scroll = 0;
    }

    /// Turns the highlighted destination into a confirmation.
    pub fn dispatch_accept(&mut self) {
        let dests = self.dispatch_dests();
        let Some(dest) = dests.get(self.dispatch_sel).cloned() else {
            return;
        };
        if let Some(why) = dest.refusal() {
            self.flash_warn(why);
            return;
        }
        let Some(subject) = self.dispatch_subject() else {
            return;
        };
        // A file has no number and no item behind it, so what identifies it
        // has to come from the explorer instead.
        let (repo, num, title, url) = if subject == crate::github::subject::Subject::File {
            let repo = self.repo_key();
            let path = self.fs_selected_file().unwrap_or_default();
            let url = format!("https://github.com/{repo}/blob/HEAD/{path}");
            (repo, 0, path, url)
        } else {
            let Some(cur) = self.current() else { return };
            let repo = self.item_repo_key();
            let url = permalink(&repo, cur.kind(), cur.num, cur.id);
            (repo, cur.num, cur.title.clone(), url)
        };
        let context = self.dispatch_context(subject);
        let template = crate::shared::config::with_note(&subject.template(), &self.dispatch_note);
        let text =
            crate::shared::config::render_prompt(&template, &repo, num, &title, &url, &context);

        self.dispatch_open = false;
        match dest {
            crate::github::app::Dest::Running(a) => {
                self.pending_fresh = None;
                self.prompt = Some(Prompt::Dispatch {
                    who: format!("{} in {}", a.kind, a.cwd),
                    pane: a.pane,
                    text,
                });
            }
            crate::github::app::Dest::Fresh {
                kind,
                repo_root,
                in_place,
            } => {
                // In a worktree the branch name is the issue, so a second
                // dispatch of the same one collides loudly instead of quietly
                // making a second worktree nobody asked for. In place there is
                // no branch to make: the agent works on what is checked out.
                let branch = in_place.is_none().then(|| format!("issue-{num}"));
                let who = match &in_place {
                    Some(b) => format!("a new {kind} on {b} in {repo_root}"),
                    None => format!("a new {kind} in a worktree of {repo_root}"),
                };
                self.prompt = Some(Prompt::Dispatch {
                    who,
                    pane: String::new(),
                    text,
                });
                self.pending_fresh = Some(crate::github::app::Fresh {
                    repo_root,
                    branch,
                    label: format!("#{num} {title}"),
                    kind,
                });
            }
            crate::github::app::Dest::NotCloned(repo) => {
                self.flash_warn(format!("{repo} is not cloned here — gh repo clone {repo}"));
            }
        }
    }

    /// The picker owns every key while it is up, because the letters are the
    /// instruction. Moving therefore lives on the arrows and on `^n`/`^p`,
    /// exactly as it does in the finder.
    fn dispatch_key(&mut self, press: Press) {
        let ctrl = press.ctrl;
        let len = self.dispatch_dests().len();
        let last = len.saturating_sub(1);
        match press.key {
            Key::Esc => self.dispatch_open = false,
            Key::Enter => self.dispatch_accept(),
            Key::Down => self.dispatch_sel = (self.dispatch_sel + 1).min(last),
            Key::Up => self.dispatch_sel = self.dispatch_sel.saturating_sub(1),
            Key::Char('n') if ctrl => self.dispatch_sel = (self.dispatch_sel + 1).min(last),
            Key::Char('p') if ctrl => {
                self.dispatch_sel = self.dispatch_sel.saturating_sub(1);
            }
            Key::Backspace => {
                self.dispatch_note.pop();
            }
            Key::Char(c) if !ctrl => self.dispatch_note.push(c),
            _ => {}
        }
    }

    /// `E`: opens the selected file in an editor, cloning the repository first
    /// if it is not here.
    ///
    /// The explorer reads GitHub; an editor reads the disk. Those are two
    /// different files whenever the local checkout is on another branch, which
    /// on this machine is the usual case rather than the exception — so the
    /// mismatch is reported rather than papered over. It is not switched
    /// either: moving someone off their branch to satisfy a keypress would be
    /// a far worse surprise than a warning.
    pub fn open_in_editor(&mut self) {
        if self.tab != crate::github::data::FILES_TAB {
            return;
        }
        let Some(path) = self.fs_selected_file() else {
            self.flash_warn("select a file first");
            return;
        };
        let repo = self.repo_key();

        let Some(root) = self.clone_path(&repo) else {
            if self.clones_state != Load::Ready {
                // Remembered rather than refused: `ensure` sees this and asks
                // for the walk, and the answer comes back here.
                self.wants_edit = true;
                self.flash_ok("looking for a local checkout…");
                return;
            }
            self.wants_edit = false;
            match crate::shared::clones::current().clone_dir() {
                Some(dest) => {
                    self.prompt = Some(Prompt::Clone {
                        repo,
                        dest: dest.to_string_lossy().into_owned(),
                    });
                }
                None => self.flash_warn("nowhere to clone into — set clone-roots in the config"),
            }
            return;
        };

        self.wants_edit = false;
        let full = std::path::Path::new(&root).join(&path);
        if !full.exists() {
            self.flash_warn(format!(
                "{path} is not in the checkout — it is probably on another branch"
            ));
            return;
        }

        // Said before the editor takes the screen, so it is on the status bar
        // when they come back rather than lost behind it.
        if let Some(branch) = crate::shared::clones::current().head_branch(&root) {
            self.flash_warn(format!("editing the copy on {branch}"));
        }
        self.edit_request = Some((full, self.file_sel + 1));
    }

    /// Opens the theme picker, remembering what to go back to.
    pub fn open_themes(&mut self) {
        let current = crate::tui::theme::current();
        self.theme_before = current;
        self.theme_sel = crate::tui::theme::Theme::all()
            .iter()
            .position(|t| *t == current)
            .unwrap_or(0);
        self.themes_open = true;
    }

    /// Applies the highlighted theme straight away: the point of the picker is
    /// to see the interface in it, not to read its name.
    pub(super) fn preview_theme(&mut self) {
        if let Some(t) = crate::tui::theme::Theme::all().get(self.theme_sel) {
            crate::tui::theme::set(*t);
        }
    }

    /// Keeps the previewed theme, and remembers it for the next run.
    ///
    /// A theme that cannot be written is still applied — losing the setting at
    /// the next start is a smaller problem than refusing the one you asked
    /// for — but it says so, because silently forgetting looks like a bug.
    pub fn accept_theme(&mut self) {
        let theme = crate::tui::theme::current();
        match crate::shared::config::save_theme(theme) {
            Ok(()) => self.flash_ok(format!("theme: {}", theme.name())),
            Err(e) => self.flash_warn(format!("theme: {} · not saved: {e}", theme.name())),
        }
    }

    /// `g` / `G`: to the start or the end of the focused pane.
    fn goto(&mut self, to: Place) {
        match self.pane {
            Pane::Body | Pane::Log | Pane::DiffBody if to == Place::Top => {
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
            _ => self.move_by(if to == Place::Top {
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
        self.move_pane_by(self.pane, d);
    }

    /// Moves within a named pane, which need not be the focused one — the
    /// wheel turns whatever the pointer is over without stealing focus.
    pub fn move_pane_by(&mut self, pane: Pane, d: i64) {
        match pane {
            Pane::Repos => {
                let i = step(self.repo, d, self.repos().len());
                self.select_in(pane, i);
            }
            Pane::List => {
                let i = step(self.item, d, self.visible().len());
                self.select_in(pane, i);
            }
            Pane::Checks => {
                let i = step(self.check, d, self.jobs().len());
                self.select_in(pane, i);
            }
            Pane::Tree => {
                let i = step(self.tree_sel, d, self.flat_tree().len());
                self.select_in(pane, i);
            }
            Pane::Files => {
                let i = step(self.file_idx, d, self.diff_files().len());
                self.select_in(pane, i);
            }
            Pane::Agents => {
                let i = step(self.agent_sel, d, self.agents_visible().len());
                self.select_in(pane, i);
            }
            Pane::FileTree => {
                let i = step(self.fs_sel, d, self.fs_rows().len());
                self.select_in(pane, i);
            }
            Pane::FileView => {
                let lines = self.file_lines();
                self.file_sel = step(self.file_sel, d, lines);
            }
            // the panes below hold flowing text rather than entries: there is
            // nothing to select, only somewhere to be
            Pane::Body => self.scroll_detail(d),
            Pane::Log => {
                // moving through the log by hand takes over from follow mode
                self.follow = false;
                self.log_scroll = (self.log_scroll as i64 + d).max(0) as usize;
            }
            Pane::DiffBody => {
                self.diff_scroll = (self.diff_scroll as i64 + d).max(0) as usize;
            }
        }
    }

    /// Puts a pane's selection on entry `i`, with whatever else that implies.
    ///
    /// Selecting is defined once, here, because a keypress and a click have to
    /// mean the same thing: landing on a repository must reset the list under
    /// it either way, or clicking would leave the two out of step.
    pub fn select_in(&mut self, pane: Pane, i: usize) {
        match pane {
            Pane::Repos => {
                self.repo = i;
                self.item = 0;
                self.item_scroll = 0;
                self.view = View::List;
            }
            Pane::List => {
                self.item = i;
                self.detail_scroll = 0;
            }
            Pane::Checks => self.check = i,
            Pane::Tree => {
                self.tree_sel = i;
                self.extra_lines = 0;
                self.log_scroll = 0;
            }
            Pane::Files => {
                self.file_idx = i;
                self.diff_scroll = 0;
            }
            Pane::Agents => self.agent_sel = i,
            Pane::FileTree => {
                self.fs_sel = i;
                // a different file starts at its own top, not where the last
                // one happened to be scrolled to
                self.file_sel = 0;
                self.file_scroll = 0;
            }
            Pane::Body | Pane::Log | Pane::DiffBody | Pane::FileView => {}
        }
    }

    /// Switches to a tab and shows its list.
    pub fn pick_tab(&mut self, i: usize) {
        self.tab = i.min(TABS.len() - 1);
        self.view = View::List;
        self.item = 0;
        self.pane = pane_for_tab(self.tab);
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
    pub(super) fn enter(&mut self) {
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
            // `enter` on a directory opens it; on a file it moves to the
            // contents, which is the only place left to go
            Pane::FileTree => match self.fs_current().map(|e| (e.is_dir, e.path.clone())) {
                Some((true, path)) => {
                    if !self.fs_open.remove(&path) {
                        self.fs_open.insert(path);
                    }
                }
                Some((false, _)) => self.pane = Pane::FileView,
                None => {}
            },
            // an agent is a leaf: there is no deeper view of one here
            Pane::Agents | Pane::Log | Pane::DiffBody | Pane::FileView => {}
        }
    }

    /// `esc` / `q`: leaves the pane, and the view once on the first one.
    pub(super) fn back(&mut self) {
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
        if self.finder_open {
            self.finder_open = false;
            return;
        }
        if self.dispatch_open {
            self.dispatch_open = false;
            return;
        }

        if self.themes_open {
            crate::tui::theme::set(self.theme_before);
            self.themes_open = false;
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
                self.pick_tab(TABS.iter().position(|t| t.id == c).unwrap_or(0));
            }
            "logs" => {
                self.view = View::Logs;
                self.pane = Pane::Tree;
            }
            "diff" | "files" => self.open_diff(0),
            "theme" | "themes" => self.open_themes(),
            "sidebar" | "repos" => self.toggle_sidebar(),
            "find" | "search" => self.open_finder(),
            "help" | "h" => self.help_open = true,
            "q" | "quit" => {
                self.view = View::List;
                self.accounts_open = false;
                self.help_open = false;
                self.themes_open = false;
            }
            _ => {}
        }
        self.cmd = None;
        self.cmd_text.clear();
    }

    pub(super) fn pick_account(&mut self, i: usize) {
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

    pub fn on_key(&mut self, press: Press) {
        // `^l` means redraw everywhere else; it means it here too. Ahead of the
        // pane keys, since plain `l` already moves right.
        if press.ctrl && press.key == Key::Char('l') {
            self.wants_redraw = true;
            return;
        }
        if press.ctrl && press.key == Key::Char('b') {
            self.toggle_sidebar();
            return;
        }
        if press.ctrl && press.key == Key::Char('p') && !self.finder_open {
            self.open_finder();
            return;
        }

        // half a page up/down in the focused pane, vim style
        if press.ctrl && matches!(self.pane, Pane::Body | Pane::Log) {
            let half = (self.detail_height / 2).max(1) as i64;
            match press.key {
                Key::Char('d') => return self.move_by(half),
                Key::Char('u') => return self.move_by(-half),
                _ => {}
            }
        }

        // Actually quitting the program (the design lives in a browser).
        if press.ctrl && matches!(press.key, Key::Char('c' | 'd')) {
            self.should_quit = true;
            return;
        }

        // A pending confirmation swallows every key.
        if let Some(prompt) = self.prompt.clone() {
            match press.key {
                Key::Enter | Key::Char('y') => self.confirm(),
                Key::Esc | Key::Char('n' | 'q') => self.cancel_prompt(),
                Key::Char('j') | Key::Down => {
                    if let Prompt::Merge(m) = prompt {
                        self.prompt = Some(Prompt::Merge((m + 1).min(2)));
                    }
                }
                Key::Char('k') | Key::Up => {
                    if let Prompt::Merge(m) = prompt {
                        self.prompt = Some(Prompt::Merge(m.saturating_sub(1)));
                    }
                }
                Key::Char(c @ '1'..='3') => {
                    if matches!(prompt, Prompt::Merge(_)) {
                        self.prompt = Some(Prompt::Merge(c as usize - '1' as usize));
                    }
                }
                _ => {}
            }
            return;
        }

        if let Some(mode) = self.cmd {
            match press.key {
                Key::Esc => {
                    self.cmd = None;
                    self.cmd_text.clear();
                }
                Key::Enter => {
                    if mode == Cmd::Colon {
                        let t = self.cmd_text.clone();
                        self.run_cmd(&t);
                    } else {
                        self.cmd = None;
                    }
                }
                Key::Backspace => {
                    self.cmd_text.pop();
                    self.sync_filter(mode);
                }
                Key::Char(c) => {
                    self.cmd_text.push(c);
                    self.sync_filter(mode);
                }
                _ => {}
            }
            return;
        }

        if self.finder_open {
            self.finder_key(press);
            return;
        }

        if self.dispatch_open {
            self.dispatch_key(press);
            return;
        }

        if self.themes_open {
            let last = crate::tui::theme::Theme::all().len() - 1;
            match press.key {
                Key::Char('j') | Key::Down => {
                    self.theme_sel = (self.theme_sel + 1).min(last);
                    self.preview_theme();
                }
                Key::Char('k') | Key::Up => {
                    self.theme_sel = self.theme_sel.saturating_sub(1);
                    self.preview_theme();
                }
                Key::Enter => {
                    self.themes_open = false;
                    self.accept_theme();
                }
                Key::Esc | Key::Char('q' | 't') => {
                    // the picker previews as you move, so leaving it puts back
                    // whatever was on when it opened
                    crate::tui::theme::set(self.theme_before);
                    self.themes_open = false;
                }
                _ => {}
            }
            return;
        }

        if self.accounts_open {
            match press.key {
                Key::Char('j') | Key::Down => {
                    self.acc_sel = (self.acc_sel + 1).min(self.accounts.len() - 1);
                }
                Key::Char('k') | Key::Up => self.acc_sel = self.acc_sel.saturating_sub(1),
                Key::Enter => self.pick_account(self.acc_sel),
                Key::Esc | Key::Char('q' | 'a') => {
                    self.accounts_open = false;
                }
                _ => {}
            }
            return;
        }

        if self.help_open {
            match press.key {
                Key::Esc | Key::Char('q') => self.help_open = false,
                Key::Char('?') => self.help_open = false,
                _ => {}
            }
            return;
        }

        match press.key {
            Key::Char('j') | Key::Down => self.move_by(1),
            Key::Char('k') | Key::Up => self.move_by(-1),
            Key::Char('h') | Key::Left => self.focus_by(Dir::Prev, false),
            Key::Char('l') | Key::Right => self.focus_by(Dir::Next, false),
            Key::Tab => self.focus_by(Dir::Next, true),
            Key::BackTab => self.focus_by(Dir::Prev, true),
            Key::Char('g') => self.goto(Place::Top),
            Key::Char('G') => self.goto(Place::Bottom),
            Key::Enter => self.enter(),
            Key::Esc | Key::Char('q') => self.back(),
            Key::Char('a') => {
                self.accounts_open = true;
                self.acc_sel = self.acc;
            }
            Key::Char('b') => self.toggle_sidebar(),
            Key::Char('p') => self.open_finder(),
            Key::Char('[') => self.step_repo(Dir::Prev),
            Key::Char(']') => self.step_repo(Dir::Next),
            Key::Char('t') => self.open_themes(),
            Key::Char('?') => self.help_open = true,
            Key::Char(':') => {
                self.cmd = Some(Cmd::Colon);
                self.cmd_text.clear();
            }
            Key::Char('/') => {
                self.cmd = Some(Cmd::Slash);
                self.cmd_text = if self.view == View::Logs {
                    self.log_filter.clone()
                } else {
                    self.filter.clone()
                };
            }
            Key::PageDown => self.page_by(1),
            Key::PageUp => self.page_by(-1),
            Key::Char('d') if self.actionable_pr() && self.view != View::Diff => {
                self.open_diff(0);
            }
            Key::Char('s') if self.view == View::Diff => {
                self.split = !self.split;
                self.diff_scroll = 0;
            }
            Key::Char('w') if self.view == View::Diff => {
                self.ws = !self.ws;
                self.diff_scroll = 0;
            }
            Key::Char('f') => self.follow = !self.follow,
            Key::Char('r') => {
                self.tick += 1;
                self.extra_lines = 0;
                self.refresh();
            }
            Key::Char(c @ '1'..='5') => {
                self.tab = c as usize - '1' as usize;
                self.view = View::List;
                self.item = 0;
                self.item_scroll = 0;
                self.check = 0;
                // the Agents tab has its own pane, so landing on `List` would
                // leave the focus on something that is not drawn
                self.pane = pane_for_tab(self.tab);
            }
            Key::Char('x') => self.open_dispatch(),
            Key::Char('E') => self.open_in_editor(),
            Key::Char('o') if self.pane == Pane::FileTree => {
                if let Some((true, path)) = self.fs_current().map(|e| (e.is_dir, e.path.clone()))
                    && !self.fs_open.remove(&path)
                {
                    self.fs_open.insert(path);
                }
            }
            Key::Char('o') if self.view == View::Logs => {
                let tree = self.flat_tree();
                if let Some(node) = tree.get(self.tree_sel_idx(tree.len())) {
                    let ji = node.ji;
                    if !self.collapsed.remove(&ji) {
                        self.collapsed.insert(ji);
                    }
                }
            }
            Key::Char('e') if self.view == View::Logs => {
                if let Some(i) = self
                    .log_lines()
                    .iter()
                    .position(|l| l.kind == crate::github::data::LogKind::Error)
                {
                    self.log_scroll = i.saturating_sub(3);
                    self.follow = false;
                }
            }
            // --- actions on the selected pull request
            Key::Char('m') if self.actionable_pr() => self.ask_merge(),
            Key::Char('c') if self.actionable_pr() => self.ask_close(),
            // `d` opens the diff (as it does in the design), so deleting a
            // branch, which is destructive, lives on the shifted key
            Key::Char('D') if self.actionable_pr() => self.ask_delete_branch(),
            Key::Char(k @ ('m' | 'c' | 'D')) => {
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
        self.poll_agents();
        self.tick = self.tick.wrapping_add(1);
    }
}

/// The pane a tab lands on. Two of the five are not the item list.
fn pane_for_tab(tab: usize) -> Pane {
    match tab {
        crate::github::data::AGENTS_TAB => Pane::Agents,
        crate::github::data::FILES_TAB => Pane::FileTree,
        _ => Pane::List,
    }
}

/// Where a thing lives on github.com.
///
/// A run is addressed by its database id under `/actions/runs`, not by the
/// number the list shows — the number is per workflow and would resolve to
/// whatever run happens to share it in another workflow.
fn permalink(repo: &str, kind: crate::github::data::Kind, num: i64, id: i64) -> String {
    match kind {
        crate::github::data::Kind::Issue => format!("https://github.com/{repo}/issues/{num}"),
        crate::github::data::Kind::Pr => format!("https://github.com/{repo}/pull/{num}"),
        crate::github::data::Kind::Run => format!("https://github.com/{repo}/actions/runs/{id}"),
    }
}

/// Moves an index by `d` within `len`, staying inside it. An empty list has
/// nowhere to go, and index 0 is the honest answer for it.
fn step(current: usize, d: i64, len: usize) -> usize {
    (current as i64 + d).clamp(0, (len as i64 - 1).max(0)) as usize
}

/// Drops the +/- pairs whose contents differ only in whitespace, which is what
/// "ignore whitespace" is expected to do.
pub(super) fn strip_ws_only(h: &crate::github::data::Hunk) -> crate::github::data::Hunk {
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
    crate::github::data::Hunk {
        hdr: h.hdr.clone(),
        lines,
    }
}

/// The text with no whitespace, so it can be compared ignoring it.
fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
