//! Deterministic data for the tests, and nothing else.
//!
//! This replaced the design's demo data set, which was nine hundred lines of
//! plausible GitHub — accounts, issues, pull requests, runs, job templates,
//! canned logs and diffs — kept behind a feature so it would not ship. It
//! existed to drive a `--demo` mode a reader could start; when that mode went,
//! what was left was scaffolding for the state tests and the golden frames,
//! and scaffolding wants to be small and boring rather than convincing.
//!
//! So: two accounts, seven repositories, and rows built from a table instead
//! of written out one at a time. No feature gate, because at this size the
//! flag cost more to carry than the data does. Nothing here reads a clock, a
//! config or the network, which is what lets a golden frame be compared
//! character for character.
//!
//! What the shape has to satisfy, since it is not arbitrary: five repositories
//! on the first account (the finder walks to the fifth), a pull request list
//! long enough to scroll a short terminal, a draft at index 3 and a merged one
//! at index 4 (the merge refusals), a changed file with no hunks last (the
//! empty-diff case), and jobs on every run (a log excerpt names the job it
//! came from).

use crate::github::data::{
    Account, Detail, FileChange, Hunk, Item, Job, Label, LogKind, PrDetail, RawLog, Repo, Review,
    ReviewState, RunDetail, StaticHunk, Status, Step,
};
use crate::github::state::app::{App, Load};

/// The account list a seeded [`app`] starts with.
pub fn accounts() -> Vec<Account> {
    fn repo(name: &str, private: bool, lang: &str, issues: u32, prs: u32) -> Repo {
        Repo {
            name: name.into(),
            private,
            lang: lang.into(),
            issues,
            prs,
            has_workflows: true,
        }
    }
    vec![
        Account {
            login: "marasanz".into(),
            kind: "(personal)".into(),
            sub: "github.com · token: gho_****4f2a".into(),
            repos: vec![
                repo("ripgrep-ui", false, "Rust", 7, 2),
                repo("dotfiles", true, "Shell", 1, 0),
                repo("tuikit", false, "Go", 12, 4),
                repo("notes.md", true, "Python", 0, 1),
                repo("aoc-2025", false, "Rust", 2, 0),
            ],
        },
        Account {
            login: "acme-platform".into(),
            kind: "(org · ghe)".into(),
            sub: "ghe.acme.dev · SSO active".into(),
            repos: vec![
                repo("edge-router", true, "Go", 34, 11),
                repo("billing-core", true, "Elixir", 21, 6),
            ],
        },
    ]
}

/// Issues for the repository at `repo`.
///
/// The index shifts the numbers so two repositories never look like the same
/// list, which is what the tests about moving between them need to see.
pub fn issues(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    let rows: &[(i64, &str, Status, &str, &str, &str)] = &[
        (
            412,
            "Panic on resize when the sidebar is narrow",
            Status::Open,
            "lmoreno",
            "2h ago",
            "bug",
        ),
        (
            408,
            "Add a fuzzy filter to the repository pane",
            Status::Open,
            "kdev",
            "9h ago",
            "feature",
        ),
        (
            401,
            "Truecolor fallback is wrong on Apple Terminal",
            Status::Open,
            "sofi",
            "1d ago",
            "bug",
        ),
        (
            397,
            "Document the :account command in the README",
            Status::Open,
            "marasanz",
            "2d ago",
            "docs",
        ),
        (
            391,
            "Wrap long labels instead of clipping them",
            Status::Open,
            "kdev",
            "3d ago",
            "feature",
        ),
        (
            388,
            "Scrollbar thumb disappears on short lists",
            Status::Closed,
            "tsuki",
            "5d ago",
            "bug",
        ),
    ];
    rows.iter()
        .map(|(num, title, state, author, when, label)| {
            let mut it = Item::issue();
            it.num = num - r;
            it.title = (*title).into();
            it.state = *state;
            it.author = (*author).into();
            it.when = (*when).into();
            it.body = "The layout solver could compute a negative width when the\nterminal was resized while the sidebar had focus.".into();
            it.labels = vec![Label::new(label, (0xf0, 0x71, 0x78))];
            it
        })
        .collect()
}

/// Pull requests for the repository at `repo`.
///
/// Twelve of them, which is more than a short terminal draws — the mouse tests
/// need a list that actually scrolls, and a list that fits is a list where
/// scrolling is untested. Index 3 is the draft and index 4 the merged one, and
/// the two merge-refusal tests address them by position.
pub fn prs(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    let rows: &[(i64, &str, Status, Status, &str, &str, &str)] = &[
        (
            219,
            "fix(layout): clamp the sidebar to a minimum width",
            Status::Open,
            Status::Failure,
            "marasanz",
            "26m ago",
            "fix/sidebar-clamp",
        ),
        (
            216,
            "feat(finder): rank by subsequence, not prefix",
            Status::Open,
            Status::Success,
            "kdev",
            "3h ago",
            "feat/fuzzy-rank",
        ),
        (
            214,
            "perf(render): reuse the cell buffer between frames",
            Status::Open,
            Status::Success,
            "sofi",
            "5h ago",
            "perf/reuse-buffer",
        ),
        (
            213,
            "wip: split the reducer by view",
            Status::Draft,
            Status::Pending,
            "tsuki",
            "6h ago",
            "wip/split-reducer",
        ),
        (
            211,
            "chore: drop the unused ansi crate",
            Status::Merged,
            Status::Success,
            "tsuki",
            "2d ago",
            "chore/deps",
        ),
        (
            209,
            "fix(logs): keep the filter across a job change",
            Status::Open,
            Status::Success,
            "lmoreno",
            "2d ago",
            "fix/log-filter",
        ),
        (
            207,
            "feat(diff): word-level highlighting",
            Status::Open,
            Status::Running,
            "kdev",
            "3d ago",
            "feat/word-diff",
        ),
        (
            204,
            "docs: write down the key map",
            Status::Open,
            Status::Success,
            "marasanz",
            "4d ago",
            "docs/keymap",
        ),
        (
            201,
            "fix(finder): do not steal the escape key",
            Status::Open,
            Status::Failure,
            "sofi",
            "5d ago",
            "fix/finder-esc",
        ),
        (
            198,
            "test: golden frames for the narrow layout",
            Status::Open,
            Status::Success,
            "tsuki",
            "6d ago",
            "test/narrow",
        ),
        (
            195,
            "refactor: move theming out of the model",
            Status::Open,
            Status::Success,
            "kdev",
            "1w ago",
            "refactor/theme",
        ),
        (
            191,
            "build: pin the toolchain",
            Status::Closed,
            Status::Success,
            "marasanz",
            "2w ago",
            "build/pin",
        ),
    ];
    rows.iter()
        .enumerate()
        .map(|(i, (num, title, state, checks, author, when, branch))| {
            let mut it = Item::pr();
            let mut d = PrDetail::default();
            it.num = num - r;
            it.title = (*title).into();
            it.state = *state;
            it.author = (*author).into();
            it.when = (*when).into();
            it.body = "The layout solver could compute a negative width when the\nterminal was resized while the sidebar had focus.\n\n- clamp the sidebar to MIN_SIDEBAR (12 cols)\n- collapse it entirely below 90 columns\n\nCloses #412".into();
            // The label follows the conventional-commit prefix, so the list
            // is not a column of the same word.
            let (label, rgb) = match title.split(&['(', ':'][..]).next().unwrap_or("") {
                "fix" => ("bug", (0xf0, 0x71, 0x78)),
                "feat" => ("feature", (0x7f, 0xd9, 0x62)),
                "docs" => ("docs", (0xff, 0xb4, 0x54)),
                _ => ("chore", (0x8b, 0x94, 0x9e)),
            };
            it.labels = vec![Label::new(label, rgb)];
            d.checks = *checks;
            d.branch = (*branch).into();
            // Every row shows a diffstat, so every row has one; the numbers
            // are derived from the position rather than invented one at a
            // time, which is all a fixture owes a column of digits.
            d.add = format!("+{}", 12 + i * 17);
            d.del = format!("-{}", 3 + i * 5);
            d.files = 1 + (i as u32 % 4);
            // Only the first one is opened by the frame that draws a diff;
            // giving all twelve a file list would be data nothing reads.
            if i == 0 {
                d.file_list = vec![
                    changed("src/layout/solver.rs", "+64", "-18"),
                    changed("src/layout/mod.rs", "+12", "-4"),
                    changed("tests/layout.rs", "+7", "-0"),
                    // Last, and with no hunks: a mode change or a rename has
                    // nothing to draw, and that is its own case.
                    FileChange {
                        path: "CHANGELOG.md".into(),
                        add: "+0".into(),
                        del: "-0".into(),
                        hunks: Vec::new(),
                    },
                ];
                d.files = 4;
                d.reviews = vec![
                    Review {
                        author: "tsuki".into(),
                        state: ReviewState::ChangesRequested,
                    },
                    Review {
                        author: "kdev".into(),
                        state: ReviewState::Approved,
                    },
                ];
            }
            it.detail = Detail::Pr(Box::new(d));
            it
        })
        .collect()
}

/// Workflow runs for the repository at `repo`.
pub fn runs(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    let rows: &[(i64, &str, Status, &str, &str)] = &[
        (
            1841,
            "CI · fix/sidebar-clamp",
            Status::Failure,
            "pull_request",
            "1m 48s",
        ),
        (1840, "CI · main", Status::Success, "push", "2m 05s"),
        (
            1839,
            "release · v0.4.1",
            Status::Success,
            "workflow_dispatch",
            "4m 12s",
        ),
    ];
    rows.iter()
        .map(|(id, title, state, event, dur)| {
            let mut it = Item::run();
            it.num = id - r;
            it.id = id - r;
            it.title = (*title).into();
            it.state = *state;
            it.author = "marasanz".into();
            it.when = "12m ago".into();
            it.detail = Detail::Run(RunDetail {
                event: (*event).into(),
                workflow: "CI".into(),
                dur: (*dur).into(),
            });
            it
        })
        .collect()
}

/// The jobs of a run, each with its steps.
///
/// Every run gets the same two. What the tests ask of them is that they exist
/// and are named — a log excerpt says which job it came from, and the tree
/// pane needs something to expand.
pub fn jobs() -> Vec<Job> {
    fn step(name: &str, status: Status, dur: &str) -> Step {
        Step {
            name: name.into(),
            status,
            dur: dur.into(),
        }
    }
    vec![
        Job {
            name: "build".into(),
            status: Status::Success,
            dur: "48s".into(),
            steps: vec![
                step("checkout", Status::Success, "2s"),
                step("cargo build", Status::Success, "41s"),
            ],
        },
        Job {
            name: "test".into(),
            status: Status::Failure,
            dur: "1m 00s".into(),
            steps: vec![
                step("checkout", Status::Success, "2s"),
                step("cargo test", Status::Failure, "58s"),
            ],
        },
    ]
}

/// The log of a run, filed by job and step the way a real one arrives.
pub fn raw_log() -> Vec<RawLog> {
    fn line(job: &str, step: &str, n: usize, text: &str, kind: LogKind) -> RawLog {
        RawLog {
            job: job.into(),
            step: step.into(),
            time: format!("10:4{}:0{}", n % 10, n % 6),
            text: text.into(),
            kind,
        }
    }
    vec![
        line(
            "build",
            "checkout",
            1,
            "Syncing repository: marasanz/tuikit",
            LogKind::Plain,
        ),
        line(
            "build",
            "cargo build",
            2,
            "   Compiling tuikit v0.4.1",
            LogKind::Plain,
        ),
        line(
            "build",
            "cargo build",
            3,
            "    Finished `dev` profile in 41.02s",
            LogKind::Success,
        ),
        line(
            "test",
            "checkout",
            4,
            "Syncing repository: marasanz/tuikit",
            LogKind::Plain,
        ),
        line("test", "cargo test", 5, "running 118 tests", LogKind::Plain),
        line(
            "test",
            "cargo test",
            6,
            "test layout::clamps_the_sidebar ... FAILED",
            LogKind::Error,
        ),
        line(
            "test",
            "cargo test",
            7,
            "test result: FAILED. 117 passed; 1 failed",
            LogKind::Error,
        ),
    ]
}

/// One changed file, with a hunk so there is a diff to draw.
fn changed(path: &str, add: &str, del: &str) -> FileChange {
    const HUNK: StaticHunk = StaticHunk {
        hdr: "@@ -18,7 +18,9 @@ fn solve(area: Rect) -> Layout {",
        lines: &[
            (' ', "    let total = area.width;"),
            ('-', "    let side = total / 4;"),
            ('+', "    let side = (total / 4).max(MIN_SIDEBAR);"),
            ('+', "    debug_assert!(side <= total);"),
            (' ', "    let body = total - side;"),
        ],
    };
    FileChange {
        path: path.into(),
        add: add.into(),
        del: del.into(),
        hunks: vec![Hunk::from(&HUNK)],
    }
}

/// An `App` with the fixture already in it and no worker behind it.
///
/// `App::new` seeds nothing on purpose — it is what the binary calls, and the
/// binary has a thread to ask rather than data to invent. Filling one in is
/// this module's job, and doing it here rather than in the constructor is what
/// let the constructor stop asking where its data came from. No worker is the
/// other half of that: with none, `ensure` asks for nothing, so what is in the
/// app stays exactly what was put there.
pub fn app() -> App {
    let mut app = App::new(None);
    let accounts = accounts();
    for a in &accounts {
        for (r, repo) in a.repos.iter().enumerate() {
            let key = format!("{}/{}", a.login, repo.name);
            app.lists.insert((key.clone(), 0), issues(r));
            app.lists.insert((key.clone(), 1), prs(r));
            let runs = runs(r);
            for run in &runs {
                app.jobs_by_run.insert((key.clone(), run.id), jobs());
                app.jobs_state.insert((key.clone(), run.id), Load::Ready);
                app.raw_logs.insert((key.clone(), run.id), raw_log());
                app.logs_state.insert((key.clone(), run.id), Load::Ready);
            }
            app.lists.insert((key.clone(), 2), runs);
            for tab in 0..3 {
                app.lists_state.insert((key.clone(), tab), Load::Ready);
            }
        }
    }
    app.accounts = accounts;
    app.accounts_state = Load::Ready;
    // The third repository, which is the one with both issues and pull
    // requests in it — opening on an empty list tests nothing.
    app.repo = 2;
    app
}
