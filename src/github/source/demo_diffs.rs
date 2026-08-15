//! Demo diffs, extracted from the `GitHub TUI.dc.html` design.

use crate::github::data::StaticHunk;

const SRC_LAYOUT_SOLVER_RS: &[StaticHunk] = &[
    StaticHunk {
        hdr: "@@ -186,14 +186,26 @@ impl Solver {",
        lines: &[
            (' ', "fn split(&mut self, area: Rect) -> (Rect, Rect) {"),
            (' ', "    let cols = area.width;"),
            ('-', "    let sidebar = cols / 4;"),
            ('-', "    let content = cols - sidebar;"),
            (
                '+',
                "    // never let the sidebar collapse into a negative width",
            ),
            ('+', "    let sidebar = if cols < MIN_TOTAL {"),
            ('+', "        0"),
            ('+', "    } else {"),
            ('+', "        (cols / 4).clamp(MIN_SIDEBAR, MAX_SIDEBAR)"),
            ('+', "    };"),
            ('+', "    let content = cols.saturating_sub(sidebar);"),
            (' ', "    (Rect::new(0, 0, sidebar, area.height),"),
            (' ', "     Rect::new(sidebar, 0, content, area.height))"),
            (' ', "}"),
        ],
    },
    StaticHunk {
        hdr: "@@ -204,7 +216,9 @@ impl Solver {",
        lines: &[
            (' ', "pub fn resize(&mut self, area: Rect) {"),
            ('-', "    debug_assert!(area.width > 0);"),
            (
                '+',
                "    debug_assert_eq!(self.sidebar.max(MIN_SIDEBAR), MIN_SIDEBAR,",
            ),
            (
                '+',
                "        \"sidebar width must never go below {MIN_SIDEBAR}\");",
            ),
            (' ', "    self.dirty = true;"),
            (' ', "}"),
        ],
    },
];

const SRC_LAYOUT_MOD_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -1,6 +1,12 @@",
    lines: &[
        (' ', "mod solver;"),
        ('+', ""),
        ('+', "/// Minimum usable sidebar width, in columns."),
        ('+', "pub const MIN_SIDEBAR: u16 = 12;"),
        ('+', "pub const MAX_SIDEBAR: u16 = 48;"),
        ('+', "pub const MIN_TOTAL: u16 = 90;"),
        (' ', ""),
        (' ', "pub use solver::Solver;"),
    ],
}];

const SRC_APP_REDUCER_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -92,11 +92,20 @@ pub fn reduce(state: &mut State, msg: Msg) {",
    lines: &[
        (' ', "Msg::Resize(w, h) => {"),
        ('-', "    state.layout.resize(Rect::new(0, 0, w, h));"),
        ('+', "    if w < MIN_TOTAL {"),
        ('+', "        state.sidebar_visible = false;"),
        ('+', "    } else if !state.sidebar_visible {"),
        ('+', "        state.sidebar_visible = true;"),
        ('+', "    }"),
        ('+', "    state.layout.resize(Rect::new(0, 0, w, h));"),
        (' ', "    state.dirty = true;"),
        (' ', "}"),
    ],
}];

const SRC_UI_SIDEBAR_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -33,10 +33,14 @@ pub fn render(f: &mut Frame, area: Rect, s: &State) {",
    lines: &[
        ('-', "    if area.width == 0 { return; }"),
        (
            '+',
            "    if !s.sidebar_visible || area.width < MIN_SIDEBAR {",
        ),
        ('+', "        return;"),
        ('+', "    }"),
        (
            ' ',
            "    let block = Block::bordered().title(\"REPOSITORIES\");",
        ),
        (' ', "    f.render_widget(block, area);"),
    ],
}];

const TESTS_LAYOUT_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -0,0 +1,7 @@",
    lines: &[
        ('+', "#[test]"),
        ('+', "fn clamps_negative_width() {"),
        ('+', "    let mut s = Solver::default();"),
        (
            '+',
            "    let (side, main) = s.split(Rect::new(0, 0, 40, 20));",
        ),
        ('+', "    assert_eq!(side.width, 0);"),
        ('+', "    assert_eq!(main.width, 40);"),
        ('+', "}"),
    ],
}];

const CHANGELOG_MD: &[StaticHunk] = &[];

const SRC_ACTIONS_STREAM_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -0,0 +1,18 @@",
    lines: &[
        ('+', "pub struct LogStream {"),
        ('+', "    rx: mpsc::Receiver<LogChunk>,"),
        ('+', "    follow: bool,"),
        ('+', "}"),
        ('+', ""),
        ('+', "impl LogStream {"),
        (
            '+',
            "    pub async fn poll(&mut self) -> Option<LogChunk> {",
        ),
        ('+', "        tokio::select! {"),
        ('+', "            chunk = self.rx.recv() => chunk,"),
        (
            '+',
            "            _ = tokio::time::sleep(POLL_EVERY) => Some(LogChunk::Keepalive),",
        ),
        ('+', "        }"),
        ('+', "    }"),
        ('+', "}"),
    ],
}];

const SRC_ACTIONS_MOD_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -1,4 +1,7 @@",
    lines: &[
        (' ', "mod runs;"),
        ('+', "mod stream;"),
        (' ', ""),
        ('+', "pub use stream::{LogStream, LogChunk};"),
        (' ', "pub use runs::WorkflowRun;"),
    ],
}];

const SRC_UI_LOGS_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -48,9 +48,21 @@ pub fn render(f: &mut Frame, area: Rect, s: &State) {",
    lines: &[
        (' ', "let lines = s.logs.visible(area.height);"),
        ('-', "let list = List::new(lines);"),
        (
            '+',
            "let list = List::new(lines).highlight_style(ERROR_STYLE);",
        ),
        ('+', "if s.logs.follow {"),
        ('+', "    s.logs.scroll_to_bottom(area.height);"),
        ('+', "}"),
        (
            ' ',
            "f.render_stateful_widget(list, area, &mut s.logs.state);",
        ),
    ],
}];

const CARGO_TOML: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -18,7 +18,7 @@ [dependencies]",
    lines: &[
        (' ', "ratatui = \"0.29\""),
        ('-', "crossterm = \"0.27.0\""),
        ('+', "crossterm = \"0.28.1\""),
        (' ', "tokio = { version = \"1\", features = [\"full\"] }"),
    ],
}];

const CARGO_LOCK: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -142,9 +142,9 @@",
    lines: &[
        (' ', "[[package]]"),
        (' ', "name = \"crossterm\""),
        ('-', "version = \"0.27.0\""),
        ('+', "version = \"0.28.1\""),
        (
            '-',
            "checksum = \"f476fe445d41c9e991fd07515a6f463074b782242ccf4a5b7b1d1012e70824df\"",
        ),
        (
            '+',
            "checksum = \"829d955a0bb380ef178a640b91779e3987da38c9aea133b20614cfed8cdea9c6\"",
        ),
    ],
}];

const SRC_AUTH_HOSTS_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -0,0 +1,12 @@",
    lines: &[
        ('+', "pub struct Host {"),
        (
            '+',
            "    pub name: String,      // github.com | ghe.acme.dev",
        ),
        ('+', "    pub token: Secret,"),
        ('+', "    pub sso: bool,"),
        ('+', "}"),
        ('+', ""),
        ('+', "pub fn load() -> Vec<Host> {"),
        ('+', "    hosts_yml().unwrap_or_default()"),
        ('+', "}"),
    ],
}];

const SRC_AUTH_SWITCHER_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -0,0 +1,9 @@",
    lines: &[
        ('+', "pub fn switch(state: &mut State, idx: usize) {"),
        ('+', "    state.account = idx;"),
        ('+', "    state.repos = Repos::pending();"),
        ('+', "    state.dispatch(Msg::FetchRepos(idx));"),
        ('+', "}"),
    ],
}];

const SRC_KEYMAP_LIB_RS: &[StaticHunk] = &[StaticHunk {
    hdr: "@@ -0,0 +1,11 @@",
    lines: &[
        ('+', "//! Vim-style keymap, extracted from the app crate."),
        ('+', "pub enum Motion { Down(u16), Up(u16), Top, Bottom }"),
        ('+', ""),
        ('+', "pub fn parse(seq: &str) -> Option<Motion> {"),
        ('+', "    let (count, key) = split_count(seq);"),
        (
            '+',
            "    match key { \"j\" => Some(Motion::Down(count)), \"k\" => Some(Motion::Up(count)), _ => None }",
        ),
        ('+', "}"),
    ],
}];

/// The hunks of the given file, or empty if the design defines none.
pub fn demo_diff(path: &str) -> &'static [StaticHunk] {
    match path {
        "src/layout/solver.rs" => SRC_LAYOUT_SOLVER_RS,
        "src/layout/mod.rs" => SRC_LAYOUT_MOD_RS,
        "src/app/reducer.rs" => SRC_APP_REDUCER_RS,
        "src/ui/sidebar.rs" => SRC_UI_SIDEBAR_RS,
        "tests/layout.rs" => TESTS_LAYOUT_RS,
        "CHANGELOG.md" => CHANGELOG_MD,
        "src/actions/stream.rs" => SRC_ACTIONS_STREAM_RS,
        "src/actions/mod.rs" => SRC_ACTIONS_MOD_RS,
        "src/ui/logs.rs" => SRC_UI_LOGS_RS,
        "Cargo.toml" => CARGO_TOML,
        "Cargo.lock" => CARGO_LOCK,
        "src/auth/hosts.rs" => SRC_AUTH_HOSTS_RS,
        "src/auth/switcher.rs" => SRC_AUTH_SWITCHER_RS,
        "src/keymap/lib.rs" => SRC_KEYMAP_LIB_RS,
        _ => &[],
    }
}
