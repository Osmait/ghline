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

mod input;
mod load;
mod select;

use std::collections::{HashMap, HashSet};

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
    /// The diff contents.
    DiffBody,
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
    }

    // --- selectores ---
}
