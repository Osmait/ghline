//! The keymap, and every state change a keystroke can cause.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, FinderTab, Hit, Load, Modal, Pane, first_code};
use super::model::{Comment, State};
use super::service::Request;

/// The commands the palette offers, and the key each is also on.
pub const COMMANDS: &[(&str, &str)] = &[
    ("toggle blame", "b"),
    ("toggle blast radius", "d"),
    ("expand context", "+"),
    ("collapse context", "-"),
    ("next file", "n"),
    ("prev file", "p"),
    ("add comment", "c"),
    ("delete comment under cursor", "x"),
    ("pick agent", "a"),
    ("send queue to agent", "S"),
    ("clear queue", ""),
    ("next scope", "]"),
    ("refresh", "r"),
    ("open help", "?"),
    ("quit", "q"),
];

pub const HELP: &[(&str, &str)] = &[
    ("j / k", "move cursor"),
    ("gg / G", "top / bottom"),
    ("h / l", "pane left / right"),
    ("n / p", "next / prev file"),
    ("/", "fuzzy finder"),
    (":", "command palette"),
    ("⇥", "switch finder scope"),
    ("V / esc", "select line range"),
    ("c", "comment on line / range"),
    ("x", "delete comment"),
    ("a", "pick target agent"),
    ("S", "send whole queue"),
    ("b", "inline blame"),
    ("d", "blast radius"),
    ("+ / -", "expand / collapse context"),
    ("[ / ]", "previous / next scope"),
    ("r", "refresh"),
    ("^l", "repaint the screen"),
    ("? / q", "this help / quit"),
];

impl App {
    // --- moving ---

    fn panes(&self) -> [Pane; 3] {
        [Pane::Tree, Pane::Diff, Pane::Queue]
    }

    fn focus_by(&mut self, d: i64) {
        let panes = self.panes();
        let i = panes.iter().position(|p| *p == self.pane).unwrap_or(1) as i64;
        let j = (i + d).clamp(0, panes.len() as i64 - 1) as usize;
        self.pane = panes[j];
    }

    /// `j`/`k` on whichever pane has focus.
    pub fn move_by(&mut self, d: i64) {
        match self.pane {
            Pane::Tree => {
                let n = self.files.len();
                self.goto_file(step(self.file_idx, d, n));
            }
            Pane::Diff => self.move_cursor(d),
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

    pub fn goto_file(&mut self, i: usize) {
        if self.files.is_empty() {
            return;
        }
        self.file_idx = i.min(self.files.len() - 1);
        self.cursor = first_code(self.diff_rows(), 0);
        self.anchor = None;
        self.diff_scroll = 0;
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
        let label = self.scope.label();
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
    pub fn open_comment(&mut self) {
        if self.selected_anchors().is_empty() {
            self.flash("move to a code line first");
            return;
        }
        self.modal = Some(Modal::Comment);
        self.draft.clear();
    }

    /// Saves the draft against the selection.
    pub fn save_comment(&mut self) {
        let body = self.draft.trim().to_string();
        let anchors = self.selected_anchors();
        self.modal = None;
        self.draft.clear();

        if body.is_empty() || anchors.is_empty() {
            return;
        }
        let n = anchors.len();
        let snippet = self
            .diff_rows()
            .get(self.span().0)
            .map(|r| r.text.trim().chars().take(60).collect::<String>())
            .unwrap_or_default();

        self.comments.push(Comment {
            anchors,
            snippet,
            body,
            state: State::Queued,
        });
        self.anchor = None;
        self.flash(format!(
            "comment queued · {n} line{}",
            if n == 1 { "" } else { "s" }
        ));
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
        let Some(agent) = self.agent().cloned() else {
            self.flash("no agent to send to — press a");
            return;
        };
        if !agent.status.is_free() {
            self.flash(format!("{} is {}", agent.kind, agent.status));
            return;
        }

        let text = self.render_queue();
        for c in &mut self.comments {
            c.state = State::Sending;
        }
        self.busy = true;
        self.flash(format!("sending to {}…", agent.kind));
        self.ask_send(agent.pane, text);
    }

    fn ask_send(&self, pane: String, text: String) {
        if let Some(s) = &self.service {
            s.send(Request::Send { pane, text });
        }
    }

    /// The whole queue as one message.
    ///
    /// One message rather than one per comment: an agent handed twelve
    /// separate prompts answers twelve times and sees no shape. Grouped by
    /// file, in line order, because that is the order it will work in.
    pub fn render_queue(&self) -> String {
        let mut out = String::from("Review notes on the current diff.\n");
        out.push_str(&format!("Repository: {}\n", self.repo));
        out.push_str(&format!("Scope: {}\n\n", self.scope.label()));

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
    pub fn run_command(&mut self, label: &str) {
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
            "refresh" => {
                self.refresh();
                self.flash("refreshing…");
            }
            "open help" => self.modal = Some(Modal::Help),
            "quit" => self.should_quit = true,
            _ => {}
        }
    }

    fn toggle_blame(&mut self) {
        self.blame_on = !self.blame_on;
        self.flash(if self.blame_on {
            "blame on"
        } else {
            "blame off"
        });
    }

    fn open_agents(&mut self) {
        // The list goes stale by the second; this is a decision about which
        // agent is free right now.
        self.agents_state = Load::Idle;
        self.modal = Some(Modal::Agents);
        self.sel = self.agent_idx;
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
        if ctrl {
            match ev.code {
                KeyCode::Char('d') => self.move_by(10),
                KeyCode::Char('u') => self.move_by(-10),
                _ => {}
            }
            return;
        }

        match ev.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_by(-1),
            KeyCode::Char('h') | KeyCode::Left => self.focus_by(-1),
            KeyCode::Char('l') | KeyCode::Right => self.focus_by(1),
            KeyCode::PageDown => self.move_by(20),
            KeyCode::PageUp => self.move_by(-20),

            KeyCode::Char('g') => {
                // `gg` in two presses, which is what the hands expect
                if self.pending_g {
                    self.pending_g = false;
                    self.cursor = first_code(self.diff_rows(), 0);
                } else {
                    self.pending_g = true;
                }
                return;
            }
            KeyCode::Char('G') => {
                self.cursor = self.diff_rows().len().saturating_sub(1);
            }

            KeyCode::Char('n') => self.step_file(1),
            KeyCode::Char('p') => self.step_file(-1),
            KeyCode::Char(']') => self.step_scope(1),
            KeyCode::Char('[') => self.step_scope(-1),

            KeyCode::Char('/') => {
                self.modal = Some(Modal::Finder);
                self.query.clear();
                self.sel = 0;
            }
            KeyCode::Char(':') => {
                self.modal = Some(Modal::Palette);
                self.query.clear();
                self.sel = 0;
            }

            KeyCode::Char('V' | 'v') => {
                self.anchor = if self.anchor.is_none() {
                    Some(self.cursor)
                } else {
                    None
                };
            }
            KeyCode::Esc => self.anchor = None,

            KeyCode::Char('c') => self.open_comment(),
            KeyCode::Char('x') => self.delete_comment(),
            KeyCode::Char('a') => self.open_agents(),
            KeyCode::Char('S') => self.send_queue(),
            KeyCode::Char('b') => self.toggle_blame(),
            KeyCode::Char('d') => self.modal = Some(Modal::Deps),
            KeyCode::Char('r') => {
                self.refresh();
                self.flash("refreshing…");
            }
            KeyCode::Char('+' | '=') => self.set_context(self.context as i64 + 3),
            KeyCode::Char('-') => self.set_context(self.context as i64 - 3),
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Enter if self.pane == Pane::Tree => self.pane = Pane::Diff,
            _ => {}
        }
        self.pending_g = false;
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
            Modal::Agents => self.agents.len(),
            Modal::Palette => self.palette_hits().len(),
            _ => self.hits().len(),
        };
        let last = len.saturating_sub(1);

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
                    self.run_command(&label);
                } else {
                    self.modal = None;
                }
            }
            Modal::Agents => {
                if self.sel < self.agents.len() {
                    self.agent_idx = self.sel;
                    let who = self.agents[self.sel].kind.clone();
                    self.flash(format!("target → {who}"));
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
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
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

    #[test]
    fn stepping_past_the_last_file_comes_back_to_the_first() {
        let mut a = app();
        press(&mut a, KeyCode::Char('n'));
        assert_eq!(a.file_idx, 1);
        press(&mut a, KeyCode::Char('n'));
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
        press(&mut a, KeyCode::Char('c'));
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
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "this whole block");
        press(&mut a, KeyCode::Enter);

        assert_eq!(a.comments[0].lines(), 3);
        assert!(!a.visual(), "and the range closed behind it");
    }

    #[test]
    fn an_empty_note_is_not_a_comment() {
        let mut a = app();
        press(&mut a, KeyCode::Char('c'));
        press(&mut a, KeyCode::Enter);
        assert!(a.comments.is_empty());
        assert!(a.modal.is_none());
    }

    #[test]
    fn escape_discards_the_draft_rather_than_saving_it() {
        let mut a = app();
        press(&mut a, KeyCode::Char('c'));
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
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.comments.len(), 1);

        // the cursor is on the second line of a two-line comment
        a.cursor = 2;
        press(&mut a, KeyCode::Char('x'));
        assert!(a.comments.is_empty(), "being inside it is enough");
    }

    #[test]
    fn a_comment_survives_the_diff_being_fetched_again() {
        // the whole point of anchoring to lines rather than to rows
        let mut a = app();
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "keep me");
        press(&mut a, KeyCode::Enter);

        a.refresh();
        assert_eq!(a.comments.len(), 1);
        assert_eq!(a.comments[0].where_label(), "a.rs:1");
    }

    #[test]
    fn changing_the_context_re_asks_only_for_the_open_file() {
        let mut a = app();
        press(&mut a, KeyCode::Char('+'));
        assert_eq!(a.context, 6);
        assert_eq!(a.rows_state.get("src/a.rs"), Some(&Load::Idle));
    }

    #[test]
    fn the_context_has_ends_and_stays_between_them() {
        let mut a = app();
        for _ in 0..20 {
            press(&mut a, KeyCode::Char('+'));
        }
        assert_eq!(a.context, 21);
        for _ in 0..20 {
            press(&mut a, KeyCode::Char('-'));
        }
        assert_eq!(a.context, 3);
    }

    // --- what an agent receives ---

    #[test]
    fn the_queue_travels_as_one_message_grouped_by_file() {
        let mut a = app();
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "first note");
        press(&mut a, KeyCode::Enter);
        a.cursor = 3;
        press(&mut a, KeyCode::Char('c'));
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
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "later");
        press(&mut a, KeyCode::Enter);
        a.cursor = 1;
        press(&mut a, KeyCode::Char('c'));
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
        press(&mut a, KeyCode::Char('S'));
        assert!(!a.busy);
        assert!(a.toast.contains("empty"), "{}", a.toast);
    }

    #[test]
    fn sending_with_no_agent_says_which_key_finds_one() {
        let mut a = app();
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Char('S'));
        assert!(!a.busy);
        assert!(a.toast.contains('a'), "{}", a.toast);
    }

    #[test]
    fn a_busy_agent_is_refused_with_its_reason() {
        let mut a = app();
        a.agents = vec![crate::data::Agent {
            kind: "claude".into(),
            status: crate::data::AgentStatus::Working,
            cwd: "/tmp/r".into(),
            pane: "wA:p1".into(),
            title: String::new(),
            focused: false,
        }];
        press(&mut a, KeyCode::Char('c'));
        typed(&mut a, "x");
        press(&mut a, KeyCode::Enter);
        press(&mut a, KeyCode::Char('S'));

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
        for k in ['/', ':', 'a', 'd', '?'] {
            let mut a = app();
            press(&mut a, KeyCode::Char(k));
            assert!(a.modal.is_some(), "{k} should open something");
            press(&mut a, KeyCode::Esc);
            assert!(a.modal.is_none(), "{k} should close on esc");
        }
    }

    #[test]
    fn every_palette_entry_is_something_run_command_handles() {
        // the guard against a command that looks available and does nothing
        for (label, _) in COMMANDS {
            assert!(
                matches!(
                    *label,
                    "toggle blame"
                        | "toggle blast radius"
                        | "expand context"
                        | "collapse context"
                        | "next file"
                        | "prev file"
                        | "add comment"
                        | "delete comment under cursor"
                        | "pick agent"
                        | "send queue to agent"
                        | "clear queue"
                        | "next scope"
                        | "refresh"
                        | "open help"
                        | "quit"
                ),
                "{label} is offered but not implemented"
            );
        }
    }
}
