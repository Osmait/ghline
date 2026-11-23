//! Cursor, pane, file, and search motions within the diff view.

use crate::shared::nav::{Place, step};

use crate::app::{App, Load, Modal, Pane, first_code};
use crate::model::{Kind, Scope};

/// Which way `w`, `b` and `e` go, and how far into the word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Word {
    Forward,
    End,
    Back,
}

impl App {
    /// `j`/`k` on whichever pane has focus.
    pub fn move_by(&mut self, d: i64) {
        match self.pane {
            Pane::Tree => self.tree_step(d),
            Pane::Diff => self.move_diff_by(d),
            Pane::Queue => {
                self.queue_sel = step(self.queue_sel, d, self.comments.len());
            }
        }
    }

    /// Moves through code rows one at a time.
    ///
    /// Passing `d` straight to `move_cursor` made it hunt in strides of `d`,
    /// so a `5j` that landed on a hunk header went five further — ten lines
    /// for one keystroke. The wheel shares this path so its cursor and the
    /// viewport cannot disagree on which part of the diff is being read.
    pub(crate) fn move_diff_by(&mut self, d: i64) {
        let step = d.signum();
        for _ in 0..d.unsigned_abs() {
            let before = self.cursor;
            self.move_cursor(step);
            if self.cursor == before {
                break;
            }
        }
    }

    /// Moves the diff cursor, stepping over hunk headers.
    ///
    /// A header is a coordinate rather than a line: landing on one would offer
    /// to comment on `@@ -14,7 +14,9 @@`, which is not something to say.
    pub(super) fn move_cursor(&mut self, d: i64) {
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
    pub(super) fn goto_line(&mut self, n: usize) {
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
    pub(super) fn goto_screen(&mut self, where_to: Place) {
        let rows = self.diff_rows();
        if rows.is_empty() {
            return;
        }
        let target_row = if self.split {
            let pairs = crate::model::pair_rows(rows);
            if pairs.is_empty() {
                return;
            }
            let top = self.diff_scroll.min(pairs.len() - 1);
            let bottom = top
                .saturating_add(self.view_height.saturating_sub(1))
                .min(pairs.len() - 1);
            let target = match where_to {
                Place::Top => top,
                Place::Middle => top + bottom.saturating_sub(top) / 2,
                Place::Bottom => bottom,
            };
            let p = pairs[target];
            p.right.or(p.left).or(p.header).unwrap_or(0)
        } else {
            let top = self.diff_scroll.min(rows.len() - 1);
            let bottom = top
                .saturating_add(self.view_height.saturating_sub(1))
                .min(rows.len() - 1);
            match where_to {
                Place::Top => top,
                Place::Middle => top + bottom.saturating_sub(top) / 2,
                Place::Bottom => bottom,
            }
        };
        self.cursor = target_row.min(rows.len() - 1);
        // A header is not a line you can sit on, so slide off it.
        if !self.diff_rows()[self.cursor].kind.is_code() {
            self.move_cursor(1);
        }
    }

    /// `}` and `{`: the next or previous hunk, landing on its first line.
    ///
    /// Past the last hunk `}` carries on into the next file, and before the
    /// first `{` goes to the file before — a change reads straight through,
    /// file after file, as ghline's pull requests do — instead of stopping
    /// at the file's last line. `{` inside the first hunk still goes to its
    /// top first.
    pub(super) fn hunk(&mut self, d: i64) {
        let rows = self.diff_rows();
        let starts: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == Kind::Header)
            .map(|(i, _)| first_code(rows, i))
            .collect();
        // A hunk's start is its first line, so standing on it is standing at
        // that hunk — the one above is what `{` wants, not the same line again.
        let target = if d > 0 {
            starts.iter().find(|s| **s > self.cursor)
        } else {
            starts.iter().rev().find(|s| **s < self.cursor)
        };
        if let Some(&t) = target {
            self.cursor = t;
            return;
        }
        if d < 0 && self.cursor > first_code(rows, 0) {
            self.cursor = first_code(rows, 0);
            return;
        }
        match self.neighbour_file(d) {
            Some(f) => {
                self.goto_file(f);
                let name = self.files.get(f).map_or("", |x| x.name()).to_string();
                self.flash(name);
            }
            None => self.flash(if d > 0 { "last file" } else { "first file" }),
        }
    }

    /// `[c` and `]c`: the previous or next run of changed lines.
    ///
    /// A run, not a line: twelve deleted lines in a row are one change, and
    /// stopping on each of them would make the motion useless on exactly the
    /// diffs it is for.
    pub(super) fn change(&mut self, d: i64) {
        let rows = self.diff_rows();
        if rows.is_empty() {
            return;
        }
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
    pub(super) fn scroll_cursor_to(&mut self, where_to: Place) {
        let h = self.view_height.max(1);
        let at = if self.split {
            let cursor = self.cursor;
            crate::model::pair_rows(self.diff_rows())
                .iter()
                .position(|p| [p.left, p.right, p.header].contains(&Some(cursor)))
                .unwrap_or(0)
        } else {
            self.cursor
        };
        self.diff_scroll = match where_to {
            Place::Top => at,
            Place::Middle => at.saturating_sub(h / 2),
            Place::Bottom => at.saturating_sub(h.saturating_sub(1)),
        };
    }

    /// `^e`/`^y`, and what a wheel notch over the diff is three of: the
    /// window moves, and the cursor is dragged only when an edge would push
    /// it off screen — scrolling is for reading, and reading a part of the
    /// diff is not yet a decision to put the cursor there.
    ///
    /// Moving only `diff_scroll` was tried twice and lost twice: the next
    /// frame's `scroll_into_view` walked it straight back to the stationary
    /// cursor, so `^e` sprang back and the wheel could never leave the
    /// cursor's window. Dragging the cursor along the near edge is what keeps
    /// the two pointing at the same part of the diff.
    pub(crate) fn scroll_view_by(&mut self, d: i64) {
        if d == 0 || self.diff_rows().is_empty() {
            return;
        }
        let h = self.view_height.max(1);
        // The window counts in drawn lines, which in the split view are pairs
        // of rows while the cursor still counts in rows. `None` marks the
        // unified view, where the two units are the same thing.
        let pairs = self
            .split
            .then(|| crate::model::pair_rows(self.diff_rows()));
        let lines = pairs.as_ref().map_or(self.diff_rows().len(), Vec::len);
        if lines == 0 {
            return;
        }
        let max = lines.saturating_sub(h) as i64;
        let scroll = (self.diff_scroll as i64 + d).clamp(0, max) as usize;
        self.diff_scroll = scroll;

        let cursor = self.cursor;
        let rows = self.diff_rows();
        // The line the cursor sits on, in window units.
        let at = match &pairs {
            Some(ps) => ps
                .iter()
                .position(|p| [p.left, p.right, p.header].contains(&Some(cursor)))
                .unwrap_or(0),
            None => cursor,
        };
        let bottom = (scroll + h - 1).min(lines - 1);
        let target = at.clamp(scroll, bottom);
        if target == at {
            return;
        }

        // Dragged in from above it lands on the top line, from below on the
        // bottom one — sliding inward off anything that cannot hold a cursor,
        // because a hunk header is a coordinate rather than a line.
        let inward: i64 = if target > at { 1 } else { -1 };
        let mut line = target as i64;
        let mut landed = None;
        while (scroll as i64..=bottom as i64).contains(&line) {
            landed = match &pairs {
                Some(ps) => {
                    let p = &ps[line as usize];
                    if p.header.is_some() {
                        None
                    } else {
                        p.right.or(p.left)
                    }
                }
                None => ((line as usize) < rows.len() && rows[line as usize].kind.is_code())
                    .then_some(line as usize),
            };
            if landed.is_some() {
                break;
            }
            line += inward;
        }
        // A window of nothing but headers leaves the cursor where it was, and
        // the next frame's `scroll_into_view` pulls the window back to it.
        if let Some(row) = landed {
            self.cursor = row;
        }
    }

    /// The text the cursor is on, which is what the horizontal motions walk.
    pub(super) fn cursor_text(&self) -> String {
        self.diff_rows()
            .get(self.cursor)
            .map(|r| r.text.clone())
            .unwrap_or_default()
    }

    /// `w`, `b`, `e`. The diff has no editable column, so these move the
    /// window by words rather than a caret between them — which is what you
    /// wanted them for here anyway: getting to the far end of a long line.
    pub(super) fn word(&mut self, motion: Word) {
        use unicode_width::UnicodeWidthChar;
        let text = self.cursor_text();
        let mut cols: Vec<usize> = Vec::new();
        let mut col = 0usize;
        let mut prev_sep = true;
        let to_end = motion == Word::End;
        let chars: Vec<char> = text.chars().collect();
        for (k, &c) in chars.iter().enumerate() {
            let sep = !c.is_alphanumeric() && c != '_';
            if to_end {
                // the last letter of a word: the next is a separator or the end
                let next_sep = chars
                    .get(k + 1)
                    .is_none_or(|n| !n.is_alphanumeric() && *n != '_');
                if !sep && next_sep {
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
        self.hscroll = next
            .unwrap_or(if motion == Word::Back {
                0
            } else {
                self.max_hscroll()
            })
            .min(self.max_hscroll());
    }

    /// `^`: the first character that is not a space.
    pub(super) fn first_non_blank(&mut self) {
        let text = self.cursor_text();
        let col: usize = text
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        self.hscroll = col.min(self.max_hscroll());
    }

    /// `n` and `N`: the next or previous line matching what `/` last asked
    /// for, in every file of the scope in the tree's order, round the end to
    /// the start. Case-insensitive, as vim is with `ignorecase`.
    ///
    /// The open file is read first, from the cursor; the others from their
    /// rows if they have been opened and from the corpus if not. When the
    /// corpus is wanted and not here yet, the search waits for it rather than
    /// answering "no match" about files it has not read.
    pub fn search(&mut self, d: i64) {
        if self.last_search.is_empty() {
            self.flash("no previous search");
            return;
        }
        let needle = self.last_search.to_lowercase();
        let hit =
            |r: &crate::model::Row| r.kind.is_code() && r.text.to_lowercase().contains(&needle);

        // the open file, past the cursor
        let here = self.diff_rows();
        let ahead = if d > 0 {
            here.iter()
                .enumerate()
                .skip(self.cursor + 1)
                .find(|(_, r)| hit(r))
        } else {
            here.iter()
                .enumerate()
                .take(self.cursor.min(here.len()))
                .rev()
                .find(|(_, r)| hit(r))
        }
        .map(|(i, _)| i);
        if let Some(i) = ahead {
            self.cursor = i;
            self.flash(format!("/{}", self.last_search));
            return;
        }

        // the other files, in the order the tree shows them
        let order = crate::shared::tree::order(&self.leaves());
        let n = order.len();
        if n > 1 && !self.corpus_ready() {
            self.search_pending = Some(d);
            self.ensure_corpus();
            self.flash(format!("/{} · reading every file…", self.last_search));
            return;
        }
        let start = order.iter().position(|f| *f == self.file_idx).unwrap_or(0);
        for step in 1..n {
            let f = order[(start as i64 + d * step as i64).rem_euclid(n as i64) as usize];
            let Some(path) = self.files.get(f).map(|x| x.path.clone()) else {
                continue;
            };
            let rows = self
                .rows
                .get(&path)
                .or_else(|| self.corpus.get(&path))
                .map_or(&[][..], Vec::as_slice);
            let found = if d > 0 {
                rows.iter().position(hit)
            } else {
                rows.iter().rposition(hit)
            };
            if let Some(r) = found {
                self.jump_to(f, r);
                let name = self.files.get(f).map_or("", |x| x.name()).to_string();
                self.flash(format!("/{} · {name}", self.last_search));
                return;
            }
        }

        // round the end, back into the open file from its other edge
        let here = self.diff_rows();
        let round = if d > 0 {
            here.iter()
                .enumerate()
                .take(self.cursor + 1)
                .find(|(_, r)| hit(r))
        } else {
            here.iter()
                .enumerate()
                .skip(self.cursor)
                .rev()
                .find(|(_, r)| hit(r))
        }
        .map(|(i, _)| i);
        match round {
            Some(i) => {
                self.cursor = i;
                self.flash(format!("/{} · round the end", self.last_search));
            }
            None => self.flash(format!("no match for {}", self.last_search)),
        }
    }

    /// Opens file `f` with the cursor on its row `r`. A file whose rows have
    /// not been fetched has nothing to put the cursor on yet, so the row waits
    /// for them.
    pub fn jump_to(&mut self, f: usize, r: usize) {
        self.goto_file(f);
        match self.rows.get(self.path()) {
            Some(rows) => self.cursor = first_code(rows, r),
            None => self.row_pending = Some(r),
        }
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

    /// `enter` on a queued note, and a double click on one: back to the lines
    /// it was written about. A note is a decision made somewhere, and the
    /// place to reconsider it is where it was made.
    pub fn goto_comment(&mut self, i: usize) {
        let Some(c) = self.comments.get(i) else {
            return;
        };
        let path = c.path().to_string();
        let anchor = c.anchors.first().cloned();
        let Some(file) = self.files.iter().position(|f| f.path == path) else {
            self.flash(format!("{path} is not in this scope any more"));
            return;
        };
        self.queue_sel = i;
        self.goto_file(file);
        // The row, when the diff is already here. When it is not — purged by
        // a refresh, still loading — the file opens at the top, which is
        // still the right file.
        if let Some(a) = anchor
            && let Some(row) = self
                .diff_rows()
                .iter()
                .position(|r| r.anchor(&path).as_ref() == Some(&a))
        {
            self.cursor = row;
        }
        self.pane = Pane::Diff;
    }

    /// `]f` / `[f`: round the files, so the last leads back to the first.
    pub fn step_file(&mut self, d: i64) {
        if self.files.is_empty() {
            return;
        }
        let Some(next) = self.file_after(d) else {
            return;
        };
        self.goto_file(next);
        self.pane = Pane::Diff;
        let name = self.path().to_string();
        self.flash(name);
    }

    /// What choosing a commit from the history means: the review becomes a
    /// review of that commit.
    ///
    /// The commit tab is retargeted rather than a fourth tab grown per pick:
    /// the tabs are the places a review can stand — the tree, the branch, a
    /// commit — and `[s`/`]s` should keep walking three of them, not a trail
    /// of everywhere the history has been.
    pub fn pick_commit(&mut self, sha: String) {
        let scope = Scope::Commit { sha };
        if let Some(slot) = self
            .scopes
            .iter_mut()
            .find(|s| matches!(s, Scope::Commit { .. }))
        {
            *slot = scope.clone();
        } else {
            self.scopes.push(scope.clone());
        }
        self.scope = scope;
        self.refresh();
    }

    /// `␣l`: the commit history, as a picker over `Scope::Commit`.
    pub fn open_history(&mut self) {
        // Asked for afresh on every opening — see `ensure` — because a commit
        // made while diffline was running belongs in the list.
        self.log_state = Load::Idle;
        self.modal = Some(Modal::History);
        self.query.clear();
        self.sel = 0;
    }

    /// The history, ranked against the query — over the subject, the author
    /// and the short sha at once, because "the fix Maria made" and "a1b2c3d"
    /// are both how a commit is remembered.
    ///
    /// Indices into `log`, so the view and the accept read the same list.
    pub fn history_hits(&self) -> Vec<usize> {
        let hay: Vec<String> = self
            .log
            .iter()
            .map(|c| format!("{} {} {}", c.subject, c.author, c.short()))
            .collect();
        crate::shared::fuzzy::rank(&self.query, &hay, |s| s.as_str())
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    }

    /// `]s` / `[s`: working tree, this branch, the last commit.
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
        self.words.clear();
        self.forget_corpus();
        self.blame_state.clear();
        self.blame.clear();
        self.file_idx = 0;
        self.cursor = 0;
        self.diff_scroll = 0;
        self.hscroll = 0;
        self.tree_scroll = 0;
        self.row_pending = None;
        self.anchor = None;
    }

    /// `␣w`. Every cached diff was asked for under the other rule, so all of
    /// them go rather than only the open one's — a file opened later would
    /// otherwise show whitespace the reader had just asked to leave out.
    pub(super) fn toggle_whitespace(&mut self) {
        self.ignore_ws = !self.ignore_ws;
        self.forget_corpus();
        self.rows.clear();
        self.spans.clear();
        self.words.clear();
        self.rows_state.clear();
        self.flash(if self.ignore_ws {
            "whitespace changes ignored"
        } else {
            "whitespace changes shown"
        });
    }

    pub(super) fn set_context(&mut self, v: i64) {
        // Three at a time between 3 and 21: below three a hunk stops reading
        // as code, and past twenty-one it is the file rather than the change.
        let ctx = v.clamp(3, 21) as u32;
        if ctx == self.context {
            return;
        }
        self.context = ctx;
        self.forget_corpus();
        // Every cached file was fetched under the old context width, so clear
        // them all just as `toggle_whitespace` does.
        self.rows.clear();
        self.spans.clear();
        self.words.clear();
        self.rows_state.clear();
        let path = self.path().to_string();
        if !path.is_empty() {
            self.rows_state.insert(path, Load::Idle);
        }
        self.flash(format!("context ±{ctx}"));
    }
}
