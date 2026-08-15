//! A fixed review, for rendering without a repository.
//!
//! github-tui has had one of these since the design was ported: `--demo` runs
//! it on a fixture, and the golden frames compare against that. diffline had
//! nothing equivalent, because what it reviews is whatever `git` says is in
//! front of you — which is exactly what a test cannot depend on.
//!
//! So the fixture is written here instead: two files, a diff with each kind of
//! row in it, a queued comment and an agent to send it to. No `git`, no disk,
//! no clock. `Worker` is `None`, so nothing is ever asked and nothing ever
//! arrives; what is in the app is what was put there.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::diffline::app::{App, Load};
use crate::diffline::model::{ChangedFile, Comment, Kind, Row, Scope, State, Status};
use crate::shared::key::parse_keys;
use crate::shared::mux::{Agent, AgentStatus};

fn row(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> Row {
    Row {
        kind,
        old,
        new,
        text: text.to_string(),
    }
}

/// The fixture, with `keys` applied to it.
///
/// Deterministic on any machine: the only things that could vary are the
/// config and the clock, and neither is read. `blink` is set for the same
/// reason github-tui's demo sets it — a cursor is on in half the frames it
/// appears in, and the half worth looking at is the one where it shows.
pub fn demo(keys: &str) -> App {
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));

    let mut app = App::new(
        "/demo/tuikit".into(),
        Scope::WorkingTree,
        vec![
            Scope::WorkingTree,
            Scope::Branch {
                base: "main".into(),
            },
        ],
        None,
    );

    app.files = vec![
        ChangedFile {
            path: "src/layout/solver.rs".into(),
            status: Status::Modified,
            add: 12,
            del: 4,
        },
        ChangedFile {
            path: "src/ui/sidebar.rs".into(),
            status: Status::Added,
            add: 7,
            del: 0,
        },
    ];
    app.files_state = Load::Ready;

    // Every kind of row, because the kinds are what the diff pane colours and
    // numbers differently: a header carries no line number, a deletion has no
    // new-side one, an addition has no old-side one.
    app.rows.insert(
        "src/layout/solver.rs".into(),
        vec![
            row(
                Kind::Header,
                None,
                None,
                "@@ -186,6 +186,10 @@ impl Solver {",
            ),
            row(
                Kind::Context,
                Some(186),
                Some(186),
                "    fn split(&mut self, area: Rect) -> (Rect, Rect) {",
            ),
            row(
                Kind::Context,
                Some(187),
                Some(187),
                "        let cols = area.width;",
            ),
            row(
                Kind::Deleted,
                Some(188),
                None,
                "        let sidebar = cols / 4;",
            ),
            row(
                Kind::Added,
                None,
                Some(188),
                "        // never let the sidebar collapse into a negative width",
            ),
            row(
                Kind::Added,
                None,
                Some(189),
                "        let sidebar = (cols / 4).clamp(MIN_SIDEBAR, MAX_SIDEBAR);",
            ),
            row(
                Kind::Context,
                Some(189),
                Some(190),
                "        let content = cols.saturating_sub(sidebar);",
            ),
        ],
    );
    app.rows_state
        .insert("src/layout/solver.rs".into(), Load::Ready);

    app.comments.push(Comment {
        anchors: vec![],
        file: "src/layout/solver.rs".into(),
        snippet: "let sidebar = cols / 4;".into(),
        body: "this clamp wants a test at cols = 0".into(),
        state: State::Queued,
    });

    app.agents = vec![
        Agent {
            kind: "claude".into(),
            status: AgentStatus::Idle,
            focused: false,
            cwd: "/demo/tuikit".into(),
            pane: "wK:p1".into(),
            title: "waiting".into(),
        },
        Agent {
            kind: "codex".into(),
            status: AgentStatus::Working,
            focused: false,
            cwd: "/demo/other".into(),
            pane: "wK:p2".into(),
            title: "rewriting the reducer".into(),
        },
    ];
    app.agents_state = Load::Ready;

    for k in parse_keys(keys) {
        app.on_key(k);
    }
    app.blink = true;
    app
}

/// One frame of the fixture as plain text, a row per line and no colour.
pub fn frame(keys: &str, width: u16, height: u16) -> std::io::Result<String> {
    let mut app = demo(keys);
    let mut term = Terminal::new(TestBackend::new(width, height))?;
    term.draw(|f| super::draw(f, &mut app))?;
    Ok(crate::tui::probe::screen(&term))
}
