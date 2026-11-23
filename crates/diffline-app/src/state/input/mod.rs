//! The keymap, and every state change a keystroke can cause.

mod modals;
mod motions;
mod queue;
#[cfg(test)]
mod tests;

use crate::shared::key::{Key, Press};
use crate::shared::nav::{Dir, Place};

use self::motions::Word;
use crate::app::{App, Modal, Pane, Pending, first_code};
use crate::keys::{self, Action};
use crate::service::Write;

/// The commands the palette offers, and the action each one is — whose key,
/// whatever the reader's keymap has made it, the palette shows beside it.
/// `None` for the few with no key of their own.
pub const COMMANDS: &[(&str, Option<Action>)] = &[
    ("toggle blame", Some(Action::Blame)),
    ("blast radius", Some(Action::Deps)),
    ("expand context", Some(Action::ContextMore)),
    ("collapse context", Some(Action::ContextLess)),
    ("next file", Some(Action::FileNext)),
    ("prev file", Some(Action::FilePrev)),
    ("add comment", Some(Action::Note)),
    ("delete comment under cursor", Some(Action::DeleteNote)),
    ("pick agent", Some(Action::Agents)),
    ("send queue to agent", Some(Action::Send)),
    ("clear queue", None),
    ("commit history", Some(Action::History)),
    ("next scope", Some(Action::ScopeNext)),
    ("prev scope", Some(Action::ScopePrev)),
    ("split or unified view", Some(Action::Split)),
    ("fold or open all folders", Some(Action::FoldAll)),
    ("ignore whitespace", Some(Action::IgnoreWhitespace)),
    ("toggle file tree", Some(Action::TreePane)),
    ("toggle queue", Some(Action::QueuePane)),
    ("find", Some(Action::Search)),
    ("pick a theme", Some(Action::Themes)),
    ("write a keymap to start from", None),
    ("write a theme to start from", None),
    ("refresh", Some(Action::Refresh)),
    ("open help", Some(Action::Help)),
    ("quit", Some(Action::Quit)),
];

impl App {
    // --- moving ---

    /// The panes you can actually walk to. A pane that is not on screen is
    /// not one of them.
    pub fn panes(&self) -> Vec<Pane> {
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
            "blast radius" => self.modal = Some(Modal::Deps),
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
            "commit history" => self.open_history(),
            "next scope" => self.step_scope(1),
            "prev scope" => self.step_scope(-1),
            "pick a theme" => self.open_themes(),
            "write a keymap to start from" => match crate::keys::path() {
                Some(path) => self.ask_write(Write::File {
                    path,
                    text: crate::keys::template(),
                }),
                None => self.flash("no config directory"),
            },
            "write a theme to start from" => {
                if crate::shared::settings::path_beside("themes").is_none() {
                    self.flash("no config directory");
                } else {
                    let (path, text) = crate::tui::theme::template("mine");
                    self.ask_write(Write::File { path, text });
                }
            }
            // each one the action its key runs, so the two cannot drift: the
            // palette's split view did not say which view it had left you in
            "split or unified view" => self.run(Action::Split, 1),
            "fold or open all folders" => self.run(Action::FoldAll, 1),
            "ignore whitespace" => self.run(Action::IgnoreWhitespace, 1),
            "toggle file tree" => self.run(Action::TreePane, 1),
            "toggle queue" => self.run(Action::QueuePane, 1),
            "find" => self.run(Action::Search, 1),
            "refresh" => self.run(Action::Refresh, 1),
            "open help" => self.run(Action::Help, 1),
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

    // --- the keymap ---

    pub fn on_key(&mut self, press: Press) {
        let ctrl = press.ctrl;

        // `^l` repaints. Outside a modal it is the keymap's like any other
        // key, `<C-l>` in the shipped one, so it can be rebound — it was
        // caught here first, and a keys file naming it did nothing. A modal
        // reads its own keys, so there it is caught here still.
        if ctrl && press.key == Key::Char('l') && self.modal.is_some() {
            self.wants_redraw = true;
            return;
        }
        if ctrl && press.key == Key::Char('c') {
            self.should_quit = true;
            return;
        }
        if let Some(m) = self.modal {
            self.modal_key(m, press);
            return;
        }
        // --- what is this key bound to? ---
        let chord = keys::Chord {
            prefix: std::mem::take(&mut self.pending),
            press,
        };

        // A digit is a count unless a count is what it would continue: `0`
        // alone is the start of the line, `10` is ten. Only ever at the top
        // level — `g5` is not a thing.
        if chord.prefix == Pending::None
            && !ctrl
            && !press.alt
            && let Key::Char(c @ '0'..='9') = press.key
            && (c != '0' || self.count.is_some())
        {
            let d = c.to_digit(10).unwrap_or(0) as usize;
            // Cap the repeat count so holding or pasting digits can never
            // overflow `usize` or turn negative when cast to `i64`.
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(d)
                    .min(1_000_000),
            );
            return;
        }

        // A prefix opens an alphabet and keeps the count for whatever
        // finishes it: `5gg` is one command, and spending the count here
        // would leave the `gg` with nothing to act on.
        if chord.prefix == Pending::None && !ctrl && !press.alt && self.keys.is_prefix(press.key) {
            self.pending = match press.key {
                Key::Char(' ') => Pending::Leader,
                Key::Char('g') => Pending::G,
                Key::Char('z') => Pending::Z,
                Key::Char('[') => Pending::Bracket(Dir::Prev),
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
            // `gg`/`G` on the list in focus, not on the diff hidden behind it
            Action::Top if self.pane == Pane::Tree => self.tree_step(-(i64::from(u32::MAX))),
            Action::Bottom if self.pane == Pane::Tree => self.tree_step(i64::from(u32::MAX)),
            Action::Top if self.pane == Pane::Queue => self.queue_sel = 0,
            Action::Bottom if self.pane == Pane::Queue => {
                self.queue_sel = self.comments.len().saturating_sub(1);
            }
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
            Action::ViewDown => self.scroll_view_by(n),
            Action::ViewUp => self.scroll_view_by(-n),
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
                self.hscroll = self
                    .hscroll
                    .saturating_add(n.max(0) as usize)
                    .min(self.max_hscroll());
            }
            Action::ScrollLeft if self.pane == Pane::Diff && self.hscroll > 0 => {
                self.hscroll = self.hscroll.saturating_sub(n as usize);
            }
            // Off the diff, or already at the start, these are the only way
            // left to walk between panes.
            // On the tree they fold and open first, as in any tree, and only
            // walk between panes when there is nothing to fold.
            Action::ScrollLeft if self.pane == Pane::Tree && self.fold_left() => {}
            Action::ScrollRight if self.pane == Pane::Tree && self.unfold_right() => {}
            Action::ScrollLeft => self.focus_by(-1),
            Action::ScrollRight => self.focus_by(1),
            Action::WordForward => self.word(Word::Forward),
            Action::WordEnd => self.word(Word::End),
            Action::WordBack => self.word(Word::Back),
            Action::LineStart => self.hscroll = 0,
            Action::FirstWord => self.first_non_blank(),
            // the end of the cursor's line at the pane's right edge, in view,
            // rather than scrolled off it as far as the line is long
            Action::LineEnd => {
                use unicode_width::UnicodeWidthStr;
                let width = self.cursor_text().width();
                self.hscroll = width.saturating_sub(self.code_cols.max(1));
            }
            Action::PaneNext => self.focus_by(1),
            Action::PanePrev => self.focus_by(-1),
            Action::PaneLeft => self.focus_by(-1),
            Action::PaneRight => self.focus_by(1),

            // --- search and modes ---
            Action::Search => {
                self.modal = Some(Modal::Finder);
                self.query.clear();
                self.sel = 0;
                // the finder reads lines too, and every file's are wanted
                self.ensure_corpus();
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
            Action::OtherEnd if self.pane == Pane::Tree => self.fold_here(),
            Action::FoldAll => self.fold_all(),
            Action::IgnoreWhitespace => self.toggle_whitespace(),
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
            Action::Enter => match self.pane {
                Pane::Tree => self.tree_enter(),
                Pane::Queue => self.goto_comment(self.queue_sel),
                // a note on the line, as a double click on it is
                Pane::Diff => self.open_comment(),
            },
            Action::Redraw => self.wants_redraw = true,

            // --- commands ---
            Action::TreePane => self.toggle_pane(Pane::Tree),
            Action::QueuePane => self.toggle_pane(Pane::Queue),
            Action::CodePane => self.pane = Pane::Diff,
            Action::History => self.open_history(),
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
            Action::Help => {
                self.modal = Some(Modal::Help);
                self.help_scroll = 0;
            }
            Action::Themes => self.open_themes(),
            Action::Quit => self.should_quit = true,
        }
    }
}
