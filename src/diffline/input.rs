//! The keymap, and every state change a keystroke can cause.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, FinderTab, Hit, Load, Modal, Pane, Pending, first_code};
use super::keys::{self, Action};
use super::model::{Comment, Kind, State};
use super::service::{Request, Write};
use crate::nav::{Dir, Place};

/// Which way `w`, `b` and `e` go, and how far into the word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Word {
    Forward,
    End,
    Back,
}

/// The commands the palette offers, and the key each is also on.
pub const COMMANDS: &[(&str, &str)] = &[
    ("toggle blame", "␣b"),
    ("toggle blast radius", "␣g"),
    ("expand context", "␣+"),
    ("collapse context", "␣-"),
    ("next file", "]f"),
    ("prev file", "[f"),
    ("add comment", "␣n"),
    ("delete comment under cursor", "␣x"),
    ("pick agent", "␣a"),
    ("send queue to agent", "␣s"),
    ("clear queue", ""),
    ("next scope", "]s"),
    ("prev scope", "[s"),
    ("split view", "␣v"),
    ("pick a theme", "␣t"),
    ("write a keymap to start from", ""),
    ("write a theme to start from", ""),
    ("refresh", "␣r"),
    ("open help", "␣?"),
    ("quit", "␣q"),
];

impl App {
    // --- moving ---

    /// The panes you can actually walk to. A pane that is not on screen is
    /// not one of them.
    fn panes(&self) -> Vec<Pane> {
        let mut v = vec![Pane::Diff];
        if self.tree_shown {
            v.insert(0, Pane::Tree);
        }
        if self.queue_shown {
            v.push(Pane::Queue);
        }
        v
    }

    fn focus_by(&mut self, d: i64) {
        let panes = self.panes();
        let i = panes.iter().position(|p| *p == self.pane).unwrap_or(0) as i64;
        let j = (i + d).clamp(0, panes.len() as i64 - 1) as usize;
        self.pane = panes[j];
    }

    /// Shows or hides the review queue. The mouse's way in, since the tab
    /// that says how many are queued is also the thing you click to open it.
    pub fn toggle_queue_pane(&mut self) {
        self.toggle_pane(Pane::Queue);
    }

    /// Takes whatever a modal has highlighted, as `enter` would.
    pub fn accept_modal(&mut self) {
        if let Some(m) = self.modal {
            self.accept(m);
        }
    }

    /// Puts the cursor on a row a click landed on, if it is one that can hold
    /// one — a hunk header is a coordinate, not a line.
    pub fn click_row(&mut self, i: usize) {
        let rows = self.diff_rows();
        if rows.get(i).is_some_and(|r| r.kind.is_code()) {
            self.cursor = i;
            self.anchor = None;
        }
    }

    /// Opens or closes a side pane.
    ///
    /// Opening moves focus into it, because that is what asking for it meant.
    /// Closing hands focus back to the code rather than leaving it on a pane
    /// that is no longer drawn.
    fn toggle_pane(&mut self, which: Pane) {
        let shown = match which {
            Pane::Tree => &mut self.tree_shown,
            Pane::Queue => &mut self.queue_shown,
            Pane::Diff => return,
        };
        // Three states, not two. Hidden means show it and go there; shown but
        // not focused means go there — hiding a pane you were only looking at
        // is never what the key was for; shown and focused means put it away.
        if !*shown {
            *shown = true;
            self.pane = which;
        } else if self.pane != which {
            self.pane = which;
        } else {
            *shown = false;
            self.pane = Pane::Diff;
        }
    }

    /// `j`/`k` on whichever pane has focus.
    pub fn move_by(&mut self, d: i64) {
        match self.pane {
            Pane::Tree => {
                let n = self.files.len();
                self.goto_file(step(self.file_idx, d, n));
            }
            // One step at a time, `d` of them. Passing `d` straight to
            // `move_cursor` made it hunt for a code line in strides of `d`,
            // so a `5j` that landed on a hunk header went five further —
            // ten lines for one keystroke — and a half-page in a short file
            // stepped clean past the end and refused to move at all.
            Pane::Diff => {
                let step = d.signum();
                for _ in 0..d.abs() {
                    let before = self.cursor;
                    self.move_cursor(step);
                    if self.cursor == before {
                        break;
                    }
                }
            }
            Pane::Queue => {
                self.queue_sel = step(self.queue_sel, d, self.comments.len());
            }
        }
    }

    /// Moves the diff cursor, stepping over hunk headers.
    ///
    /// A header is a coordinate rather than a line: landing on one would offer
    /// to comment on `@@ -14,7 +14,9 @@`, which is not something to say.
    fn move_cursor(&mut self, d: i64) {
        let rows = self.diff_rows();
        if rows.is_empty() {
            return;
        }
        let last = rows.len() as i64 - 1;
        let mut c = self.cursor as i64;

        // Walks until it finds a line, and stays put if there is none that
        // way. Clamping to the edge instead would park the cursor on the
        // hunk header above the first line, which is the one row it must
        // never sit on — there is nothing there to comment about.
        for _ in 0..rows.len().max(1) {
            c += d;
            if c < 0 || c > last {
                return;
            }
            if rows[c as usize].kind.is_code() {
                self.cursor = c as usize;
                return;
            }
        }
    }

    // --- motions ---
    //
    // Everything here moves and nothing here acts, which is the division the
    // keymap is built on: the plain keys navigate, the leader commands.

    /// The nth code row, counting from one as `:42` does.
    fn goto_line(&mut self, n: usize) {
        let rows = self.diff_rows();
        let code: Vec<usize> = (0..rows.len())
            .filter(|i| rows[*i].kind.is_code())
            .collect();
        if code.is_empty() {
            return;
        }
        let i = n.saturating_sub(1).min(code.len() - 1);
        self.cursor = code[i];
    }

    /// `H`, `M`, `L`: the top, middle and bottom of what is on screen.
    ///
    /// Of the window, not of the file — that is the whole point of them, and
    /// it is why the render has to hand back the height it used.
    fn goto_screen(&mut self, where_to: Place) {
        let rows = self.diff_rows().len();
        if rows == 0 {
            return;
        }
        let top = self.diff_scroll;
        let bottom = (top + self.view_height.saturating_sub(1)).min(rows - 1);
        let target = match where_to {
            Place::Top => top,
            Place::Middle => top + (bottom - top) / 2,
            Place::Bottom => bottom,
        };
        self.cursor = target.min(rows - 1);
        // A header is not a line you can sit on, so slide off it.
        if !self.diff_rows()[self.cursor].kind.is_code() {
            self.move_cursor(1);
        }
    }

    /// `{` and `}`: the previous or next hunk header, vim's paragraph motion
    /// read onto a diff — a hunk is what a paragraph is here.
    fn hunk(&mut self, d: i64) {
        let rows = self.diff_rows();
        let mut i = self.cursor as i64;
        loop {
            i += d;
            if i < 0 || i >= rows.len() as i64 {
                // the ends are still somewhere to go, as they are in vim
                self.cursor = if d > 0 {
                    rows.len().saturating_sub(1)
                } else {
                    0
                };
                let rows = self.diff_rows();
                if !rows.is_empty() && !rows[self.cursor].kind.is_code() {
                    self.move_cursor(if d > 0 { -1 } else { 1 });
                }
                return;
            }
            if rows[i as usize].kind == Kind::Header {
                self.cursor = i as usize;
                self.move_cursor(1);
                return;
            }
        }
    }

    /// `[c` and `]c`: the previous or next run of changed lines.
    ///
    /// A run, not a line: twelve deleted lines in a row are one change, and
    /// stopping on each of them would make the motion useless on exactly the
    /// diffs it is for.
    fn change(&mut self, d: i64) {
        let rows = self.diff_rows();
        let changed = |i: usize| matches!(rows[i].kind, Kind::Added | Kind::Deleted);
        let mut i = self.cursor as i64;
        let mut left_current = !changed(self.cursor.min(rows.len().saturating_sub(1)));
        loop {
            i += d;
            if i < 0 || i >= rows.len() as i64 {
                return;
            }
            let here = changed(i as usize);
            if !here {
                left_current = true;
            } else if left_current {
                // walk back to the first line of the run when arriving from
                // below, so `[c` lands on its start rather than its end
                let mut j = i;
                if d < 0 {
                    while j > 0 && changed((j - 1) as usize) {
                        j -= 1;
                    }
                }
                self.cursor = j as usize;
                return;
            }
        }
    }

    /// `zz`, `zt`, `zb`: move the window rather than the cursor.
    fn scroll_cursor_to(&mut self, where_to: Place) {
        let h = self.view_height.max(1);
        self.diff_scroll = match where_to {
            Place::Top => self.cursor,
            Place::Middle => self.cursor.saturating_sub(h / 2),
            Place::Bottom => self.cursor.saturating_sub(h.saturating_sub(1)),
        };
    }

    /// The text the cursor is on, which is what the horizontal motions walk.
    fn cursor_text(&self) -> String {
        self.diff_rows()
            .get(self.cursor)
            .map(|r| r.text.clone())
            .unwrap_or_default()
    }

    /// `w`, `b`, `e`. The diff has no editable column, so these move the
    /// window by words rather than a caret between them — which is what you
    /// wanted them for here anyway: getting to the far end of a long line.
    fn word(&mut self, motion: Word) {
        use unicode_width::UnicodeWidthChar;
        let text = self.cursor_text();
        let mut cols: Vec<usize> = Vec::new();
        let mut col = 0usize;
        let mut prev_sep = true;
        let to_end = motion == Word::End;
        for c in text.chars() {
            let sep = !c.is_alphanumeric() && c != '_';
            if to_end {
                if !sep && col > 0 {
                    cols.push(col);
                }
            } else if !sep && prev_sep {
                cols.push(col);
            }
            prev_sep = sep;
            col += UnicodeWidthChar::width(c).unwrap_or(0);
        }
        let here = self.hscroll;
        let next = if motion != Word::Back {
            cols.into_iter().find(|c| *c > here)
        } else {
            cols.into_iter().rfind(|c| *c < here)
        };
        self.hscroll = next.unwrap_or(if motion == Word::Back {
            0
        } else {
            self.longest_line()
        });
    }

    /// `^`: the first character that is not a space.
    fn first_non_blank(&mut self) {
        let text = self.cursor_text();
        self.hscroll = text.len() - text.trim_start().len();
    }

    /// `n` and `N`: the next or previous line matching what `/` last asked
    /// for. Case-insensitive, as vim is with `ignorecase`, and it wraps.
    fn search(&mut self, d: i64) {
        if self.last_search.is_empty() {
            self.flash("no previous search");
            return;
        }
        let needle = self.last_search.to_lowercase();
        let rows = self.diff_rows();
        let n = rows.len();
        if n == 0 {
            return;
        }
        for step in 1..=n {
            let i = (self.cursor as i64 + d * step as i64).rem_euclid(n as i64) as usize;
            if rows[i].kind.is_code() && rows[i].text.to_lowercase().contains(&needle) {
                self.cursor = i;
                self.flash(format!("/{}", self.last_search));
                return;
            }
        }
        self.flash(format!("no match for {}", self.last_search));
    }

    pub fn goto_file(&mut self, i: usize) {
        if self.files.is_empty() {
            return;
        }
        self.file_idx = i.min(self.files.len() - 1);
        self.cursor = first_code(self.diff_rows(), 0);
        self.anchor = None;
        self.diff_scroll = 0;
        // A new file starts at the left, the way opening one in an editor does
        self.hscroll = 0;
    }

    /// `n` / `p`: round the files, so the last leads back to the first.
    pub fn step_file(&mut self, d: i64) {
        if self.files.is_empty() {
            return;
        }
        let n = self.files.len() as i64;
        let next = (self.file_idx as i64 + d).rem_euclid(n) as usize;
        self.goto_file(next);
        self.pane = Pane::Diff;
        let name = self.path().to_string();
        self.flash(name);
    }

    /// `[` / `]`: working tree, this branch, the last commit.
    pub fn step_scope(&mut self, d: i64) {
        if self.scopes.is_empty() {
            return;
        }
        let n = self.scopes.len() as i64;
        let i = self
            .scopes
            .iter()
            .position(|s| *s == self.scope)
            .unwrap_or(0) as i64;
        self.scope = self.scopes[(i + d).rem_euclid(n) as usize].clone();
        self.refresh();
        let label = self.scope.to_string();
        self.flash(format!("scope → {label}"));
    }

    /// Everything is asked for again. Comments survive: they are anchored to
    /// lines of files, not to anything a refetch invalidates.
    pub fn refresh(&mut self) {
        self.files_state = Load::Idle;
        self.rows_state.clear();
        self.rows.clear();
        self.spans.clear();
        self.blame_state.clear();
        self.blame.clear();
        self.file_idx = 0;
        self.cursor = 0;
        self.anchor = None;
    }

    fn set_context(&mut self, v: i64) {
        // Three at a time between 3 and 21: below three a hunk stops reading
        // as code, and past twenty-one it is the file rather than the change.
        let ctx = v.clamp(3, 21) as u32;
        if ctx == self.context {
            return;
        }
        self.context = ctx;
        // only the open file is re-asked; the rest when they are opened
        let path = self.path().to_string();
        self.rows_state.insert(path, Load::Idle);
        self.flash(format!("context ±{ctx}"));
    }

    // --- comments ---

    /// `c`: opens the note editor over whatever is selected.
    /// `c`. In the tree this is a note about the file itself; in the diff it
    /// is a note about the selected lines.
    pub fn open_comment(&mut self) {
        if self.pane == Pane::Tree {
            if self.path().is_empty() {
                self.flash("no file to ask about");
                return;
            }
            self.about_file = true;
        } else {
            if self.selected_anchors().is_empty() {
                self.flash("move to a code line first");
                return;
            }
            self.about_file = false;
        }
        self.modal = Some(Modal::Comment);
        self.draft.clear();
    }

    /// Saves the draft against the selection.
    pub fn save_comment(&mut self) {
        let body = self.draft.trim().to_string();
        let about_file = std::mem::take(&mut self.about_file);
        let anchors = if about_file {
            Vec::new()
        } else {
            self.selected_anchors()
        };
        let file = self.path().to_string();
        self.modal = None;
        self.draft.clear();

        if body.is_empty() || file.is_empty() || (!about_file && anchors.is_empty()) {
            return;
        }
        let n = anchors.len();
        let snippet = if about_file {
            // What the file is, rather than a line of it: enough for the
            // agent to know which one is meant without opening it.
            let f = self.file();
            f.map(|f| format!("{} · +{} −{}", f.status.label(), f.add, f.del))
                .unwrap_or_default()
        } else {
            self.diff_rows()
                .get(self.span().0)
                .map(|r| r.text.trim().chars().take(60).collect::<String>())
                .unwrap_or_default()
        };

        self.comments.push(Comment {
            anchors,
            file,
            snippet,
            body,
            state: State::Queued,
        });
        self.anchor = None;
        self.flash(if about_file {
            "queued · about the whole file".to_string()
        } else {
            format!("comment queued · {n} line{}", if n == 1 { "" } else { "s" })
        });
    }

    /// `x`: drops whatever comment covers the cursor.
    pub fn delete_comment(&mut self) {
        let path = self.path().to_string();
        let Some(a) = self.row().and_then(|r| r.anchor(&path)) else {
            return;
        };
        let before = self.comments.len();
        self.comments.retain(|c| !c.anchors.contains(&a));
        if self.comments.len() < before {
            self.queue_sel = self.queue_sel.min(self.comments.len().saturating_sub(1));
            self.flash("comment removed");
        }
    }

    /// `S`: hands the queue to the chosen agent.
    pub fn send_queue(&mut self) {
        if self.comments.is_empty() {
            self.flash("queue is empty");
            return;
        }
        let text = self.render_queue();

        if let Some(kind) = self.new_kind.clone() {
            for c in &mut self.comments {
                c.state = State::Sending;
            }
            self.busy = true;
            self.flash(format!("starting a {kind}…"));
            self.ask_spawn(kind, text);
            return;
        }

        let Some(agent) = self.agent().cloned() else {
            self.flash("no agent to send to — press a");
            return;
        };
        // Asked of the multiplexer, not of the status: a backend that cannot
        // see what its agents are doing must not have every send refused on
        // its behalf.
        if let Some(why) = self.refusal(&agent) {
            // Say what to do, not only what went wrong: with every agent busy
            // the way out is to start one, and that is two keys away.
            self.flash(format!("{}: {why} · ␣a to pick another", agent.kind));
            return;
        }

        for c in &mut self.comments {
            c.state = State::Sending;
        }
        self.busy = true;
        self.flash(format!("sending to {}…", agent.kind));
        self.ask_send(agent.pane, text);
    }

    /// Hands a write to the worker. Nothing in here touches a disk itself.
    fn ask_write(&mut self, what: Write) {
        if !self
            .service
            .as_ref()
            .is_some_and(|s| s.send(Request::Write(what)))
        {
            self.flash("the worker is gone — nothing was written");
        }
    }

    /// Starts an agent for this repository and hands it the queue.
    ///
    /// The label names where it came from, so the workspace is recognisable in
    /// herdr next to whatever else is running.
    fn ask_spawn(&mut self, kind: String, text: String) -> bool {
        let label = format!("review {}", self.path());
        let sent = self.service.as_ref().is_some_and(|s| {
            s.send(Request::Spawn {
                repo: self.repo.clone(),
                label,
                kind,
                text,
            })
        });
        if !sent {
            self.busy = false;
            for c in &mut self.comments {
                c.state = State::Queued;
            }
            self.flash("the worker is gone — nothing was sent");
        }
        sent
    }

    fn ask_send(&mut self, pane: String, text: String) -> bool {
        let sent = self
            .service
            .as_ref()
            .is_some_and(|s| s.send(Request::Send { pane, text }));
        if !sent {
            self.busy = false;
            for c in &mut self.comments {
                c.state = State::Queued;
            }
            self.flash("the worker thread is gone — restart diffline");
        }
        sent
    }

    /// The whole queue as one message.
    ///
    /// One message rather than one per comment: an agent handed twelve
    /// separate prompts answers twelve times and sees no shape. Grouped by
    /// file, in line order, because that is the order it will work in.
    pub fn render_queue(&self) -> String {
        let mut out = String::from("Review notes on the current diff.\n");
        out.push_str(&format!("Repository: {}\n", self.repo));
        out.push_str(&format!("Scope: {}\n\n", self.scope));

        let mut paths: Vec<&str> = self.comments.iter().map(Comment::path).collect();
        paths.sort_unstable();
        paths.dedup();

        for path in paths {
            out.push_str(&format!("--- {path}\n"));
            let mut here: Vec<&Comment> =
                self.comments.iter().filter(|c| c.path() == path).collect();
            here.sort_by_key(|c| c.anchors.iter().map(|a| a.line).min().unwrap_or(0));
            for c in here {
                out.push_str(&format!("\n  {}\n", c.where_label()));
                if !c.snippet.is_empty() {
                    out.push_str(&format!("    | {}\n", c.snippet));
                }
                out.push_str(&format!("    {}\n", c.body));
            }
            out.push('\n');
        }
        out
    }

    // --- the finder ---

    /// What the finder is offering, ranked against the query.
    pub fn hits(&self) -> Vec<Hit> {
        let items: Vec<Hit> = match self.finder_tab {
            FinderTab::Files => self
                .files
                .iter()
                .enumerate()
                .map(|(i, f)| Hit {
                    label: f.path.clone(),
                    meta: format!("+{} −{}", f.add, f.del),
                    icon: f.status.mark().to_string(),
                    file: i,
                    row: None,
                })
                .collect(),

            FinderTab::Hunks => self.rows_across(|r| !r.kind.is_code()),
            FinderTab::Grep => self.rows_across(|r| {
                matches!(
                    r.kind,
                    super::model::Kind::Added | super::model::Kind::Deleted
                )
            }),

            // Only the files that have been opened have rows to search, so
            // this lists what is known rather than pretending to index the
            // repository — an honest smaller answer.
            FinderTab::Symbols => self.rows_across(|r| looks_like_a_definition(&r.text)),
        };

        crate::fuzzy::rank(&self.query, &items, |h| h.label.as_str())
            .into_iter()
            .take(200)
            .map(|(i, _)| items[i].clone())
            .collect()
    }

    /// Rows from every file already fetched that pass `keep`.
    fn rows_across(&self, keep: impl Fn(&super::model::Row) -> bool) -> Vec<Hit> {
        let mut out = Vec::new();
        for (i, f) in self.files.iter().enumerate() {
            let Some(rows) = self.rows.get(&f.path) else {
                continue;
            };
            for (ri, r) in rows.iter().enumerate() {
                if !keep(r) {
                    continue;
                }
                out.push(Hit {
                    label: r.text.trim().chars().take(80).collect(),
                    meta: format!(
                        "{}:{}",
                        f.name(),
                        r.new.or(r.old).map(|n| n.to_string()).unwrap_or_default()
                    ),
                    icon: r.sign().to_string(),
                    file: i,
                    row: Some(ri),
                });
            }
        }
        out
    }

    /// Jumps to what the finder has highlighted.
    pub fn take_hit(&mut self) {
        let hits = self.hits();
        let Some(hit) = hits.get(self.sel).cloned() else {
            self.modal = None;
            return;
        };
        self.modal = None;
        self.query.clear();
        self.goto_file(hit.file);
        if let Some(r) = hit.row {
            self.cursor = first_code(self.diff_rows(), r);
        }
        self.pane = Pane::Diff;
    }

    /// Runs a palette entry. The key each is also on is what actually does the
    /// work, so this is one match rather than a second implementation.
    /// Runs a palette entry, and says whether it knew how.
    ///
    /// The answer is what the test uses. It used to compare against a second
    /// copy of the label list kept in the test, which drifted every time a
    /// command was added — three times, each caught by a failure rather than
    /// by the guard doing its job.
    pub fn run_command(&mut self, label: &str) -> bool {
        self.modal = None;
        self.query.clear();
        match label {
            "toggle blame" => self.toggle_blame(),
            "toggle blast radius" => self.modal = Some(Modal::Deps),
            "expand context" => self.set_context(self.context as i64 + 3),
            "collapse context" => self.set_context(self.context as i64 - 3),
            "next file" => self.step_file(1),
            "prev file" => self.step_file(-1),
            "add comment" => self.open_comment(),
            "delete comment under cursor" => self.delete_comment(),
            "pick agent" => self.open_agents(),
            "send queue to agent" => self.send_queue(),
            "clear queue" => {
                self.comments.clear();
                self.queue_sel = 0;
                self.flash("queue cleared");
            }
            "next scope" => self.step_scope(1),
            "prev scope" => self.step_scope(-1),
            "pick a theme" => self.open_themes(),
            "write a keymap to start from" => self.ask_write(Write::KeymapTemplate),
            "write a theme to start from" => self.ask_write(Write::ThemeTemplate("mine".into())),
            "split view" => {
                self.split = !self.split;
                self.hscroll = 0;
            }
            "refresh" => {
                self.refresh();
                self.flash("refreshing…");
            }
            "open help" => self.modal = Some(Modal::Help),
            "quit" => self.should_quit = true,
            _ => return false,
        }
        true
    }

    fn toggle_blame(&mut self) {
        self.blame_on = !self.blame_on;
        self.flash(if self.blame_on {
            "blame on"
        } else {
            "blame off"
        });
    }

    fn open_themes(&mut self) {
        self.modal = Some(Modal::Themes);
        let now = crate::theme::current();
        self.sel = crate::theme::Theme::all()
            .iter()
            .position(|t| *t == now)
            .unwrap_or(0);
    }

    fn open_agents(&mut self) {
        // The list goes stale by the second; this is a decision about which
        // agent is free right now.
        self.agents_state = Load::Idle;
        self.modal = Some(Modal::Agents);
        // Land on something worth choosing rather than on whatever herdr
        // listed first, which is often busy or is this very window.
        if self.new_kind.is_none()
            && self
                .agents
                .get(self.agent_idx)
                .is_some_and(|a| self.refusal(a).is_some())
            && let Some(i) = self.first_usable_agent()
        {
            self.agent_idx = i;
        }
        self.sel = match &self.new_kind {
            // land on the entry that is already the target, whichever half of
            // the list it is in
            Some(kind) => self
                .agent_choices()
                .iter()
                .position(|(k, is_new)| *is_new && k == kind)
                .unwrap_or(self.agent_idx),
            None => self.agent_idx,
        };
    }

    // --- the keymap ---

    pub fn on_key(&mut self, ev: KeyEvent) {
        let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);

        // `^l` means repaint everywhere else; it means it here too, and ahead
        // of the pane keys since plain `l` moves right.
        if ctrl && ev.code == KeyCode::Char('l') {
            self.wants_redraw = true;
            return;
        }
        if ctrl && matches!(ev.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }
        if let Some(m) = self.modal {
            self.modal_key(m, ev, ctrl);
            return;
        }
        // --- what is this key bound to? ---
        let chord = keys::Chord {
            prefix: std::mem::take(&mut self.pending),
            ctrl,
            code: ev.code,
        };

        // A digit is a count unless a count is what it would continue: `0`
        // alone is the start of the line, `10` is ten. Only ever at the top
        // level — `g5` is not a thing.
        if chord.prefix == Pending::None
            && !ctrl
            && let KeyCode::Char(c @ '0'..='9') = ev.code
            && (c != '0' || self.count.is_some())
        {
            let d = c.to_digit(10).unwrap_or(0) as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + d);
            return;
        }

        // A prefix opens an alphabet and keeps the count for whatever
        // finishes it: `5gg` is one command, and spending the count here
        // would leave the `gg` with nothing to act on.
        if chord.prefix == Pending::None && !ctrl && self.keys.is_prefix(ev.code) {
            self.pending = match ev.code {
                KeyCode::Char(' ') => Pending::Leader,
                KeyCode::Char('g') => Pending::G,
                KeyCode::Char('z') => Pending::Z,
                KeyCode::Char('[') => Pending::Bracket(Dir::Prev),
                _ => Pending::Bracket(Dir::Next),
            };
            return;
        }

        let n = self.count.take().unwrap_or(1) as i64;
        if let Some(action) = self.keys.get(chord) {
            self.run(action, n);
        }
    }

    /// Does one thing, `n` times where that means anything.
    ///
    /// Every key in the program arrives here, which is what makes the keymap
    /// a table a reader can edit rather than a `match` only a compiler reads.
    pub fn run(&mut self, action: Action, n: i64) {
        let h = self.view_height.max(1) as i64;
        match action {
            // --- motions ---
            Action::LineDown => self.move_by(n),
            Action::LineUp => self.move_by(-n),
            Action::Top => {
                if n > 1 {
                    self.goto_line(n as usize);
                } else {
                    self.cursor = first_code(self.diff_rows(), 0);
                }
            }
            Action::Bottom => {
                if n > 1 {
                    self.goto_line(n as usize);
                } else {
                    self.cursor = self.diff_rows().len().saturating_sub(1);
                    if !self.diff_rows().is_empty() && !self.diff_rows()[self.cursor].kind.is_code()
                    {
                        self.move_cursor(-1);
                    }
                }
            }
            Action::ScreenTop => self.goto_screen(Place::Top),
            Action::ScreenMiddle => self.goto_screen(Place::Middle),
            Action::ScreenBottom => self.goto_screen(Place::Bottom),
            Action::HalfDown => self.move_by(h / 2),
            Action::HalfUp => self.move_by(-h / 2),
            Action::PageDown => self.move_by(h),
            Action::PageUp => self.move_by(-h),
            Action::ViewDown => self.diff_scroll += 1,
            Action::ViewUp => self.diff_scroll = self.diff_scroll.saturating_sub(1),
            Action::HunkPrev => self.hunk(-1),
            Action::HunkNext => self.hunk(1),
            Action::ChangePrev => self.change(-1),
            Action::ChangeNext => self.change(1),
            Action::FilePrev => self.step_file(-1),
            Action::FileNext => self.step_file(1),
            Action::ScopePrev => self.step_scope(-1),
            Action::ScopeNext => self.step_scope(1),
            Action::CursorToMiddle => self.scroll_cursor_to(Place::Middle),
            Action::CursorToTop => self.scroll_cursor_to(Place::Top),
            Action::CursorToBottom => self.scroll_cursor_to(Place::Bottom),

            // --- along the line ---
            Action::ScrollRight if self.pane == Pane::Diff => {
                self.hscroll = (self.hscroll + n as usize).min(self.longest_line());
            }
            Action::ScrollLeft if self.pane == Pane::Diff && self.hscroll > 0 => {
                self.hscroll = self.hscroll.saturating_sub(n as usize);
            }
            // Off the diff, or already at the start, these are the only way
            // left to walk between panes.
            Action::ScrollLeft => self.focus_by(-1),
            Action::ScrollRight => self.focus_by(1),
            Action::WordForward => self.word(Word::Forward),
            Action::WordEnd => self.word(Word::End),
            Action::WordBack => self.word(Word::Back),
            Action::LineStart => self.hscroll = 0,
            Action::FirstWord => self.first_non_blank(),
            Action::LineEnd => self.hscroll = self.longest_line(),
            Action::PaneNext => self.focus_by(1),
            Action::PanePrev => self.focus_by(-1),
            Action::PaneLeft => self.focus_by(-1),
            Action::PaneRight => self.focus_by(1),

            // --- search and modes ---
            Action::Search => {
                self.modal = Some(Modal::Finder);
                self.query.clear();
                self.sel = 0;
            }
            Action::SearchNext => self.search(1),
            Action::SearchPrev => self.search(-1),
            Action::Visual => {
                self.anchor = if self.anchor.is_none() {
                    Some(self.cursor)
                } else {
                    None
                };
            }
            Action::OtherEnd => {
                if let Some(a) = self.anchor {
                    self.anchor = Some(self.cursor);
                    self.cursor = a;
                }
            }
            Action::Cancel => {
                self.anchor = None;
                self.count = None;
            }
            Action::Commands => {
                self.modal = Some(Modal::Palette);
                self.query.clear();
                self.sel = 0;
            }
            Action::Enter => {
                if self.pane == Pane::Tree {
                    self.pane = Pane::Diff;
                }
            }
            Action::Redraw => self.wants_redraw = true,

            // --- commands ---
            Action::TreePane => self.toggle_pane(Pane::Tree),
            Action::QueuePane => self.toggle_pane(Pane::Queue),
            Action::CodePane => self.pane = Pane::Diff,
            Action::Note => self.open_comment(),
            Action::DeleteNote => self.delete_comment(),
            Action::Agents => self.open_agents(),
            Action::Send => self.send_queue(),
            Action::Split => {
                self.split = !self.split;
                // the columns halve, so wherever the code was scrolled to
                // means something else now
                self.hscroll = 0;
                self.flash(if self.split {
                    "split view"
                } else {
                    "unified view"
                });
            }
            Action::Blame => self.toggle_blame(),
            Action::Deps => self.modal = Some(Modal::Deps),
            Action::ContextMore => self.set_context(self.context as i64 + 3),
            Action::ContextLess => self.set_context(self.context as i64 - 3),
            Action::Refresh => {
                self.refresh();
                self.flash("refreshing…");
            }
            Action::Help => self.modal = Some(Modal::Help),
            Action::Themes => self.open_themes(),
            Action::Quit => self.should_quit = true,
        }
    }

    /// While a modal is up it owns the keyboard, because the letters are its
    /// content.
    fn modal_key(&mut self, m: Modal, ev: KeyEvent, ctrl: bool) {
        if ev.code == KeyCode::Esc {
            self.modal = None;
            self.query.clear();
            self.draft.clear();
            return;
        }

        if m == Modal::Comment {
            match ev.code {
                KeyCode::Enter => self.save_comment(),
                KeyCode::Backspace => {
                    self.draft.pop();
                }
                KeyCode::Char(c) if !ctrl => self.draft.push(c),
                _ => {}
            }
            return;
        }
        if matches!(m, Modal::Help | Modal::Deps) {
            self.modal = None;
            return;
        }

        let len = match m {
            Modal::Agents => self.agent_choices().len(),
            Modal::Themes => crate::theme::Theme::all().len(),
            Modal::Palette => self.palette_hits().len(),
            _ => self.hits().len(),
        };
        let last = len.saturating_sub(1);

        // The agent picker has no query to type into, so it is the one modal
        // where the letters are free to be movement. Everywhere else they are
        // the search, and `j` has to stay a `j`.
        if m == Modal::Themes {
            let all = crate::theme::Theme::all();
            match ev.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.sel = (self.sel + 1).min(all.len().saturating_sub(1));
                }
                KeyCode::Char('k') | KeyCode::Up => self.sel = self.sel.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(t) = all.get(self.sel).copied() {
                        crate::theme::set(t);
                        // Applied here and written on the worker: a theme you
                        // picked and liked should survive a crash, but not at
                        // the cost of a disk write between the key and the
                        // frame that answers it.
                        self.ask_write(Write::Theme(t));
                    }
                    self.modal = None;
                }
                _ => {}
            }
            // Live: moving through the list repaints in that theme, because
            // the only way to judge one is to see it on the diff behind it.
            if let Some(t) = all.get(self.sel).copied() {
                crate::theme::set(t);
            }
            return;
        }

        if m == Modal::Agents {
            match ev.code {
                KeyCode::Char('j') | KeyCode::Down => self.sel = (self.sel + 1).min(last),
                KeyCode::Char('k') | KeyCode::Up => self.sel = self.sel.saturating_sub(1),
                KeyCode::Char('n') if ctrl => self.sel = (self.sel + 1).min(last),
                KeyCode::Char('p') if ctrl => self.sel = self.sel.saturating_sub(1),
                KeyCode::Enter => self.accept(m),
                _ => {}
            }
            return;
        }

        match ev.code {
            KeyCode::Down => self.sel = (self.sel + 1).min(last),
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Char('n') if ctrl => self.sel = (self.sel + 1).min(last),
            KeyCode::Char('p') if ctrl => self.sel = self.sel.saturating_sub(1),
            KeyCode::Tab if m == Modal::Finder => {
                let i = FinderTab::ALL
                    .iter()
                    .position(|t| *t == self.finder_tab)
                    .unwrap_or(0);
                self.finder_tab = FinderTab::ALL[(i + 1) % FinderTab::ALL.len()];
                self.sel = 0;
            }
            KeyCode::Enter => self.accept(m),
            KeyCode::Backspace => {
                self.query.pop();
                self.sel = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.sel = 0;
                // What `n` and `N` will repeat. Taken as it is typed rather
                // than on accept, so that a search you looked at and escaped
                // out of is still the last search — which is what vim does.
                if m == Modal::Finder {
                    self.last_search = self.query.clone();
                }
            }
            _ => {}
        }
    }

    fn accept(&mut self, m: Modal) {
        match m {
            Modal::Finder => self.take_hit(),
            Modal::Palette => {
                let hits = self.palette_hits();
                if let Some(label) = hits.get(self.sel).cloned() {
                    let _ = self.run_command(&label);
                } else {
                    self.modal = None;
                }
            }
            Modal::Agents => {
                let choices = self.agent_choices();
                if let Some((kind, is_new)) = choices.get(self.sel).cloned() {
                    if is_new {
                        self.new_kind = Some(kind.clone());
                        self.flash(format!("target → a new {kind}"));
                    } else {
                        self.agent_idx = self.sel;
                        self.new_kind = None;
                        self.flash(format!("target → {kind}"));
                    }
                }
                self.modal = None;
            }
            _ => self.modal = None,
        }
    }

    /// The palette's entries, ranked against the query.
    pub fn palette_hits(&self) -> Vec<String> {
        let labels: Vec<&str> = COMMANDS.iter().map(|(l, _)| *l).collect();
        crate::fuzzy::rank(&self.query, &labels, |l| l)
            .into_iter()
            .map(|(i, _)| labels[i].to_string())
            .collect()
    }
}

fn step(current: usize, d: i64, len: usize) -> usize {
    (current as i64 + d).clamp(0, (len as i64 - 1).max(0)) as usize
}

/// A rough guess at a line that declares something.
///
/// Rough on purpose: the alternative is a parser per language, which is the
/// trade this program already declined for colour. A line that starts with a
/// declaring word is a definition often enough to be worth jumping to.
fn looks_like_a_definition(text: &str) -> bool {
    let t = text.trim_start();
    const WORDS: &[&str] = &[
        "fn ",
        "pub fn",
        "struct ",
        "enum ",
        "trait ",
        "impl ",
        "type ",
        "const ",
        "class ",
        "interface ",
        "func ",
        "def ",
        "export ",
        "function ",
        "var ",
        "let ",
        "public ",
        "private ",
        "module ",
        "package ",
    ];
    WORDS.iter().any(|w| t.starts_with(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaring_line_is_recognised_and_a_call_is_not() {
        assert!(looks_like_a_definition("pub fn parse(x: &str) {"));
        assert!(looks_like_a_definition("  export class TtlCache {"));
        assert!(looks_like_a_definition("func main() {"));
        assert!(!looks_like_a_definition("  return parse(x);"));
        assert!(!looks_like_a_definition(""));
    }

    #[test]
    fn h_and_l_move_the_code_rather_than_the_focus() {
        // The complaint this answers: `l` inside the diff walked out to the
        // comment pane, so there was no way to reach the right-hand end of a
        // line that had been cut off.
        let mut a = app();
        a.pane = Pane::Diff;
        press(&mut a, KeyCode::Char('l'));
        assert_eq!(a.pane, Pane::Diff, "focus should not have moved");
        assert!(a.hscroll > 0, "the code should have scrolled");

        press(&mut a, KeyCode::Char('0'));
        assert_eq!(a.hscroll, 0, "0 returns to the start of the line");
    }

    #[test]
    fn h_at_the_start_of_the_line_still_leaves_for_the_tree() {
        // Scrolled all the way back there is nothing left to scroll, so the
        // old habit keeps working rather than silently doing nothing.
        let mut a = app();
        a.pane = Pane::Diff;
        a.hscroll = 0;
        press(&mut a, KeyCode::Char('h'));
        assert_eq!(a.pane, Pane::Tree);
    }

    #[test]
    fn the_code_does_not_scroll_past_its_longest_line() {
        let mut a = app();
        a.pane = Pane::Diff;
        for _ in 0..200 {
            press(&mut a, KeyCode::Char('l'));
        }
        assert_eq!(
            a.hscroll,
            a.longest_line(),
            "a pane of blank columns reads as a broken program"
        );
    }

    fn bracket(a: &mut App, open: char, c: char) {
        press(a, KeyCode::Char(open));
        press(a, KeyCode::Char(c));
    }

    fn leader(a: &mut App, c: char) {
        press(a, KeyCode::Char(' '));
        press(a, KeyCode::Char(c));
    }

    #[test]
    fn c_in_the_tree_asks_about_the_file_rather_than_a_line() {
        let mut a = app();
        a.pane = Pane::Tree;
        leader(&mut a, 'n');
        assert_eq!(a.modal, Some(Modal::Comment));
        typed(&mut a, "why is this here?");
        press(&mut a, KeyCode::Enter);

        assert_eq!(a.comments.len(), 1);
        let c = &a.comments[0];
        assert!(c.is_file_note(), "no line was selected, and none is meant");
        assert_eq!(c.path(), "src/a.rs");
        assert_eq!(c.body, "why is this here?");
        // and it says which file in the message the agent gets
        assert!(
            a.render_queue().contains("a.rs · the file"),
            "{}",
            a.render_queue()
        );
    }

    #[test]
    fn a_file_note_does_not_take_the_lines_the_cursor_happened_to_be_on() {
        // The cursor is still somewhere in the diff while the tree has focus.
        // A note about the file must not quietly pick that up.
        let mut a = app();
        a.pane = Pane::Diff;
        a.cursor = 1;
        a.anchor = Some(3);
        assert!(
            !a.selected_anchors().is_empty(),
            "the fixture needs a selection"
        );

        a.pane = Pane::Tree;
        leader(&mut a, 'n');
        typed(&mut a, "about the file");
        press(&mut a, KeyCode::Enter);
        assert!(a.comments[0].anchors.is_empty());
    }

    #[test]
    fn the_toggle_focuses_before_it_hides() {
        // Hiding a pane you were only looking at is never what the key meant.
        let mut a = app();
        a.pane = Pane::Diff;
        assert!(a.tree_shown);
        leader(&mut a, 'e');
        assert!(a.tree_shown, "still there");
        assert_eq!(a.pane, Pane::Tree, "and now you are in it");
        leader(&mut a, 'e');
        assert!(!a.tree_shown, "a second press puts it away");
    }

    #[test]
    fn the_queue_starts_away_and_the_tree_starts_open() {
        let a = app();
        assert!(a.tree_shown, "which files changed is the first question");
        assert!(
            !a.queue_shown,
            "empty until there is something to put in it"
        );
    }

    #[test]
    fn the_leader_shows_and_hides_a_side_pane() {
        let mut a = app();
        a.pane = Pane::Diff;

        leader(&mut a, 'c');
        assert!(a.queue_shown);
        assert_eq!(a.pane, Pane::Queue, "asking for it means going to it");

        leader(&mut a, 'c');
        assert!(!a.queue_shown);
        assert_eq!(a.pane, Pane::Diff, "focus must not stay on what is gone");

        // the tree is already on screen, so the first press goes to it and
        // the second is the one that puts it away
        leader(&mut a, 'e');
        assert!(a.tree_shown);
        leader(&mut a, 'e');
        assert!(!a.tree_shown);
        leader(&mut a, 'd');
        assert_eq!(a.pane, Pane::Diff);
    }

    #[test]
    fn focus_cannot_walk_onto_a_pane_that_is_not_drawn() {
        // The whole reason `panes()` is computed rather than fixed: `h` from
        // the code with the tree hidden used to land on a pane with nothing
        // on screen, and every key after that went somewhere invisible.
        let mut a = app();
        a.tree_shown = false;
        a.queue_shown = false;
        a.pane = Pane::Diff;
        a.hscroll = 0;
        press(&mut a, KeyCode::Char('h'));
        assert_eq!(a.pane, Pane::Diff);
        press(&mut a, KeyCode::Tab);
        assert_eq!(a.pane, Pane::Diff);
    }

    #[test]
    fn a_leader_that_leads_nowhere_does_nothing_and_lets_go() {
        let mut a = app();
        a.pane = Pane::Diff;
        press(&mut a, KeyCode::Char(' '));
        press(&mut a, KeyCode::Char('z'));
        assert_eq!(a.pane, Pane::Diff);
        assert_eq!(a.pending, Pending::None, "the leader must not stay held");
        // and the very next key is a plain key again
        press(&mut a, KeyCode::Char('l'));
        assert!(a.hscroll > 0);
    }

    fn agent_at(
        kind: &str,
        status: crate::mux::AgentStatus,
        focused: bool,
        cwd: &str,
    ) -> crate::mux::Agent {
        crate::mux::Agent {
            kind: kind.into(),
            pane: format!("w:{kind}"),
            cwd: cwd.into(),
            status,
            title: String::new(),
            focused,
        }
    }

    #[test]
    fn the_agent_showing_you_the_diff_is_not_a_target() {
        // Started from inside herdr, diffline is running in one of the very
        // panes it lists. Handing a review to yourself does nothing, and it
        // is an easy mistake when that agent is at the top of the list.
        use crate::mux::AgentStatus;
        let mut a = app();
        let me = agent_at("claude", AgentStatus::Idle, true, "/tmp/r");
        assert!(
            a.refusal(&me).is_some(),
            "an idle agent is still no target when it is this window"
        );
        a.agents = vec![me];
        assert_eq!(a.first_usable_agent(), None);
    }

    #[test]
    fn a_busy_agent_is_refused_with_a_reason_and_a_way_out() {
        use crate::mux::AgentStatus;
        let a = app();
        let busy = agent_at("pi", AgentStatus::Working, false, "/tmp/r");
        let why = a.refusal(&busy).unwrap_or_default();
        assert!(why.contains("context"), "{why}");
        assert!(
            a.refusal(&agent_at("pi", AgentStatus::Idle, false, "/tmp/r"))
                .is_none()
        );
    }

    #[test]
    fn the_first_usable_agent_prefers_one_in_this_repository() {
        // An agent working somewhere else has none of the files the review is
        // about.
        use crate::mux::AgentStatus;
        let mut a = app();
        a.agents = vec![
            agent_at("pi", AgentStatus::Idle, false, "/somewhere/else"),
            agent_at("codex", AgentStatus::Idle, false, "/tmp/r"),
        ];
        assert_eq!(a.first_usable_agent(), Some(1), "the one that is here");
    }

    #[test]
    fn a_busy_first_agent_does_not_become_the_default_target() {
        // `agent_idx` starts at 0, which is whatever the multiplexer listed
        // first — often this very window, or something busy elsewhere.
        use crate::mux::AgentStatus;
        let mut a = app();
        a.agents = vec![
            agent_at("claude", AgentStatus::Working, true, "/tmp/r"),
            agent_at("codex", AgentStatus::Idle, false, "/tmp/r"),
        ];
        a.agent_idx = 0;
        press(&mut a, KeyCode::Char(' '));
        press(&mut a, KeyCode::Char('a'));
        assert_eq!(a.agent_idx, 1, "the picker landed on one worth choosing");
    }

    #[test]
    fn a_new_agent_can_be_the_target_when_none_is_running() {
        // The complaint this answers: the queue could only go to an agent that
        // was already up, so a review with nothing running had nowhere to go.
        let mut a = app();
        assert!(a.agents.is_empty());
        let choices = a.agent_choices();
        assert!(
            !choices.is_empty(),
            "with nothing running there must still be something to pick"
        );
        assert!(choices.iter().all(|(_, is_new)| *is_new));

        a.sel = 0;
        let kind = choices[0].0.clone();
        a.modal = Some(Modal::Agents);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.new_kind.as_deref(), Some(kind.as_str()));
    }

    #[test]
    fn j_and_k_move_in_the_agent_picker() {
        // They did not: the picker has no query, but `j` fell through to the
        // branch that types into one, which also reset the selection — so the
        // list could only be walked with the arrow keys, and every letter
        // silently sent you back to the top.
        let mut a = app();
        a.modal = Some(Modal::Agents);
        a.sel = 0;
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.sel, 1);
        press(&mut a, KeyCode::Char('k'));
        assert_eq!(a.sel, 0);
        press(&mut a, KeyCode::Char('z'));
        assert_eq!(a.sel, 0, "a letter with nothing to do must not move it");
        assert!(a.query.is_empty(), "there is no query here to type into");
    }

    #[test]
    fn picking_a_running_agent_clears_a_pending_new_one() {
        let mut a = app();
        a.agents = vec![crate::mux::Agent {
            kind: "claude".into(),
            pane: "w:1".into(),
            cwd: "/tmp/r".into(),
            status: crate::mux::AgentStatus::Idle,
            title: String::new(),
            focused: false,
        }];
        a.new_kind = Some("pi".into());
        a.modal = Some(Modal::Agents);
        a.sel = 0;
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.agent_idx, 0);
        assert!(
            a.new_kind.is_none(),
            "two targets at once would send the queue twice or nowhere"
        );
    }

    #[test]
    fn stepping_stops_at_the_ends_and_survives_an_empty_list() {
        assert_eq!(step(0, -1, 5), 0);
        assert_eq!(step(4, 1, 5), 4);
        assert_eq!(step(0, 1, 0), 0, "nowhere to go");
    }

    use crate::diffline::app::Load;
    use crate::diffline::model::{ChangedFile, Kind, Row, Scope, Status};
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(key(code));
    }

    fn typed(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    /// A diff already in place and no worker: these are about the reducer.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
        );
        a.service = None;
        a.files = vec![
            ChangedFile {
                path: "src/a.rs".into(),
                status: Status::Modified,
                add: 2,
                del: 1,
            },
            ChangedFile {
                path: "src/b.rs".into(),
                status: Status::Added,
                add: 9,
                del: 0,
            },
        ];
        a.files_state = Load::Ready;
        let rows = vec![
            Row {
                kind: Kind::Header,
                old: None,
                new: None,
                text: "@@ -1,3 +1,4 @@".into(),
            },
            Row {
                kind: Kind::Context,
                old: Some(1),
                new: Some(1),
                text: "fn main() {".into(),
            },
            Row {
                kind: Kind::Deleted,
                old: Some(2),
                new: None,
                text: "  old();".into(),
            },
            Row {
                kind: Kind::Added,
                old: None,
                new: Some(2),
                text: "  new();".into(),
            },
            Row {
                kind: Kind::Added,
                old: None,
                new: Some(3),
                text: "  more();".into(),
            },
        ];
        a.rows.insert("src/a.rs".into(), rows);
        a.rows_state.insert("src/a.rs".into(), Load::Ready);
        a.cursor = 1;
        a
    }

    // --- moving ---

    #[test]
    fn the_cursor_never_lands_on_a_hunk_header() {
        let mut a = app();
        press(&mut a, KeyCode::Char('k'));
        assert_eq!(a.cursor, 1, "it stopped rather than sitting on the @@ line");
        assert!(a.diff_rows()[a.cursor].kind.is_code());
    }

    #[test]
    fn the_cursor_stops_at_the_end() {
        let mut a = app();
        for _ in 0..20 {
            press(&mut a, KeyCode::Char('j'));
        }
        assert_eq!(a.cursor, a.diff_rows().len() - 1);
    }

    #[test]
    fn gg_takes_two_presses_and_g_then_j_is_a_j() {
        let mut a = app();
        a.cursor = 4;
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.cursor, 4, "one g does nothing yet");
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.cursor, 1, "the second one goes to the top");

        a.cursor = 3;
        press(&mut a, KeyCode::Char('g'));
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.cursor, 4, "the pending g did not swallow the j");
    }

    fn ctrl(a: &mut App, c: char) {
        a.on_key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
    }

    #[test]
    fn a_rebound_key_does_the_new_thing_and_the_old_key_does_nothing() {
        let mut a = app();
        a.keys = crate::diffline::keys::Map::with("s = split\nj = none\n<C-n> = line-down\n");
        a.pane = Pane::Diff;

        assert!(!a.split);
        press(&mut a, KeyCode::Char('s'));
        assert!(a.split, "s was bound to split");

        let before = a.cursor;
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.cursor, before, "j was taken away");

        ctrl(&mut a, 'n');
        assert_ne!(a.cursor, before, "and ^n took its job");
    }

    #[test]
    fn unbinding_everything_behind_a_prefix_gives_the_prefix_back() {
        // `]` swallowed the next key because it was hardcoded as a prefix.
        // Now it is one only while something lives behind it, so a reader who
        // clears them out can bind `]` to something itself.
        let mut a = app();
        a.keys =
            crate::diffline::keys::Map::with("]c = none\n]f = none\n]s = none\n] = line-down\n");
        a.pane = Pane::Diff;
        let before = a.cursor;
        press(&mut a, KeyCode::Char(']'));
        assert_ne!(a.cursor, before, "] should have moved, not waited");
    }

    #[test]
    fn a_count_survives_the_prefix_that_follows_it() {
        // `5gg` is one command. Consuming the count when `g` opened its
        // alphabet left the `gg` with nothing to act on, and the cursor went
        // to line 1 instead of line 5.
        let mut a = app();
        typed(&mut a, "3");
        assert_eq!(a.count, Some(3), "the digit is held, not acted on");
        press(&mut a, KeyCode::Char('g'));
        assert_eq!(a.count, Some(3), "and it survives the prefix");
        press(&mut a, KeyCode::Char('g'));

        let code: Vec<usize> = (0..a.diff_rows().len())
            .filter(|i| a.diff_rows()[*i].kind.is_code())
            .collect();
        assert_eq!(a.cursor, code[2], "the third code line");
    }

    #[test]
    fn a_count_repeats_a_motion() {
        let mut a = app();
        a.cursor = 1;
        typed(&mut a, "2");
        press(&mut a, KeyCode::Char('j'));
        let mut b = app();
        b.cursor = 1;
        press(&mut b, KeyCode::Char('j'));
        press(&mut b, KeyCode::Char('j'));
        assert_eq!(a.cursor, b.cursor, "2j is j twice");
        assert_eq!(a.count, None, "and the count is spent");
    }

    #[test]
    fn a_digit_is_a_count_but_zero_alone_is_a_motion() {
        // `0` is the start of the line unless it is continuing a number,
        // which is the one piece of vim's digit handling that is not obvious.
        let mut a = app();
        a.pane = Pane::Diff;
        a.hscroll = 12;
        press(&mut a, KeyCode::Char('0'));
        assert_eq!(a.hscroll, 0, "a bare 0 goes to the start of the line");

        a.hscroll = 12;
        typed(&mut a, "10");
        assert_eq!(a.count, Some(10), "after a 1 it is part of the number");
        assert_eq!(a.hscroll, 12, "and it did not move anything");
    }

    #[test]
    fn hunks_and_changes_are_different_motions() {
        let mut a = app();
        a.cursor = first_code(a.diff_rows(), 0);

        // `}` goes to the next hunk; this fixture has one, so it lands at the
        // end rather than nowhere
        press(&mut a, KeyCode::Char('}'));
        assert!(a.diff_rows()[a.cursor].kind.is_code(), "never on a header");

        // `]c` walks changed lines, and a run of them counts once
        let mut b = app();
        b.cursor = first_code(b.diff_rows(), 0);
        bracket(&mut b, ']', 'c');
        assert!(
            matches!(b.diff_rows()[b.cursor].kind, Kind::Added | Kind::Deleted),
            "]c should land on a change, landed on {:?}",
            b.diff_rows()[b.cursor].kind
        );
    }

    #[test]
    fn screen_motions_are_about_the_window_not_the_file() {
        let mut a = app();
        a.view_height = 3;
        a.diff_scroll = 1;
        press(&mut a, KeyCode::Char('L'));
        let low = a.cursor;
        press(&mut a, KeyCode::Char('H'));
        assert!(a.cursor <= low, "H is above L");
        assert!(
            a.cursor >= a.diff_scroll,
            "H must not go above what is on screen"
        );
    }

    #[test]
    fn zt_and_zb_move_the_window_and_leave_the_cursor() {
        let mut a = app();
        a.view_height = 4;
        a.cursor = 4;
        press(&mut a, KeyCode::Char('z'));
        press(&mut a, KeyCode::Char('t'));
        assert_eq!(a.diff_scroll, 4, "the cursor line is now the top one");
        assert_eq!(a.cursor, 4, "and the cursor did not move");
    }

    #[test]
    fn ctrl_d_is_half_a_screen_of_whatever_size_it_is() {
        let mut a = app();
        a.view_height = 10;
        a.cursor = 1;
        ctrl(&mut a, 'd');
        let ten = a.cursor;
        a.cursor = 1;
        a.view_height = 2;
        ctrl(&mut a, 'd');
        assert!(
            a.cursor <= ten,
            "a shorter window should move less, not the same fixed ten"
        );
    }

    #[test]
    fn n_repeats_what_slash_asked_for() {
        let mut a = app();
        press(&mut a, KeyCode::Char('/'));
        typed(&mut a, "new");
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.last_search, "new", "escaping still leaves a last search");

        a.cursor = 1;
        press(&mut a, KeyCode::Char('n'));
        assert!(
            a.diff_rows()[a.cursor].text.to_lowercase().contains("new"),
            "landed on {:?}",
            a.diff_rows()[a.cursor].text
        );
    }

    #[test]
    fn a_motion_in_visual_mode_grows_the_selection() {
        // Visual mode is the whole point of the motions being separate from
        // the commands: everything that moves must extend a range too.
        let mut a = app();
        a.cursor = 1;
        press(&mut a, KeyCode::Char('V'));
        typed(&mut a, "2");
        press(&mut a, KeyCode::Char('j'));
        let (lo, hi) = a.span();
        assert!(a.visual());
        assert!(hi > lo, "the range should have grown");
        assert_eq!(lo, 1, "and kept the end it started from");
    }

    #[test]
    fn o_swaps_the_ends_of_a_selection() {
        let mut a = app();
        a.cursor = 1;
        press(&mut a, KeyCode::Char('V'));
        press(&mut a, KeyCode::Char('j'));
        let (lo, hi) = a.span();
        press(&mut a, KeyCode::Char('o'));
        assert_eq!(a.span(), (lo, hi), "the range is the same");
        assert_eq!(a.cursor, lo, "but the cursor is now at the other end");
    }

    #[test]
    fn stepping_past_the_last_file_comes_back_to_the_first() {
        let mut a = app();
        bracket(&mut a, ']', 'f');
        assert_eq!(a.file_idx, 1);
        bracket(&mut a, ']', 'f');
        assert_eq!(a.file_idx, 0);
    }

    // --- the visual range ---

    #[test]
    fn v_opens_a_range_and_esc_closes_it() {
        let mut a = app();
        press(&mut a, KeyCode::Char('V'));
        assert!(a.visual());
        press(&mut a, KeyCode::Esc);
        assert!(!a.visual());
    }

    #[test]
    fn a_range_grows_with_the_cursor() {
        let mut a = app();
        press(&mut a, KeyCode::Char('V'));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('j'));
        assert_eq!(a.span(), (1, 3));
        assert_eq!(a.selected_anchors().len(), 3);
    }

    // --- comments ---

    #[test]
    fn a_comment_is_written_against_the_line_it_was_made_on() {
        let mut a = app();
        leader(&mut a, 'n');
        typed(&mut a, "why here?");
        press(&mut a, KeyCode::Enter);

        assert_eq!(a.comments.len(), 1);
        assert_eq!(a.comments[0].body, "why here?");
        assert_eq!(a.comments[0].where_label(), "a.rs:1");
    }

    #[test]
    fn a_comment_on_a_range_holds_every_line_of_it() {
        let mut a = app();
        press(&mut a, KeyCode::Char('V'));
        press(&mut a, KeyCode::Char('j'));
        press(&mut a, KeyCode::Char('j'));
        leader(&mut a, 'n');
        typed(&mut a, "this whole block");
        press(&mut a, KeyCode::Enter);

        assert_eq!(a.comments[0].lines(), 3);
        assert!(!a.visual(), "and the range closed behind it");
    }

    #[test]
    fn an_empty_note_is_not_a_comment() {
        let mut a = app();
        leader(&mut a, 'n');
        press(&mut a, KeyCode::Enter);
        assert!(a.comments.is_empty());
        assert!(a.modal.is_none());
    }

    #[test]
    fn escape_discards_the_draft_rather_than_saving_it() {
        let mut a = app();
        leader(&mut a, 'n');
        typed(&mut a, "never mind");
        press(&mut a, KeyCode::Esc);
        assert!(a.comments.is_empty());
        assert!(a.draft.is_empty());
    }

    #[test]
    fn x_removes_the_comment_the_cursor_is_inside() {
        let mut a = app();
        press(&mut a, KeyCode::Char('V'));
        press(&mut a, KeyCode::Char('j'));
        leader(&mut a, 'n');
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.comments.len(), 1);

        // the cursor is on the second line of a two-line comment
        a.cursor = 2;
        leader(&mut a, 'x');
        assert!(a.comments.is_empty(), "being inside it is enough");
    }

    #[test]
    fn a_comment_survives_the_diff_being_fetched_again() {
        // the whole point of anchoring to lines rather than to rows
        let mut a = app();
        leader(&mut a, 'n');
        typed(&mut a, "keep me");
        press(&mut a, KeyCode::Enter);

        a.refresh();
        assert_eq!(a.comments.len(), 1);
        assert_eq!(a.comments[0].where_label(), "a.rs:1");
    }

    #[test]
    fn changing_the_context_re_asks_only_for_the_open_file() {
        let mut a = app();
        leader(&mut a, '+');
        assert_eq!(a.context, 6);
        assert_eq!(a.rows_state.get("src/a.rs"), Some(&Load::Idle));
    }

    #[test]
    fn the_context_has_ends_and_stays_between_them() {
        let mut a = app();
        for _ in 0..20 {
            leader(&mut a, '+');
        }
        assert_eq!(a.context, 21);
        for _ in 0..20 {
            leader(&mut a, '-');
        }
        assert_eq!(a.context, 3);
    }

    // --- what an agent receives ---

    #[test]
    fn the_queue_travels_as_one_message_grouped_by_file() {
        let mut a = app();
        leader(&mut a, 'n');
        typed(&mut a, "first note");
        press(&mut a, KeyCode::Enter);
        a.cursor = 3;
        leader(&mut a, 'n');
        typed(&mut a, "second note");
        press(&mut a, KeyCode::Enter);

        let text = a.render_queue();
        assert!(text.contains("--- src/a.rs"), "grouped under its file");
        assert_eq!(text.matches("--- src/a.rs").count(), 1, "once, not twice");
        assert!(text.contains("first note") && text.contains("second note"));
        assert!(text.contains("a.rs:1") && text.contains("a.rs:2"));
        assert!(text.contains("working tree"), "the scope is named");
    }

    #[test]
    fn notes_arrive_in_the_order_the_agent_will_work_in() {
        let mut a = app();
        a.cursor = 4; // the later line first
        leader(&mut a, 'n');
        typed(&mut a, "later");
        press(&mut a, KeyCode::Enter);
        a.cursor = 1;
        leader(&mut a, 'n');
        typed(&mut a, "earlier");
        press(&mut a, KeyCode::Enter);

        let text = a.render_queue();
        assert!(
            text.find("earlier") < text.find("later"),
            "line order, not the order they were written:\n{text}"
        );
    }

    #[test]
    fn sending_an_empty_queue_says_so_rather_than_sending_nothing() {
        let mut a = app();
        leader(&mut a, 's');
        assert!(!a.busy);
        assert!(a.toast.contains("empty"), "{}", a.toast);
    }

    #[test]
    fn sending_with_no_agent_says_which_key_finds_one() {
        let mut a = app();
        leader(&mut a, 'n');
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        leader(&mut a, 's');
        assert!(!a.busy);
        assert!(a.toast.contains('a'), "{}", a.toast);
    }

    #[test]
    fn a_busy_agent_is_refused_with_its_reason() {
        let mut a = app();
        a.agents = vec![crate::mux::Agent {
            kind: "claude".into(),
            status: crate::mux::AgentStatus::Working,
            cwd: "/tmp/r".into(),
            pane: "wA:p1".into(),
            title: String::new(),
            focused: false,
        }];
        leader(&mut a, 'n');
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        leader(&mut a, 's');

        assert!(!a.busy, "nothing was sent");
        assert!(a.toast.contains("working"), "{}", a.toast);
        assert_eq!(a.comments[0].state, State::Queued, "and it stayed queued");
    }

    // --- modals ---

    #[test]
    fn a_modal_owns_the_keyboard_while_it_is_up() {
        let mut a = app();
        press(&mut a, KeyCode::Char('/'));
        let before = a.cursor;
        typed(&mut a, "jjj");
        assert_eq!(a.cursor, before, "the letters were the query");
        assert_eq!(a.query, "jjj");
    }

    #[test]
    fn tab_walks_the_finder_scopes_and_comes_back_round() {
        let mut a = app();
        press(&mut a, KeyCode::Char('/'));
        for _ in 0..FinderTab::ALL.len() {
            press(&mut a, KeyCode::Tab);
        }
        assert_eq!(a.finder_tab, FinderTab::Files);
    }

    #[test]
    fn the_finder_lists_the_files_and_jumping_opens_one() {
        let mut a = app();
        press(&mut a, KeyCode::Char('/'));
        typed(&mut a, "b.rs");
        assert!(!a.hits().is_empty());
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.path(), "src/b.rs");
        assert!(a.modal.is_none());
    }

    #[test]
    fn the_palette_runs_what_it_offers() {
        let mut a = app();
        assert!(!a.blame_on);
        press(&mut a, KeyCode::Char(':'));
        typed(&mut a, "blame");
        press(&mut a, KeyCode::Enter);
        assert!(a.blame_on, "the command actually ran");
        assert!(a.modal.is_none());
    }

    #[test]
    fn escape_closes_whatever_is_up() {
        for k in ['/', ':'] {
            let mut a = app();
            press(&mut a, KeyCode::Char(k));
            assert!(a.modal.is_some(), "{k} should open something");
            press(&mut a, KeyCode::Esc);
            assert!(a.modal.is_none(), "{k} should close on esc");
        }
        // and the ones that moved under the leader when the plain keys
        // became motions
        for k in ['a', 'g', '?'] {
            let mut a = app();
            leader(&mut a, k);
            assert!(a.modal.is_some(), "leader {k} should open something");
            press(&mut a, KeyCode::Esc);
            assert!(a.modal.is_none(), "leader {k} should close on esc");
        }
    }

    #[test]
    fn every_palette_entry_is_something_run_command_handles() {
        // the guard against a command that looks available and does nothing
        for (label, _) in COMMANDS {
            let mut a = app();
            assert!(
                a.run_command(label),
                "{label} is offered in the palette but nothing runs it"
            );
        }
    }
}
