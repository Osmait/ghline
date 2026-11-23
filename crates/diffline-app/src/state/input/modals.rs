//! Modal pickers, finder search hits, and palette dispatch.

use super::COMMANDS;
use crate::shared::key::{Key, Press};

use crate::app::{App, FinderTab, Hit, Load, Modal, Pane};
use crate::service::Write;

impl App {
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
                    crate::model::Kind::Added | crate::model::Kind::Deleted
                )
            }),

            // The scope's own lines, every file's once the corpus has come:
            // what this diff defines, not an index of the repository.
            FinderTab::Symbols => self.rows_across(|r| looks_like_a_definition(&r.text)),
        };

        crate::shared::fuzzy::rank(&self.query, &items, |h| h.label.as_str())
            .into_iter()
            .take(200)
            .map(|(i, _)| items[i].clone())
            .collect()
    }

    /// Rows from every file that pass `keep`: those on show from their rows,
    /// the rest from the corpus once it has come.
    fn rows_across(&self, keep: impl Fn(&crate::model::Row) -> bool) -> Vec<Hit> {
        let mut out = Vec::new();
        for (i, f) in self.files.iter().enumerate() {
            let Some(rows) = self.rows.get(&f.path).or_else(|| self.corpus.get(&f.path)) else {
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
        match hit.row {
            Some(r) => self.jump_to(hit.file, r),
            None => self.goto_file(hit.file),
        }
        self.pane = Pane::Diff;
    }

    pub(super) fn open_themes(&mut self) {
        self.modal = Some(Modal::Themes);
        let now = crate::tui::theme::current();
        self.theme_before = now;
        self.sel = crate::tui::theme::Theme::all()
            .iter()
            .position(|t| *t == now)
            .unwrap_or(0);
    }

    pub(super) fn open_agents(&mut self) {
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

    /// While a modal is up it owns the keyboard, because the letters are its
    /// content.
    pub(super) fn modal_key(&mut self, m: Modal, press: Press) {
        let ctrl = press.ctrl;
        if press.key == Key::Esc {
            self.close_modal();
            return;
        }

        if m == Modal::Comment {
            match press.key {
                Key::Enter => self.save_comment(),
                Key::Backspace => {
                    self.draft.pop();
                }
                Key::Char(c) if !ctrl && !press.alt => self.draft.push(c),
                _ => {}
            }
            return;
        }
        // The help is longer than a short terminal: it scrolls, and closes on
        // the keys that close things rather than on any key at all, which
        // made the scroll impossible.
        if m == Modal::Help {
            match press.key {
                Key::Char('j') | Key::Down => self.help_scroll += 1,
                Key::Char('k') | Key::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                Key::Char('d') if ctrl => self.help_scroll += 10,
                Key::Char('u') if ctrl => self.help_scroll = self.help_scroll.saturating_sub(10),
                Key::Char('q' | '?') | Key::Enter => self.modal = None,
                _ => {}
            }
            return;
        }
        if m == Modal::Deps {
            self.modal = None;
            return;
        }

        let len = self.modal_len(m);
        let last = len.saturating_sub(1);

        // The agent picker has no query to type into, so it is the one modal
        // where the letters are free to be movement. Everywhere else they are
        // the search, and `j` has to stay a `j`.
        if m == Modal::Themes {
            let all = crate::tui::theme::Theme::all();
            match press.key {
                Key::Char('j') | Key::Down => {
                    self.sel = (self.sel + 1).min(all.len().saturating_sub(1));
                }
                Key::Char('k') | Key::Up => self.sel = self.sel.saturating_sub(1),
                Key::Enter => {
                    self.accept(Modal::Themes);
                    return;
                }
                _ => {}
            }
            // Live: moving through the list repaints in that theme, because
            // the only way to judge one is to see it on the diff behind it.
            if let Some(t) = all.get(self.sel).copied() {
                crate::tui::theme::set(t);
            }
            return;
        }

        if m == Modal::Agents {
            match press.key {
                Key::Char('j') | Key::Down => self.sel = (self.sel + 1).min(last),
                Key::Char('k') | Key::Up => self.sel = self.sel.saturating_sub(1),
                Key::Char('n') if ctrl => self.sel = (self.sel + 1).min(last),
                Key::Char('p') if ctrl => self.sel = self.sel.saturating_sub(1),
                Key::Enter => self.accept(m),
                _ => {}
            }
            return;
        }

        match press.key {
            Key::Down => self.sel = (self.sel + 1).min(last),
            Key::Up => self.sel = self.sel.saturating_sub(1),
            Key::Char('n') if ctrl => self.sel = (self.sel + 1).min(last),
            Key::Char('p') if ctrl => self.sel = self.sel.saturating_sub(1),
            Key::Tab | Key::BackTab if m == Modal::Finder => {
                let n = FinderTab::ALL.len();
                let i = FinderTab::ALL
                    .iter()
                    .position(|t| *t == self.finder_tab)
                    .unwrap_or(0);
                let step = if press.key == Key::Tab { 1 } else { n - 1 };
                self.finder_tab = FinderTab::ALL[(i + step) % n];
                self.sel = 0;
            }
            Key::Enter => self.accept(m),
            Key::Backspace => {
                self.query.pop();
                self.sel = 0;
                if m == Modal::Finder {
                    self.last_search = self.query.clone();
                }
            }
            Key::Char(c) if !ctrl && !press.alt => {
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

    /// How many rows the modal `m` lists, for the keys and the wheel alike.
    pub fn modal_len(&self, m: Modal) -> usize {
        match m {
            Modal::Agents => self.agent_choices().len(),
            Modal::Themes => crate::tui::theme::Theme::all().len(),
            Modal::Palette => self.palette_hits().len(),
            Modal::History => self.history_hits().len(),
            _ => self.hits().len(),
        }
    }

    /// Closes whatever modal is up, as `esc` does — and the theme picker,
    /// which previews as it moves, puts back the theme it opened on. Its hint
    /// says `esc undo`, and leaving it used to keep whatever was last looked
    /// at, unsaved.
    pub fn close_modal(&mut self) {
        if self.modal == Some(Modal::Themes) {
            crate::tui::theme::set(self.theme_before);
        }
        self.modal = None;
        self.query.clear();
        self.draft.clear();
    }

    pub(super) fn accept(&mut self, m: Modal) {
        match m {
            // Applied here and written on the worker: a theme you picked and
            // liked should survive a crash, but not at the cost of a disk
            // write between the key and the frame that answers it. Here
            // rather than under enter, so a double click keeps it too.
            Modal::Themes => {
                if let Some(t) = crate::tui::theme::Theme::all().get(self.sel).copied() {
                    crate::tui::theme::set(t);
                    self.ask_write(Write::Theme(t));
                }
                self.modal = None;
            }
            Modal::Finder => self.take_hit(),
            Modal::History => {
                let hits = self.history_hits();
                if let Some(commit) = hits.get(self.sel).and_then(|i| self.log.get(*i)).cloned() {
                    let said = format!("commit {} · {}", commit.short(), commit.subject);
                    self.pick_commit(commit.sha);
                    self.flash(said);
                }
                self.modal = None;
                self.query.clear();
            }
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
        crate::shared::fuzzy::rank(&self.query, &labels, |l| l)
            .into_iter()
            .map(|(i, _)| labels[i].to_string())
            .collect()
    }
}

/// A rough guess at a line that declares something.
///
/// Rough on purpose: the alternative is a parser per language, which is the
/// trade this program already declined for colour. A line that starts with a
/// declaring word is a definition often enough to be worth jumping to.
pub(super) fn looks_like_a_definition(text: &str) -> bool {
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
