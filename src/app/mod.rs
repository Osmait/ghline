//! Application state: a 1:1 port of the design's `Component` class.
//!
//! Split by responsibility rather than by type, because that is how it is
//! read: what the state *is* lives here, what it *answers* in `select`, what
//! it *fetches* in `load`, and how it *reacts* in `input`.

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests;

pub mod hit;
mod input;
mod load;
mod mouse;
mod select;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use self::hit::Region;
use crate::actions::{Flash, Prompt};
use crate::data::{Account, Item, Job, RawLog, Status};
use crate::demo;
use crate::service::{Response, Service};

/// Where the data comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The design's fake data, with no network.
    Demo,
    /// The `gh` CLI against real GitHub.
    Live,
}

/// Load state of one piece of data.
#[derive(Clone, PartialEq, Eq)]
pub enum Load {
    Idle,
    Loading,
    Ready,
    Failed(String),
}

impl Load {
    pub fn is_loading(&self) -> bool {
        *self == Self::Loading
    }
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    List,
    Detail,
    /// A PR's changed files and their diff.
    Diff,
    Logs,
}

/// The navigable panes. `h`/`l` move between the ones the current view has,
/// and `j`/`k` always act on whichever one has focus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The repository sidebar.
    Repos,
    /// The issue / PR / run list.
    List,
    /// The issue body or the PR description.
    Body,
    /// The checks pane of a PR or a run.
    Checks,
    /// The jobs and steps tree.
    Tree,
    /// The log output.
    Log,
    /// The changed-files list.
    Files,
    /// The list of running agents.
    Agents,
    /// The repository's file tree.
    FileTree,
    /// The contents of the selected file.
    FileView,
    /// The diff contents.
    DiffBody,
}

/// Somewhere an issue can be sent.
#[derive(Clone)]
pub enum Dest {
    /// An agent already running, addressed by the pane it lives in.
    Running(crate::data::Agent),
    /// A new agent on a local clone.
    ///
    /// Either in a worktree branched off it, or in the checkout itself. The
    /// difference is one field because it is one decision — where the agent's
    /// edits land — and everything else about the two is identical.
    Fresh {
        /// `claude`, `codex`, … — what to start.
        kind: String,
        /// The checkout to work from.
        repo_root: String,
        /// The branch the checkout has out, when working in it directly.
        /// `None` means a worktree of its own.
        in_place: Option<String>,
    },
    /// The repository is not on this disk, so nothing can be started in it.
    /// Listed rather than omitted, because "clone it first" is an answer and
    /// a shorter list is not.
    NotCloned(String),
}

impl Dest {
    pub fn title(&self) -> String {
        match self {
            Self::Running(a) => format!("{} {}  ·  {}", a.icon(), a.kind, a.where_short()),
            Self::Fresh {
                kind,
                in_place: Some(branch),
                ..
            } => format!(
                "{} new {kind} on {branch}, in the checkout",
                crate::config::agent_icon(kind)
            ),
            Self::Fresh { kind, .. } => format!(
                "{} new worktree with {kind}",
                crate::config::agent_icon(kind)
            ),
            Self::NotCloned(repo) => format!("{repo} is not on this machine"),
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Running(a) => format!("{}   {}", a.cwd, a.pane),
            // Working in place is the one destination that can collide with
            // the reader: same files, same branch, no isolation. Saying so is
            // the whole reason the detail line exists.
            Self::Fresh {
                repo_root,
                in_place: Some(_),
                ..
            } => format!("{repo_root}   ·   alongside your own work, uncommitted changes included"),
            Self::Fresh { repo_root, .. } => repo_root.clone(),
            Self::NotCloned(repo) => format!("gh repo clone {repo}"),
        }
    }

    /// Why this destination cannot take the issue, if it cannot.
    ///
    /// Returned rather than filtered so the row is still listed: "claude is
    /// busy" is a more useful answer than a list that quietly leaves it out.
    pub fn refusal(&self) -> Option<String> {
        match self {
            Self::Running(a) if a.focused => {
                Some("this window — it is the one showing you this list".into())
            }
            Self::Running(a) if !a.status.is_free() => Some(format!(
                "{} — interrupting would lose its context",
                a.status
            )),
            Self::NotCloned(_) => Some("an agent needs a checkout — clone it and try again".into()),
            _ => None,
        }
    }
}

/// The half of a fresh dispatch that the confirmation dialog has no use for.
///
/// `Prompt::Dispatch` carries what the reader is being asked about; this
/// carries what the service needs if they say yes. Kept apart so the dialog
/// stays about the question.
#[derive(Clone)]
pub struct Fresh {
    pub repo_root: String,
    /// The branch to create, or `None` to work in the checkout as it is.
    pub branch: Option<String>,
    pub label: String,
    pub kind: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    Colon,
    Slash,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Job,
    Step,
}

pub struct TreeNode {
    pub kind: NodeKind,
    pub ji: usize,
    pub name: String,
    pub status: Status,
    pub dur: String,
}

pub struct LogRow {
    pub n: usize,
    /// Real HH:MM:SS when the log comes from GitHub; synthetic in demo mode.
    pub time: String,
    pub text: String,
    pub kind: &'static str,
}

pub struct App {
    pub source: Source,
    pub service: Option<Service>,
    pub accounts: Vec<Account>,
    pub accounts_state: Load,
    /// Load state of each account's repos, keyed by login.
    pub repos_state: HashMap<String, Load>,
    /// Lists keyed by `owner/repo` and tab; this is what the actions mutate.
    pub lists: HashMap<(String, usize), Vec<Item>>,
    pub lists_state: HashMap<(String, usize), Load>,
    /// A workflow run's jobs, keyed by `owner/repo` and run id.
    pub jobs_by_run: HashMap<(String, i64), Vec<Job>>,
    pub jobs_state: HashMap<(String, i64), Load>,
    /// A run's full log, not yet split per job.
    pub raw_logs: HashMap<(String, i64), Vec<RawLog>>,
    pub logs_state: HashMap<(String, i64), Load>,
    /// Load state of each PR's diff.
    pub diff_state: HashMap<(String, i64), Load>,
    /// Load state of the body, files and reviews of each item, which arrive
    /// separately from the list that names them.
    pub detail_state: HashMap<(String, i64), Load>,
    /// A write action is in flight.
    pub busy: bool,
    /// Frame counter for the loading skeletons. It only advances while
    /// something is actually being waited on.
    pub anim: u64,
    pub prompt: Option<Prompt>,
    pub flash: Option<Flash>,
    pub acc: usize,
    pub repo: usize,
    pub tab: usize,
    pub pane: Pane,
    pub item: usize,
    pub view: View,
    pub check: usize,
    pub tree_sel: usize,
    pub collapsed: HashSet<usize>,
    pub follow: bool,
    pub extra_lines: usize,
    pub accounts_open: bool,
    pub acc_sel: usize,
    /// Whether the repository pane is wanted. The view and the terminal width
    /// have the last word: see `sidebar_shown`.
    pub sidebar: bool,
    /// Whether the last frame actually drew it. The pane list reads this, so
    /// `h` can never land on a sidebar that is not on screen.
    pub sidebar_shown: bool,
    /// The finder, open over everything else while it is up.
    pub finder_open: bool,
    pub finder_source: crate::finder::Source,
    pub finder_query: String,
    pub finder_sel: usize,
    pub finder_scroll: usize,
    /// Results of the last remote search, and its state.
    pub finder_hits: Vec<crate::finder::Hit>,
    pub finder_state: Load,
    /// The query the last request carried, so a stale answer is not shown and
    /// an unchanged query is not asked for twice.
    pub finder_sent: String,
    /// Theme picker. It previews as you move, so the theme active when it
    /// opened is kept in order to put it back on `esc`.
    pub themes_open: bool,
    pub theme_sel: usize,
    pub theme_before: crate::theme::Theme,
    pub help_open: bool,
    pub cmd: Option<Cmd>,
    pub cmd_text: String,
    pub filter: String,
    pub log_filter: String,
    pub tick: u64,
    pub blink: bool,
    pub should_quit: bool,
    // scroll offsets of the scrollable panes
    pub repo_scroll: usize,
    pub item_scroll: usize,
    pub tree_scroll: usize,
    pub log_scroll: usize,
    /// Selected file in the diff view.
    pub file_idx: usize,
    /// Two-column diff instead of unified (`s`).
    pub split: bool,
    /// Ignore whitespace-only changes (`w`).
    pub ws: bool,
    pub diff_scroll: usize,
    /// Scroll offset of the issue body / PR description.
    pub detail_scroll: usize,
    /// True height of the scrollable pane, which only the render knows.
    pub detail_height: u16,
    /// Every agent herdr is running, and whether we have asked yet. Not keyed
    /// by repository: agents belong to the machine, not to a repo.
    pub agents: Vec<crate::data::Agent>,
    pub agents_state: Load,
    /// A repository's file tree, keyed by `owner/repo`.
    pub trees: HashMap<String, Vec<crate::data::TreeEntry>>,
    pub trees_state: HashMap<String, Load>,
    /// File contents, keyed by `owner/repo` and path.
    pub file_text: HashMap<(String, String), String>,
    pub file_state: HashMap<(String, String), Load>,
    /// Colour spans per line, lexed once when the file lands rather than on
    /// every frame: a block comment means a line cannot be read on its own,
    /// so the whole file is done at once.
    pub file_spans: HashMap<(String, String), Vec<Vec<crate::syntax::Span>>>,
    /// Which directories have been opened. Expanded rather than collapsed, so
    /// a repository starts showing its top level and nothing more.
    pub fs_open: HashSet<String>,
    pub fs_sel: usize,
    pub fs_scroll: usize,
    /// The source line the cursor is on, zero-based. A line rather than a
    /// scroll offset because it is what `E` hands to the editor, and a reader
    /// should be able to see which line that will be.
    pub file_sel: usize,
    pub file_scroll: usize,
    /// Set when `E` was pressed before the disk had been walked. The scan is
    /// asked for and the editor opens when it lands.
    pub wants_edit: bool,
    /// Set by `^l`. Only the main loop can act on it: ratatui draws the
    /// difference between two buffers, so nothing inside a frame can make it
    /// repaint a cell it believes is already correct.
    pub wants_redraw: bool,
    /// Set when `E` has been pressed: the file to open and the line to open it
    /// at. The main loop picks this up, because leaving the alternate screen
    /// and coming back is the terminal's business, not the reducer's.
    pub edit_request: Option<(std::path::PathBuf, usize)>,
    /// `owner/repo` → checkout, built by walking the disk once.
    pub clones: crate::clones::Index,
    pub clones_state: Load,
    /// Set when the pending confirmation is for a worktree rather than an
    /// agent that already exists.
    pub pending_fresh: Option<Fresh>,
    /// The dispatch picker, open over an issue or pull request.
    pub dispatch_open: bool,
    /// A specific instruction typed into the picker, or empty for the
    /// template on its own.
    pub dispatch_note: String,
    pub dispatch_sel: usize,
    pub dispatch_scroll: usize,
    /// Selected row of the Agents tab.
    pub agent_sel: usize,
    pub agent_scroll: usize,
    /// What the last frame drew and where, for the mouse to aim at. Rebuilt
    /// every frame; empty before the first one, which is why a click that
    /// arrives before a render simply does nothing.
    pub hits: Vec<Region>,
    /// Where and when the last click landed, for spotting a double click.
    pub last_click: Option<(u16, u16, Instant)>,
}

impl App {
    pub fn new(source: Source) -> Self {
        let accounts = if source == Source::Demo {
            demo::accounts()
        } else {
            Vec::new()
        };
        let mut lists = HashMap::new();
        let mut lists_state = HashMap::new();
        if source == Source::Demo {
            for a in &accounts {
                for (r, repo) in a.repos.iter().enumerate() {
                    let key = format!("{}/{}", a.login, repo.name);
                    lists.insert((key.clone(), 0), demo::issues(r));
                    lists.insert((key.clone(), 1), demo::prs(r));
                    lists.insert((key.clone(), 2), demo::runs(r));
                    for t in 0..3 {
                        lists_state.insert((key.clone(), t), Load::Ready);
                    }
                }
            }
        }
        Self {
            source,
            service: (source == Source::Live).then(Service::spawn),
            accounts_state: if source == Source::Demo {
                Load::Ready
            } else {
                Load::Idle
            },
            repos_state: HashMap::new(),
            lists,
            lists_state,
            jobs_by_run: HashMap::new(),
            jobs_state: HashMap::new(),
            raw_logs: HashMap::new(),
            logs_state: HashMap::new(),
            diff_state: HashMap::new(),
            detail_state: HashMap::new(),
            busy: false,
            anim: 0,
            accounts,
            prompt: None,
            flash: None,
            acc: 0,
            // the design starts on the third repo; with real data, on the first
            repo: if source == Source::Demo { 2 } else { 0 },
            tab: 1, // 'prs'
            pane: Pane::List,
            item: 0,
            view: View::List,
            check: 0,
            tree_sel: 0,
            collapsed: HashSet::new(),
            follow: true,
            extra_lines: 0,
            accounts_open: false,
            acc_sel: 0,
            finder_open: false,
            finder_source: crate::finder::Source::Repos,
            finder_query: String::new(),
            finder_sel: 0,
            finder_scroll: 0,
            finder_hits: Vec::new(),
            finder_state: Load::Ready,
            finder_sent: String::new(),
            // hidden by default: with sixty repositories it is a wall, and
            // `[`/`]` plus the finder reach any of them without it
            sidebar: false,
            sidebar_shown: false,
            themes_open: false,
            theme_sel: 0,
            theme_before: crate::theme::current(),
            help_open: false,
            cmd: None,
            cmd_text: String::new(),
            filter: String::new(),
            log_filter: String::new(),
            tick: 0,
            blink: true,
            should_quit: false,
            repo_scroll: 0,
            item_scroll: 0,
            tree_scroll: 0,
            log_scroll: 0,
            file_idx: 0,
            split: false,
            ws: false,
            diff_scroll: 0,
            detail_scroll: 0,
            detail_height: 10,
            trees: HashMap::new(),
            trees_state: HashMap::new(),
            file_text: HashMap::new(),
            file_state: HashMap::new(),
            file_spans: HashMap::new(),
            fs_open: HashSet::new(),
            fs_sel: 0,
            fs_scroll: 0,
            file_sel: 0,
            file_scroll: 0,
            wants_edit: false,
            wants_redraw: false,
            edit_request: None,
            clones: crate::clones::Index::new(),
            clones_state: Load::Idle,
            pending_fresh: None,
            dispatch_open: false,
            dispatch_note: String::new(),
            dispatch_sel: 0,
            dispatch_scroll: 0,
            agents: Vec::new(),
            agents_state: Load::Idle,
            agent_sel: 0,
            agent_scroll: 0,
            hits: Vec::new(),
            last_click: None,
        }
    }

    pub fn live(&self) -> bool {
        self.source == Source::Live
    }

    pub fn poll_service(&self) -> Option<Response> {
        self.service
            .as_ref()
            .and_then(super::service::Service::poll)
    }

    /// Is anything requested and still unanswered? The loop waits less if so.
    pub fn waiting(&self) -> bool {
        self.busy
            || self.accounts_state.is_loading()
            || self.repos_state.values().any(Load::is_loading)
            || self.lists_state.values().any(Load::is_loading)
            || self.jobs_state.values().any(Load::is_loading)
            || self.logs_state.values().any(Load::is_loading)
            || self.diff_state.values().any(Load::is_loading)
            || self.detail_state.values().any(Load::is_loading)
            || self.finder_state.is_loading()
            || self.agents_state.is_loading()
            || self.clones_state.is_loading()
            || self.trees_state.values().any(Load::is_loading)
            || self.file_state.values().any(Load::is_loading)
    }

    // --- selectores ---
}
