//! The reducer: pane focus, movement, and the key map. Every state change a
//! keystroke can cause starts here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Cmd, Load, NodeKind, Pane, Prompt, View};
use crate::data::{Kind, TABS};
use crate::demo;

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
    fn finder_key(&mut self, ev: KeyEvent) {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
        let len = self.finder_len();
        match ev.code {
            KeyCode::Esc => self.finder_open = false,
            KeyCode::Enter => self.finder_accept(),
            KeyCode::Tab => self.finder_source_by(1),
            KeyCode::BackTab => self.finder_source_by(-1),
            KeyCode::Down => self.finder_move(1, len),
            KeyCode::Up => self.finder_move(-1, len),
            KeyCode::Char('n') if ctrl => self.finder_move(1, len),
            KeyCode::Char('p') if ctrl => self.finder_move(-1, len),
            KeyCode::Backspace => {
                self.finder_query.pop();
                self.finder_sel = 0;
                self.finder_scroll = 0;
            }
            KeyCode::Char(c) if !ctrl => {
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

    fn finder_source_by(&mut self, d: i64) {
        let all = crate::finder::Source::ALL;
        let i = all
            .iter()
            .position(|s| *s == self.finder_source)
            .unwrap_or(0) as i64;
        self.finder_source = all[(i + d).rem_euclid(all.len() as i64) as usize];
        self.finder_sel = 0;
        self.finder_scroll = 0;
        self.finder_hits.clear();
        // the query stays: switching source usually means "the same words,
        // somewhere else"
        self.finder_sent = "\u{0}".into();
        self.finder_state = Load::Ready;
    }

    /// Goes wherever the highlighted row lives.
    fn finder_accept(&mut self) {
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
            crate::finder::HitKind::Repo => {
                self.view = View::List;
                self.pane = Pane::List;
            }
            crate::finder::HitKind::Commit => {
                // there is no commit view; the repository plus the sha is the
                // most useful place to land
                self.view = View::List;
                self.pane = Pane::List;
                self.flash_ok(format!("{} · {}", hit.repo, hit.detail));
            }
            crate::finder::HitKind::Issue | crate::finder::HitKind::Pr => {
                self.tab = usize::from(hit.kind == crate::finder::HitKind::Pr);
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
    pub fn step_repo(&mut self, d: i64) {
        let n = self.repos().len() as i64;
        if n == 0 {
            return;
        }
        let next = (self.repo_idx() as i64 + d).rem_euclid(n) as usize;
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

    /// Opens the theme picker, remembering what to go back to.
    pub fn open_themes(&mut self) {
        let current = crate::theme::current();
        self.theme_before = current;
        self.theme_sel = crate::theme::Theme::ALL
            .iter()
            .position(|t| *t == current)
            .unwrap_or(0);
        self.themes_open = true;
    }

    /// Applies the highlighted theme straight away: the point of the picker is
    /// to see the interface in it, not to read its name.
    fn preview_theme(&mut self) {
        if let Some(t) = crate::theme::Theme::ALL.get(self.theme_sel) {
            crate::theme::set(*t);
        }
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
        if self.finder_open {
            self.finder_open = false;
            return;
        }

        if self.themes_open {
            crate::theme::set(self.theme_before);
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
        if ev.modifiers.contains(KeyModifiers::CONTROL) && ev.code == KeyCode::Char('b') {
            self.toggle_sidebar();
            return;
        }
        if ev.modifiers.contains(KeyModifiers::CONTROL)
            && ev.code == KeyCode::Char('p')
            && !self.finder_open
        {
            self.open_finder();
            return;
        }

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

        if self.finder_open {
            self.finder_key(ev);
            return;
        }

        if self.themes_open {
            let last = crate::theme::Theme::ALL.len() - 1;
            match ev.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.theme_sel = (self.theme_sel + 1).min(last);
                    self.preview_theme();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.theme_sel = self.theme_sel.saturating_sub(1);
                    self.preview_theme();
                }
                KeyCode::Enter => {
                    self.themes_open = false;
                    self.flash_ok(format!("theme: {}", crate::theme::current().name()));
                }
                KeyCode::Esc | KeyCode::Char('q' | 't') => {
                    // the picker previews as you move, so leaving it puts back
                    // whatever was on when it opened
                    crate::theme::set(self.theme_before);
                    self.themes_open = false;
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
            KeyCode::Char('b') => self.toggle_sidebar(),
            KeyCode::Char('p') => self.open_finder(),
            KeyCode::Char('[') => self.step_repo(-1),
            KeyCode::Char(']') => self.step_repo(1),
            KeyCode::Char('t') => self.open_themes(),
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
pub(super) fn strip_ws_only(h: &crate::data::Hunk) -> crate::data::Hunk {
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
