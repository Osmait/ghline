//! Diffline's state, and the questions it answers.

use std::collections::HashMap;

use std::sync::Arc;

use crate::error::Error;
use crate::nav::Dir;

use super::model::{Anchor, ChangedFile, Comment, Row, Scope, State};
use super::service::{Request, Response, Service};
use crate::mux::Agent;

/// Load state of one piece of data.
#[derive(Clone, Debug, Default)]
pub enum Load {
    #[default]
    Idle,
    Loading,
    Ready,
    /// What went wrong, kept whole.
    ///
    /// The error rather than a sentence about it: `Error` knows whether it is
    /// worth retrying and what caused it, and flattening it to a `String` at
    /// this boundary threw both away — `is_transient` was unreachable from
    /// here and `source` was never walked anywhere in the program.
    ///
    /// `Arc` because the state is cloned to be rendered and `io::Error` is
    /// not `Clone`; sharing it is right anyway, since there is one failure and
    /// several places that describe it.
    Failed(Arc<Error>),
}

impl Load {
    pub fn is_loading(&self) -> bool {
        *self == Self::Loading
    }

    /// The failure, for anything that wants more than a sentence.
    pub fn failure(&self) -> Option<&Error> {
        match self {
            Self::Failed(e) => Some(e),
            _ => None,
        }
    }

    /// One line for the status bar or an empty pane.
    pub fn error(&self) -> Option<String> {
        self.failure().map(Error::brief)
    }

    /// Whether trying again might work. A network blip is worth a retry; a
    /// missing `git` is not, and offering one would be a lie.
    pub fn is_transient(&self) -> bool {
        self.failure().is_some_and(Error::is_transient)
    }
}

impl PartialEq for Load {
    /// Two failures are the same state for the interface's purposes: it is
    /// asking "am I loading, ready, or broken", not which error it was.
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Idle, Self::Idle)
                | (Self::Loading, Self::Loading)
                | (Self::Ready, Self::Ready)
                | (Self::Failed(_), Self::Failed(_))
        )
    }
}

/// The panes, left to right. `h`/`l` walk them and `j`/`k` act on the one with
/// focus — the same bargain the GitHub browser makes, so the hands transfer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Tree,
    Diff,
    Queue,
}

/// A prefix key that has been pressed and not yet resolved.
///
/// vim's grammar is prefixes: `g` and `z` open a second alphabet, `[` and `]`
/// open the "go to the previous or next one of these" alphabet, and the
/// leader opens this program's own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Pending {
    #[default]
    None,
    Leader,
    G,
    Z,
    /// `[` or `]`, carrying which one it was.
    Bracket(Dir),
}

/// What is over everything else, if anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modal {
    /// The four-scope fuzzy finder.
    Finder,
    /// The `:` command list.
    Palette,
    /// The theme picker.
    Themes,
    /// Writing a note against the selection.
    Comment,
    /// Which agent gets the queue.
    Agents,
    /// What else imports the file being read.
    Deps,
    Help,
}

/// What the finder is searching.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FinderTab {
    Files,
    Hunks,
    Symbols,
    Grep,
}

impl FinderTab {
    pub const ALL: [Self; 4] = [Self::Files, Self::Hunks, Self::Symbols, Self::Grep];

    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "FILES",
            Self::Hunks => "HUNKS",
            Self::Symbols => "SYMBOLS",
            Self::Grep => "LIVE GREP",
        }
    }
}

/// One row of the finder's result list.
#[derive(Clone)]
pub struct Hit {
    pub label: String,
    pub meta: String,
    pub icon: String,
    /// Which file it is in, as an index into `files`.
    pub file: usize,
    /// Which row of that file to land on, when the hit knows.
    pub row: Option<usize>,
}

pub struct App {
    pub repo: String,
    pub service: Option<Service>,

    // --- what is being reviewed ---
    pub scope: Scope,
    pub scopes: Vec<Scope>,
    pub files: Vec<ChangedFile>,
    pub files_state: Load,
    pub file_idx: usize,
    pub tree_scroll: usize,

    // --- the diff ---
    /// Rows keyed by path, so stepping between files does not re-fetch.
    pub rows: HashMap<String, Vec<Row>>,
    pub spans: HashMap<String, Vec<Vec<crate::syntax::Span>>>,
    pub rows_state: HashMap<String, Load>,
    pub cursor: usize,
    pub diff_scroll: usize,
    /// Where a visual-line selection started, if one is open.
    pub anchor: Option<usize>,
    /// Lines of context either side, as `git diff -U<n>`.
    pub context: u32,
    pub blame_on: bool,
    pub blame: HashMap<String, Vec<String>>,
    pub blame_state: HashMap<String, Load>,

    // --- the queue ---
    pub comments: Vec<Comment>,
    pub queue_sel: usize,
    pub replies: Vec<String>,
    pub agents: Vec<Agent>,
    pub agents_state: Load,
    pub agent_idx: usize,
    /// Set when the queue is bound for an agent that is not running yet: the
    /// kind to start. `None` means send to `agent_idx`, one already up.
    pub new_kind: Option<String>,
    /// Whether the file tree is on screen. Open to begin with: which files
    /// changed is the first question a review asks.
    /// The comment being drafted is about the file, not about any line in
    /// it. Set when `c` is pressed with the tree in focus.
    /// Side by side rather than one column: old on the left, new on the
    /// right. `s` switches.
    pub split: bool,
    pub about_file: bool,
    pub tree_shown: bool,
    /// Whether the review queue is on screen. Closed to begin with: it is
    /// empty until there is something to put in it, and until then it is a
    /// third of the width spent on nothing. The floating tab keeps the count
    /// visible while it is away.
    pub queue_shown: bool,
    /// Columns the code is scrolled right by. The gutters do not move with
    /// it: a line number that scrolled away would leave the pane unreadable
    /// exactly when you are furthest from the start of the line.
    pub hscroll: usize,

    // --- interface ---
    pub pane: Pane,
    pub modal: Option<Modal>,
    pub finder_tab: FinderTab,
    pub query: String,
    pub sel: usize,
    pub draft: String,
    pub toast: String,
    pub toast_ttl: u8,
    pub busy: bool,
    pub blink: bool,
    pub anim: u64,
    pub should_quit: bool,
    pub wants_redraw: bool,
    /// Half of a `gg`. Cleared by any other key, so `g` then `j` is a `j`.
    /// What the keys mean. Read once at startup from `<config>/keys`, or
    /// the shipped map when there is no file.
    pub keys: super::keys::Map,
    /// A key that begins something longer and is waiting for the rest of it.
    pub pending: Pending,
    /// Digits typed before a motion. `5j` is five lines, not a five and a j.
    pub count: Option<usize>,
    /// What `/` last looked for, which is what `n` and `N` repeat.
    pub last_search: String,
    /// How many rows of diff were on screen at the last draw. `H`, `M`, `L`
    /// and the `z` commands are about the window, so they have to know how
    /// big it is, and only the render does.
    pub view_height: usize,
}

impl App {
    pub fn new(repo: String, scope: Scope, scopes: Vec<Scope>) -> Self {
        Self {
            repo,
            service: Some(Service::spawn()),
            scope,
            scopes,
            files: Vec::new(),
            files_state: Load::Idle,
            file_idx: 0,
            tree_scroll: 0,
            rows: HashMap::new(),
            spans: HashMap::new(),
            rows_state: HashMap::new(),
            cursor: 0,
            diff_scroll: 0,
            anchor: None,
            context: 3,
            blame_on: false,
            blame: HashMap::new(),
            blame_state: HashMap::new(),
            comments: Vec::new(),
            queue_sel: 0,
            replies: Vec::new(),
            agents: Vec::new(),
            agents_state: Load::Idle,
            agent_idx: 0,
            new_kind: None,
            split: false,
            about_file: false,
            tree_shown: true,
            queue_shown: false,
            hscroll: 0,
            pane: Pane::Diff,
            modal: None,
            finder_tab: FinderTab::Files,
            query: String::new(),
            sel: 0,
            draft: String::new(),
            toast: "ready".into(),
            toast_ttl: 0,
            busy: false,
            blink: true,
            anim: 0,
            should_quit: false,
            wants_redraw: false,
            keys: super::keys::load(),
            pending: Pending::None,
            count: None,
            last_search: String::new(),
            view_height: 20,
        }
    }

    // --- what it is looking at ---

    pub fn file(&self) -> Option<&ChangedFile> {
        self.files
            .get(self.file_idx.min(self.files.len().saturating_sub(1)))
    }

    pub fn path(&self) -> &str {
        self.file().map_or("", |f| f.path.as_str())
    }

    pub fn diff_rows(&self) -> &[Row] {
        self.rows
            .get(self.path())
            .map_or(&[], std::vec::Vec::as_slice)
    }

    pub fn diff_state(&self) -> Load {
        self.rows_state
            .get(self.path())
            .cloned()
            .unwrap_or(Load::Idle)
    }

    pub fn row(&self) -> Option<&Row> {
        self.diff_rows().get(self.cursor)
    }

    /// The rows the selection covers: the cursor alone, or the visual range.
    pub fn span(&self) -> (usize, usize) {
        let a = self.anchor.unwrap_or(self.cursor);
        (a.min(self.cursor), a.max(self.cursor))
    }

    pub fn visual(&self) -> bool {
        self.anchor.is_some()
    }

    /// Every anchor the selection covers, skipping what cannot hold one.
    pub fn selected_anchors(&self) -> Vec<Anchor> {
        let (lo, hi) = self.span();
        let path = self.path().to_string();
        self.diff_rows()
            .iter()
            .skip(lo)
            .take(hi.saturating_sub(lo) + 1)
            .filter_map(|r| r.anchor(&path))
            .collect()
    }

    /// The comments attached to a row, if any.
    pub fn comments_at(&self, row: &Row) -> usize {
        let path = self.path();
        let Some(a) = row.anchor(path) else { return 0 };
        self.comments
            .iter()
            .filter(|c| c.anchors.contains(&a))
            .count()
    }

    /// Whether a row is the *first* line of a comment, which is where the
    /// badge goes: repeating it down a twelve-line note would be noise.
    pub fn comment_head_at(&self, row: &Row) -> usize {
        let path = self.path();
        let Some(a) = row.anchor(path) else { return 0 };
        self.comments
            .iter()
            .filter(|c| c.anchors.first() == Some(&a))
            .count()
    }

    /// What the agent picker offers: the agents that are up, then one entry
    /// per kind that could be started for this repository.
    ///
    /// Both in one list because the reader is answering one question — who
    /// gets this — and splitting it into two would make them ask it twice.
    pub fn agent_choices(&self) -> Vec<(String, bool)> {
        let mut out: Vec<(String, bool)> = self
            .agents
            .iter()
            .map(|a| (a.kind.clone(), false))
            .collect();
        out.extend(crate::config::agent_kinds().into_iter().map(|k| (k, true)));
        out
    }

    /// The widest line in the file being shown, in columns.
    ///
    /// The cap on how far right the code can scroll: past this there is
    /// nothing to see, and a pane of blank columns reads as a broken program
    /// rather than as the end of the text.
    pub fn longest_line(&self) -> usize {
        use unicode_width::UnicodeWidthStr;
        self.diff_rows()
            .iter()
            .map(|r| r.text.width())
            .max()
            .unwrap_or(0)
    }

    pub fn agent(&self) -> Option<&Agent> {
        self.agents.get(self.agent_idx)
    }

    pub fn blame_lines(&self) -> Option<&Vec<String>> {
        self.blame.get(self.path())
    }

    // --- talking to the worker ---

    /// Hands a request to the worker. `false` means it never got there.
    fn ask(&self, req: Request) -> bool {
        self.service.as_ref().is_some_and(|s| s.send(req))
    }

    /// What a state becomes when the worker cannot be reached.
    ///
    /// A thread that has died would otherwise leave every pane marked
    /// `Loading`, animating a skeleton over data that is never coming — a
    /// loader that cannot finish is worse than an error, because it looks
    /// like progress.
    fn gone() -> Load {
        Load::Failed(Arc::new(Error::Spawn {
            program: "the worker thread",
            source: std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "it is gone — restart diffline",
            ),
        }))
    }

    /// The next answer from the worker, if one has arrived.
    ///
    /// Takes `&mut self` because finding out the worker is gone is itself a
    /// state change: everything still waiting will wait for ever otherwise,
    /// which on screen is a skeleton animating over data that is not coming.
    pub fn poll(&mut self) -> Option<Response> {
        match self.service.as_ref().map(Service::poll) {
            Some(Ok(r)) => r,
            Some(Err(_)) => {
                self.worker_died();
                None
            }
            None => None,
        }
    }

    /// Fails everything that was waiting, once, when the worker goes.
    fn worker_died(&mut self) {
        if self.service.is_none() {
            return;
        }
        // Dropped so this runs once rather than on every frame from here on.
        self.service = None;
        self.busy = false;
        let gone = || Self::gone();
        if self.files_state.is_loading() {
            self.files_state = gone();
        }
        if self.agents_state.is_loading() {
            self.agents_state = gone();
        }
        for st in self.rows_state.values_mut() {
            if st.is_loading() {
                *st = gone();
            }
        }
        for st in self.blame_state.values_mut() {
            if st.is_loading() {
                *st = gone();
            }
        }
        for c in &mut self.comments {
            if c.state == crate::diffline::model::State::Sending {
                c.state = crate::diffline::model::State::Queued;
            }
        }
        self.flash("the worker thread is gone — restart diffline");
    }

    /// Is anything requested and unanswered? The loop waits less if so.
    pub fn waiting(&self) -> bool {
        self.busy
            || self.files_state.is_loading()
            || self.agents_state.is_loading()
            || self.rows_state.values().any(Load::is_loading)
            || self.blame_state.values().any(Load::is_loading)
    }

    /// Requests whatever the current view still needs. Idempotent: each piece
    /// is marked `Loading` before being asked for.
    pub fn ensure(&mut self) {
        if self.files_state == Load::Idle {
            self.files_state = Load::Loading;
            if !self.ask(Request::Files {
                repo: self.repo.clone(),
                scope: self.scope.clone(),
            }) {
                self.files_state = Self::gone();
            }
            return;
        }

        let path = self.path().to_string();
        if path.is_empty() {
            return;
        }
        if self.rows_state.get(&path).unwrap_or(&Load::Idle) == &Load::Idle {
            self.rows_state.insert(path.clone(), Load::Loading);
            let sent = self.ask(Request::Diff {
                repo: self.repo.clone(),
                scope: self.scope.clone(),
                path: path.clone(),
                context: self.context,
            });
            if !sent {
                self.rows_state.insert(path, Self::gone());
            }
            return;
        }

        // Blame is only fetched once it is being shown: it is a second walk
        // over the file's whole history, and most reading never wants it.
        if self.blame_on && self.blame_state.get(&path).unwrap_or(&Load::Idle) == &Load::Idle {
            self.blame_state.insert(path.clone(), Load::Loading);
            if !self.ask(Request::Blame {
                repo: self.repo.clone(),
                path: path.clone(),
            }) {
                self.blame_state.insert(path, Self::gone());
            }
        }

        // The agent list is wanted by the picker and by the footer that names
        // the target, so it is asked for once at the start.
        if self.agents_state == Load::Idle {
            self.agents_state = Load::Loading;
            if !self.ask(Request::Agents) {
                self.agents_state = Self::gone();
            }
        }
    }

    pub fn apply(&mut self, res: Response) {
        match res {
            Response::Files { scope, result } => {
                // A scope that has since changed has an answer nobody wants.
                if scope != self.scope {
                    return;
                }
                match result {
                    Ok(files) => {
                        self.files = files;
                        self.files_state = Load::Ready;
                        self.file_idx = 0;
                        self.cursor = 0;
                    }
                    Err(e) => {
                        self.flash(e.brief());
                        self.files_state = Load::Failed(Arc::new(e));
                    }
                }
            }

            Response::Diff {
                path,
                context,
                result,
            } => {
                // The context may have moved on while this was in flight.
                if context != self.context {
                    self.rows_state.insert(path, Load::Idle);
                    return;
                }
                match result {
                    Ok((rows, spans)) => {
                        if path == self.path() {
                            self.cursor = first_code(&rows, self.cursor);
                        }
                        self.rows.insert(path.clone(), rows);
                        self.spans.insert(path.clone(), spans);
                        self.rows_state.insert(path, Load::Ready);
                    }
                    Err(e) => {
                        self.rows_state.insert(path, Load::Failed(Arc::new(e)));
                    }
                }
            }

            Response::Blame { path, result } => match result {
                Ok(lines) => {
                    self.blame.insert(path.clone(), lines);
                    self.blame_state.insert(path, Load::Ready);
                }
                Err(e) => {
                    self.blame_state.insert(path, Load::Failed(Arc::new(e)));
                }
            },

            Response::Agents(result) => match result {
                Ok(agents) => {
                    self.agents = agents;
                    self.agents_state = Load::Ready;
                }
                Err(e) => {
                    self.agents_state = Load::Failed(Arc::new(e));
                }
            },

            Response::Wrote(result) => match result {
                Ok(said) => self.flash(said),
                Err(e) => self.flash(format!("could not write it: {e}")),
            },

            Response::Sent(result) => {
                self.busy = false;
                match result {
                    Ok(()) => self.on_sent(),
                    Err(e) => {
                        // The queue is put back: a note that did not arrive is
                        // still a note you meant to make.
                        for c in &mut self.comments {
                            c.state = State::Queued;
                        }
                        self.flash(format!("✗ {}", e.brief()));
                    }
                }
            }
        }
    }

    /// What the queue becomes once an agent has it.
    fn on_sent(&mut self) {
        let n = self.comments.len();
        let files = {
            let mut v: Vec<&str> = self.comments.iter().map(Comment::path).collect();
            v.sort_unstable();
            v.dedup();
            v.len()
        };
        let who = self.agent().map_or("the agent".into(), |a| a.kind.clone());
        self.comments.clear();
        self.queue_sel = 0;
        self.replies.push(format!(
            "{who} · accepted {n} note{}\nReading them across {files} file{}.",
            plural(n),
            plural(files)
        ));
        self.flash(format!("→ {n} sent to {who}"));
    }

    pub fn flash(&mut self, text: impl Into<String>) {
        self.toast = text.into();
        self.toast_ttl = 3;
    }

    /// One beat of the clock: the toast fades on it.
    pub fn tick(&mut self) {
        if self.toast_ttl > 0 {
            self.toast_ttl -= 1;
            if self.toast_ttl == 0 {
                self.toast = "ready".into();
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// The first row at or after `from` that a cursor can sit on.
///
/// A hunk header is a coordinate rather than a line, so the cursor steps over
/// it — landing there would offer to comment on `@@ -14,7 +14,9 @@`.
pub fn first_code(rows: &[Row], from: usize) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let mut i = from.min(rows.len() - 1);
    while i < rows.len() && !rows[i].kind.is_code() {
        i += 1;
    }
    i.min(rows.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffline::model::Kind;

    fn rows() -> Vec<Row> {
        vec![
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
                text: "one".into(),
            },
            Row {
                kind: Kind::Deleted,
                old: Some(2),
                new: None,
                text: "two".into(),
            },
            Row {
                kind: Kind::Added,
                old: None,
                new: Some(2),
                text: "TWO".into(),
            },
            Row {
                kind: Kind::Added,
                old: None,
                new: Some(3),
                text: "extra".into(),
            },
        ]
    }

    /// An app with a file and its rows already in place, and no worker: these
    /// are about the state machine, not about git.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/x".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
        );
        a.service = None;
        a.files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: super::super::model::Status::Modified,
            add: 2,
            del: 1,
        }];
        a.files_state = Load::Ready;
        a.rows.insert("src/a.rs".into(), rows());
        a.rows_state.insert("src/a.rs".into(), Load::Ready);
        a.cursor = 1;
        a
    }

    #[test]
    fn the_cursor_steps_over_a_hunk_header() {
        // there is nothing to say about `@@ -1,3 +1,4 @@`
        assert_eq!(first_code(&rows(), 0), 1);
    }

    #[test]
    fn first_code_of_nothing_is_zero_rather_than_a_panic() {
        assert_eq!(first_code(&[], 5), 0);
    }

    #[test]
    fn first_code_past_the_end_lands_on_the_last_row() {
        assert_eq!(first_code(&rows(), 99), rows().len() - 1);
    }

    #[test]
    fn without_a_visual_selection_the_span_is_the_cursor() {
        let a = app();
        assert_eq!(a.span(), (1, 1));
        assert!(!a.visual());
    }

    #[test]
    fn a_visual_selection_covers_both_ends_whichever_way_it_was_made() {
        let mut a = app();
        a.anchor = Some(4);
        a.cursor = 2;
        assert_eq!(a.span(), (2, 4), "selected upwards");
        a.anchor = Some(2);
        a.cursor = 4;
        assert_eq!(a.span(), (2, 4), "and downwards");
    }

    #[test]
    fn a_selection_yields_one_anchor_per_code_line() {
        let mut a = app();
        a.anchor = Some(1);
        a.cursor = 4;
        let anchors = a.selected_anchors();
        assert_eq!(anchors.len(), 4);
        assert_eq!(anchors[0].to_string(), "src/a.rs:n1");
        assert_eq!(anchors[1].to_string(), "src/a.rs:o2", "the deleted line");
        assert_eq!(anchors[3].to_string(), "src/a.rs:n3");
    }

    #[test]
    fn a_selection_that_includes_a_header_skips_it() {
        let mut a = app();
        a.anchor = Some(0); // the @@ line
        a.cursor = 1;
        assert_eq!(a.selected_anchors().len(), 1);
    }

    #[test]
    fn a_badge_sits_on_the_first_line_of_a_comment_only() {
        let mut a = app();
        a.anchor = Some(1);
        a.cursor = 3;
        let anchors = a.selected_anchors();
        a.comments.push(Comment {
            anchors,
            file: "src/a.rs".into(),
            snippet: "one".into(),
            body: "look at this".into(),
            state: State::Queued,
        });

        let rows = a.diff_rows().to_vec();
        assert_eq!(a.comment_head_at(&rows[1]), 1, "the first line carries it");
        assert_eq!(a.comment_head_at(&rows[2]), 0, "the rest do not");
        assert_eq!(a.comments_at(&rows[2]), 1, "but they are still covered");
    }

    #[test]
    fn an_answer_for_a_scope_that_has_moved_on_is_dropped() {
        let mut a = app();
        a.scope = Scope::Branch {
            base: "main".into(),
        };
        a.apply(Response::Files {
            scope: Scope::WorkingTree,
            result: Ok(vec![]),
        });
        assert_eq!(a.files.len(), 1, "the stale answer did not land");
    }

    #[test]
    fn an_answer_at_the_wrong_context_is_asked_for_again() {
        let mut a = app();
        a.context = 9;
        a.apply(Response::Diff {
            path: "src/a.rs".into(),
            context: 3,
            result: Ok((Vec::new(), Vec::new())),
        });
        assert_eq!(
            a.rows_state.get("src/a.rs"),
            Some(&Load::Idle),
            "so `ensure` fetches the width that is now wanted"
        );
        assert_eq!(a.diff_rows().len(), 5, "and the old rows still stand");
    }

    #[test]
    fn a_failure_keeps_the_error_rather_than_a_sentence_about_it() {
        // `Load::Failed(String)` threw away the type the whole program is
        // built on: `is_transient` could not be asked from here and `source`
        // was never walked anywhere.
        let mut a = app();
        a.apply(Response::Files {
            scope: Scope::WorkingTree,
            result: Err(crate::error::Error::Command {
                args: "git diff".into(),
                status: Some(1),
                stderr: "connection reset".into(),
            }),
        });
        assert!(a.files_state.failure().is_some(), "the error is kept whole");
        assert!(
            a.files_state.is_transient(),
            "a connection error is worth retrying, and now that is askable"
        );

        a.apply(Response::Files {
            scope: Scope::WorkingTree,
            result: Err(crate::error::Error::Spawn {
                program: "git",
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
        });
        assert!(
            !a.files_state.is_transient(),
            "a missing git is not, and offering a retry would be a lie"
        );
        assert_eq!(
            a.files_state.error().as_deref(),
            Some("git not found — is it installed?")
        );
    }

    #[test]
    fn a_worker_that_dies_mid_flight_does_not_leave_a_skeleton_turning() {
        // The send side already refused to leave a request in the air. The
        // receive side turned "the worker is gone" into "nothing yet", which
        // looks identical and lasts for ever.
        let mut a = app();
        a.service = Some(Service::spawn());
        a.files_state = Load::Loading;
        a.rows_state.insert("src/a.rs".into(), Load::Loading);

        // Dropping the worker's end of the channel is what dying looks like
        // from here.
        drop(a.service.take());
        a.service = Some(Service::dead());

        assert!(a.poll().is_none());
        assert!(!a.waiting(), "nothing may still be marked as on its way");
        assert!(a.files_state.failure().is_some());
        assert!(a.rows_state["src/a.rs"].failure().is_some());
    }

    #[test]
    fn a_send_that_failed_puts_the_queue_back() {
        let mut a = app();
        a.comments.push(Comment {
            file: "src/a.rs".into(),
            anchors: vec![Anchor {
                path: "src/a.rs".into(),
                side: super::super::model::Side::New,
                line: 1,
            }],
            snippet: String::new(),
            body: "x".into(),
            state: State::Sending,
        });
        a.busy = true;
        a.apply(Response::Sent(Err(crate::error::Error::Field {
            args: "agent prompt".into(),
            field: "pane",
        })));

        assert_eq!(
            a.comments.len(),
            1,
            "a note that did not arrive is not gone"
        );
        assert_eq!(a.comments[0].state, State::Queued);
        assert!(!a.busy);
    }

    #[test]
    fn a_send_that_worked_empties_the_queue_and_leaves_a_reply() {
        let mut a = app();
        a.agents = vec![Agent {
            kind: "claude".into(),
            status: crate::mux::AgentStatus::Idle,
            cwd: "/tmp/x".into(),
            pane: "wA:p1".into(),
            title: String::new(),
            focused: false,
        }];
        a.comments.push(Comment {
            file: "src/a.rs".into(),
            anchors: vec![Anchor {
                path: "src/a.rs".into(),
                side: super::super::model::Side::New,
                line: 1,
            }],
            snippet: String::new(),
            body: "x".into(),
            state: State::Sending,
        });
        a.apply(Response::Sent(Ok(())));

        assert!(a.comments.is_empty());
        assert_eq!(a.replies.len(), 1);
        assert!(a.replies[0].contains("claude"), "{}", a.replies[0]);
    }

    #[test]
    fn the_toast_goes_back_to_ready_on_its_own() {
        let mut a = app();
        a.flash("something happened");
        for _ in 0..3 {
            a.tick();
        }
        assert_eq!(a.toast, "ready");
    }
}
