//! The design's demo data set: accounts, repos, issues, PRs, runs, jobs and
//! logs. It is a fixture, kept apart from the model it fills in.

use crate::data::{
    Account, Comment, DemoLine, Detail, FileChange, Hunk, IssueDetail, Item, Job, Label, PrDetail,
    Repo, Review, ReviewState, RunDetail, Status, Step,
};
use crate::demo_diffs;

fn ago(n: u32, u: &str) -> String {
    format!("{n}{u} ago")
}

pub fn accounts() -> Vec<Account> {
    vec![
        Account {
            login: "marasanz".into(),
            kind: "(personal)".into(),
            sub: "github.com · token: gho_****4f2a".into(),
            repos: vec![
                Repo {
                    name: "ripgrep-ui".into(),
                    private: false,
                    lang: "Rust".into(),
                    issues: 7,
                    prs: 2,
                    star: "1.2k".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "dotfiles".into(),
                    private: true,
                    lang: "Shell".into(),
                    issues: 1,
                    prs: 0,
                    star: "41".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "tuikit".into(),
                    private: false,
                    lang: "Go".into(),
                    issues: 12,
                    prs: 4,
                    star: "832".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "notes.md".into(),
                    private: true,
                    lang: "Python".into(),
                    issues: 0,
                    prs: 1,
                    star: "3".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "aoc-2025".into(),
                    private: false,
                    lang: "Rust".into(),
                    issues: 2,
                    prs: 0,
                    star: "18".into(),
                    has_workflows: true,
                },
            ],
        },
        Account {
            login: "acme-platform".into(),
            kind: "(org · ghe)".into(),
            sub: "ghe.acme.dev · SSO active".into(),
            repos: vec![
                Repo {
                    name: "edge-router".into(),
                    private: true,
                    lang: "Go".into(),
                    issues: 34,
                    prs: 11,
                    star: "—".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "billing-core".into(),
                    private: true,
                    lang: "Elixir".into(),
                    issues: 21,
                    prs: 6,
                    star: "—".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "web-console".into(),
                    private: true,
                    lang: "TypeScript".into(),
                    issues: 58,
                    prs: 19,
                    star: "—".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "infra-terraform".into(),
                    private: true,
                    lang: "Shell".into(),
                    issues: 9,
                    prs: 3,
                    star: "—".into(),
                    has_workflows: true,
                },
            ],
        },
        Account {
            login: "oss-tuiclub".into(),
            kind: "(org)".into(),
            sub: "github.com · member".into(),
            repos: vec![
                Repo {
                    name: "gum".into(),
                    private: false,
                    lang: "Go".into(),
                    issues: 15,
                    prs: 5,
                    star: "19k".into(),
                    has_workflows: true,
                },
                Repo {
                    name: "ansi-parser".into(),
                    private: false,
                    lang: "Rust".into(),
                    issues: 4,
                    prs: 1,
                    star: "2.4k".into(),
                    has_workflows: true,
                },
            ],
        },
    ]
}

pub fn issues(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    type IssueRow = (
        i64,
        &'static str,
        &'static str,
        &'static str,
        (u32, &'static str),
        u32,
        Vec<Label>,
    );
    let base: Vec<IssueRow> = vec![
        (
            412,
            "Panic on resize when sidebar width < 12 cols",
            "open",
            "lmoreno",
            (2, "h"),
            6,
            vec![
                Label::new("bug", (0xf0, 0x71, 0x78)),
                Label::new("tui", (0x39, 0xba, 0xe6)),
            ],
        ),
        (
            408,
            "Add fuzzy filter to the repository pane",
            "open",
            "kdev",
            (9, "h"),
            2,
            vec![Label::new("feature", (0x7f, 0xd9, 0x62))],
        ),
        (
            401,
            "Truecolor fallback is wrong on Apple Terminal",
            "open",
            "sofi",
            (1, "d"),
            11,
            vec![
                Label::new("bug", (0xf0, 0x71, 0x78)),
                Label::new("good first issue", (0xd2, 0xa6, 0xff)),
            ],
        ),
        (
            397,
            "Document the :account command in the README",
            "open",
            "marasanz",
            (2, "d"),
            1,
            vec![Label::new("docs", (0xff, 0xb4, 0x54))],
        ),
        (
            388,
            "Cache the GraphQL check-run query per PR",
            "open",
            "tsuki",
            (3, "d"),
            4,
            vec![Label::new("perf", (0xff, 0x8f, 0x40))],
        ),
        (
            371,
            "Vim-style counts (3j) are ignored in lists",
            "closed",
            "kdev",
            (6, "d"),
            8,
            vec![Label::new("bug", (0xf0, 0x71, 0x78))],
        ),
        (
            355,
            "Support GHE hosts alongside github.com",
            "closed",
            "lmoreno",
            (2, "w"),
            17,
            vec![Label::new("feature", (0x7f, 0xd9, 0x62))],
        ),
    ];

    let body = format!(
        "Steps to reproduce\n  1. open {repo} with a narrow terminal\n  2. press l to focus the content pane\n  3. shrink the window below 90 columns\n\nExpected: the sidebar collapses.\nActual: the render loop throws and the alternate screen is left dirty."
    );

    base.into_iter()
        .map(|(num, title, state, author, (n, u), comments, labels)| {
            let mut it = Item::issue();
            it.num = num - r * 13;
            it.title = title.to_string();
            it.state = Status::parse(state);
            it.author = author.into();
            it.when = ago(n, u);
            it.labels = labels;
            it.body = body.clone();
            let detail = IssueDetail {
                comments,
                comment_list: vec![
                    Comment {
                        author: "lmoreno".into(),
                        when: ago(2, "h"),
                        body: "Reproduced on kitty 0.35 and on wezterm. Only when the sidebar is focused.".into(),
                    },
                    Comment {
                        author: "marasanz".into(),
                        when: ago(1, "h"),
                        body: "Likely the layout solver clamping to a negative width. I will guard it in the reducer.".into(),
                    },
                ],
            };
            it.detail = Detail::Issue(detail);
            it
        })
        .collect()
}

pub fn prs(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    let body = "The layout solver could compute a negative width when the terminal was\nresized while the sidebar had focus, which panicked the render loop and\nleft the alternate screen dirty.\n\n- clamp the sidebar to MIN_SIDEBAR (12 cols)\n- collapse it entirely below 90 columns\n- regression test in layout::solver\n\nCloses #412";

    /// A changed file, carrying whatever hunks the design defines for it.
    fn fc(path: &str, add: &str, del: &str) -> FileChange {
        FileChange {
            path: path.into(),
            add: add.into(),
            del: del.into(),
            hunks: demo_diffs::demo_diff(path).iter().map(Hunk::from).collect(),
        }
    }

    let mut out = Vec::new();

    let mut p = Item::pr();
    let mut d = PrDetail::default();
    p.num = 219 - r;
    p.body = body.to_string();
    d.file_list = vec![
        fc("src/layout/solver.rs", "+64", "-18"),
        fc("src/layout/mod.rs", "+12", "-4"),
        fc("src/app/reducer.rs", "+27", "-9"),
        fc("src/ui/sidebar.rs", "+18", "-3"),
        fc("tests/layout.rs", "+7", "-0"),
        fc("CHANGELOG.md", "+0", "-0"),
    ];
    d.files = 6;

    p.title = "fix(layout): clamp sidebar width to a minimum of 12 cols".into();
    p.state = Status::Open;
    p.author = "marasanz".into();
    p.when = ago(26, "m");
    d.checks = Status::Failure;
    d.add = "+128".into();
    d.del = "-34".into();
    d.branch = "fix/sidebar-clamp".into();
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
    p.labels = vec![Label::new("bug", (0xf0, 0x71, 0x78))];
    p.detail = Detail::Pr(Box::new(d));
    out.push(p);

    let mut p = Item::pr();
    let mut d = PrDetail::default();
    p.num = 216 - r;
    p.body = "Streams job logs over the Actions API instead of re-fetching the whole\nblob on every tick.\n\n- LogStream with a bounded channel and keepalives\n- follow mode (f) pinned to the bottom of the pane\n- highlight ##[error] lines and index them for e\n\nRefs #388".into();
    d.file_list = vec![
        fc("src/actions/stream.rs", "+188", "-0"),
        fc("src/actions/mod.rs", "+9", "-2"),
        fc("src/ui/logs.rs", "+141", "-7"),
        fc("CHANGELOG.md", "+4", "-0"),
    ];
    d.files = 4;
    d.file_list = vec![
        fc("src/actions/stream.rs", "+188", "-0"),
        fc("src/actions/mod.rs", "+9", "-2"),
        fc("src/ui/logs.rs", "+141", "-7"),
        fc("CHANGELOG.md", "+4", "-0"),
    ];
    d.files = 4;

    p.title = "feat(actions): stream job logs with follow mode".into();
    p.state = Status::Open;
    p.author = "kdev".into();
    p.when = ago(5, "h");
    d.checks = Status::Running;
    d.add = "+342".into();
    d.del = "-9".into();
    d.branch = "feat/log-stream".into();
    d.reviews = vec![Review {
        author: "marasanz".into(),
        state: ReviewState::Commented,
    }];
    p.labels = vec![Label::new("feature", (0x7f, 0xd9, 0x62))];
    p.detail = Detail::Pr(Box::new(d));
    out.push(p);

    let mut p = Item::pr();
    let mut d = PrDetail::default();
    p.num = 211 - r;
    p.body = "Bumps crossterm from 0.27.0 to 0.28.1.\n\nRelease notes and changelog omitted — see the upstream repository.".into();
    d.file_list = vec![fc("Cargo.toml", "+1", "-1"), fc("Cargo.lock", "+8", "-8")];
    d.files = 2;
    d.file_list = vec![fc("Cargo.toml", "+1", "-1"), fc("Cargo.lock", "+8", "-8")];
    d.files = 2;

    p.title = "chore(deps): bump crossterm to 0.28".into();
    p.state = Status::Open;
    p.author = "dependabot".into();
    p.when = ago(1, "d");
    d.checks = Status::Success;
    d.add = "+9".into();
    d.del = "-9".into();
    d.branch = "dependabot/crossterm".into();
    d.reviews = vec![Review {
        author: "kdev".into(),
        state: ReviewState::Approved,
    }];
    p.labels = vec![Label::new("deps", (0xd2, 0xa6, 0xff))];
    p.detail = Detail::Pr(Box::new(d));
    out.push(p);

    let mut p = Item::pr();
    let mut d = PrDetail::default();
    p.num = 205 - r;
    p.body = "Reads every host from hosts.yml (github.com and GHE) and lets you switch\nwith a modal picker bound to a.\n\nStill draft: the SSO re-auth flow is stubbed.".into();
    d.file_list = vec![
        fc("src/auth/hosts.rs", "+164", "-0"),
        fc("src/auth/switcher.rs", "+97", "-0"),
        fc("src/ui/account_modal.rs", "+212", "-14"),
        fc("src/app/reducer.rs", "+63", "-42"),
        fc("tests/auth.rs", "+75", "-0"),
        fc("CHANGELOG.md", "+0", "-0"),
    ];
    d.files = 6;
    d.file_list = vec![
        fc("src/auth/hosts.rs", "+164", "-0"),
        fc("src/auth/switcher.rs", "+97", "-0"),
        fc("src/ui/account_modal.rs", "+212", "-14"),
        fc("src/app/reducer.rs", "+63", "-42"),
        fc("tests/auth.rs", "+75", "-0"),
        fc("CHANGELOG.md", "+0", "-0"),
    ];
    d.files = 6;

    p.title = "feat(auth): multi-account switcher with GHE hosts".into();
    p.state = Status::Draft;
    p.author = "sofi".into();
    p.when = ago(2, "d");
    d.checks = Status::Pending;
    d.add = "+611".into();
    d.del = "-56".into();
    d.branch = "feat/multi-account".into();
    p.labels = vec![
        Label::new("feature", (0x7f, 0xd9, 0x62)),
        Label::new("auth", (0x39, 0xba, 0xe6)),
    ];
    p.detail = Detail::Pr(Box::new(d));
    out.push(p);

    let mut p = Item::pr();
    let mut d = PrDetail::default();
    p.num = 198 - r;
    p.body = "Pure move plus a parser for vim counts (3j). No behaviour change beyond\ncounts now being honoured in every list.\n\nCloses #371".into();
    d.file_list = vec![
        fc("src/keymap/lib.rs", "+318", "-0"),
        fc("src/app/keys.rs", "+22", "-641"),
        fc("Cargo.toml", "+6", "-1"),
        fc("Cargo.lock", "+22", "-6"),
        fc("CHANGELOG.md", "+2", "-0"),
    ];
    d.files = 5;
    d.file_list = vec![
        fc("src/keymap/lib.rs", "+318", "-0"),
        fc("src/app/keys.rs", "+22", "-641"),
        fc("Cargo.toml", "+6", "-1"),
        fc("Cargo.lock", "+22", "-6"),
        fc("CHANGELOG.md", "+2", "-0"),
    ];
    d.files = 5;

    p.title = "refactor: move keymap into its own crate".into();
    p.state = Status::Merged;
    p.author = "tsuki".into();
    p.when = ago(4, "d");
    d.checks = Status::Success;
    d.add = "+370".into();
    d.del = "-648".into();
    d.branch = "refactor/keymap-crate".into();
    d.reviews = vec![
        Review {
            author: "marasanz".into(),
            state: ReviewState::Approved,
        },
        Review {
            author: "kdev".into(),
            state: ReviewState::Approved,
        },
    ];
    p.detail = Detail::Pr(Box::new(d));
    out.push(p);

    out
}

pub fn runs(repo: usize) -> Vec<Item> {
    let r = repo as i64;
    type RunRow = (
        i64,
        &'static str,
        &'static str,
        &'static str,
        (u32, &'static str),
        &'static str,
        &'static str,
    );
    let base: [RunRow; 5] = [
        (
            1841,
            "CI · fix/sidebar-clamp",
            "failure",
            "marasanz",
            (24, "m"),
            "pull_request",
            "3m 02s",
        ),
        (
            1840,
            "Release · build-release",
            "running",
            "kdev",
            (4, "m"),
            "workflow_dispatch",
            "1m 06s",
        ),
        (
            1839,
            "CI · main",
            "success",
            "tsuki",
            (3, "h"),
            "push",
            "2m 51s",
        ),
        (
            1838,
            "Nightly · tui snapshots",
            "success",
            "github-actions",
            (11, "h"),
            "schedule",
            "6m 12s",
        ),
        (
            1837,
            "CI · feat/log-stream",
            "cancelled",
            "kdev",
            (1, "d"),
            "pull_request",
            "48s",
        ),
    ];

    base.into_iter()
        .map(|(num, title, state, author, (n, u), event, dur)| {
            let mut it = Item::run();
            it.num = num - r * 7;
            it.title = title.to_string();
            it.state = Status::parse(state);
            it.author = author.into();
            it.when = ago(n, u);
            it.detail = Detail::Run(RunDetail {
                event: event.into(),
                workflow: String::new(),
                dur: dur.into(),
            });
            it
        })
        .collect()
}

fn step(name: &str, status: Status, dur: &str) -> Step {
    Step {
        name: name.into(),
        status,
        dur: dur.into(),
    }
}

pub fn job_templates() -> Vec<Job> {
    vec![
        Job {
            name: "lint".into(),
            status: Status::Success,
            dur: "38s".into(),
            steps: vec![
                step("Set up job", Status::Success, "2s"),
                step("Checkout", Status::Success, "3s"),
                step("Setup toolchain", Status::Success, "11s"),
                step("cargo fmt --check", Status::Success, "6s"),
                step("clippy -D warnings", Status::Success, "14s"),
                step("Post job cleanup", Status::Success, "2s"),
            ],
        },
        Job {
            name: "test (ubuntu-24.04)".into(),
            status: Status::Success,
            dur: "2m 14s".into(),
            steps: vec![
                step("Set up job", Status::Success, "2s"),
                step("Checkout", Status::Success, "4s"),
                step("Cache deps", Status::Success, "9s"),
                step("cargo test --all", Status::Success, "1m 51s"),
                step("Upload coverage", Status::Success, "8s"),
            ],
        },
        Job {
            name: "test (macos-15)".into(),
            status: Status::Failure,
            dur: "3m 02s".into(),
            steps: vec![
                step("Set up job", Status::Success, "3s"),
                step("Checkout", Status::Success, "5s"),
                step("Cache deps", Status::Success, "12s"),
                step("cargo test --all", Status::Failure, "2m 34s"),
                step("Upload artifacts", Status::Skipped, "—"),
            ],
        },
        Job {
            name: "build-release".into(),
            status: Status::Running,
            dur: "1m 06s".into(),
            steps: vec![
                step("Set up job", Status::Success, "2s"),
                step("Checkout", Status::Success, "4s"),
                step("cross build aarch64", Status::Running, "1m 00s"),
                step("Sign binaries", Status::Pending, "—"),
                step("Publish draft", Status::Pending, "—"),
            ],
        },
        Job {
            name: "e2e-tui".into(),
            status: Status::Pending,
            dur: "—".into(),
            steps: vec![
                step("Set up job", Status::Pending, "—"),
                step("Run vhs tapes", Status::Pending, "—"),
            ],
        },
    ]
}

pub fn logs_for(status: Status) -> &'static [DemoLine] {
    match status {
        Status::Success => &[
            ("##[group]Run cargo test --all", "group"),
            ("  cargo test --all --locked", "dim"),
            (
                "   Compiling tuikit v0.9.3 (/home/runner/work/tuikit)",
                "fg",
            ),
            (
                "    Finished test profile [unoptimized + debuginfo] in 41.22s",
                "fg",
            ),
            ("     Running unittests src/lib.rs", "fg"),
            ("test layout::solver::clamps_negative_width ... ok", "green"),
            ("test render::diff::skips_unchanged_cells ... ok", "green"),
            ("test keymap::vim::parses_counts ... ok", "green"),
            ("test result: ok. 148 passed; 0 failed; 2 ignored", "green"),
            ("##[endgroup]", "group"),
        ],
        Status::Failure => &[
            ("##[group]Run cargo test --all", "group"),
            ("  cargo test --all --locked", "dim"),
            (
                "   Compiling tuikit v0.9.3 (/Users/runner/work/tuikit)",
                "fg",
            ),
            ("    Finished test profile in 1m 12s", "fg"),
            ("     Running unittests src/lib.rs", "fg"),
            ("test keymap::vim::parses_counts ... ok", "green"),
            ("test render::diff::skips_unchanged_cells ... ok", "green"),
            (
                "test layout::solver::clamps_negative_width ... FAILED",
                "red",
            ),
            ("", "fg"),
            ("failures:", "red"),
            (
                "---- layout::solver::clamps_negative_width stdout ----",
                "dim",
            ),
            (
                "thread 'main' panicked at src/layout/solver.rs:212:9:",
                "red",
            ),
            (
                "assertion `left == right` failed: sidebar width must never go below 12",
                "red",
            ),
            ("  left: -4", "red"),
            (" right: 12", "red"),
            (
                "note: run with `RUST_BACKTRACE=1` to see a backtrace",
                "dim",
            ),
            ("##[error]Process completed with exit code 101.", "red"),
            ("##[endgroup]", "group"),
        ],
        Status::Running => &[
            (
                "##[group]Run cross build --target aarch64-apple-darwin",
                "group",
            ),
            (
                "  cross build --release --target aarch64-apple-darwin",
                "dim",
            ),
            ("   Compiling libc v0.2.161", "fg"),
            ("   Compiling crossterm v0.28.1", "fg"),
            ("   Compiling unicode-width v0.2.0", "fg"),
            ("   Compiling tuikit v0.9.3", "fg"),
            ("warning: unused variable: `cols`", "yellow"),
            ("  --> src/layout/solver.rs:198:13", "dim"),
        ],
        Status::Skipped => &[(
            "This step was skipped because a previous step failed.",
            "dim",
        )],
        _ => &[("Waiting for a runner to pick up this job…", "dim")],
    }
}

pub const STREAM: &[DemoLine] = &[
    ("   Compiling ratatui v0.29.0", "fg"),
    ("   Compiling signal-hook v0.3.17", "fg"),
    ("   Compiling tui-textarea v0.7.0", "fg"),
    ("warning: field `last_tick` is never read", "yellow"),
    ("   Compiling tuikit-macros v0.9.3", "fg"),
    ("    Finished release profile in 2m 08s", "green"),
    ("##[group]Run codesign --deep --force", "group"),
    (
        "  codesign --sign \"Developer ID Application\" target/release/gh-tui",
        "dim",
    ),
    (
        "target/release/gh-tui: signed Mach-O universal binary",
        "green",
    ),
];

/// The design's `stepLog(name)`: per-step-name canned logs.
pub fn step_log(name: &str) -> Option<Vec<(String, &'static str)>> {
    let n = name.to_lowercase();
    let owned = |lines: &[DemoLine]| -> Option<Vec<(String, &'static str)>> {
        Some(lines.iter().map(|(t, k)| (t.to_string(), *k)).collect())
    };

    if n.starts_with("set up job") {
        let mut v: Vec<(String, &'static str)> = vec![
            ("Current runner version: '2.320.0'".into(), "dim"),
            ("##[group]Operating System".into(), "group"),
            ("  Ubuntu 24.04.1 LTS".into(), "fg"),
            ("##[endgroup]".into(), "group"),
            ("##[group]Runner Image".into(), "group"),
            ("  Image: ubuntu-24.04  Version: 20250803.1".into(), "fg"),
            ("##[endgroup]".into(), "group"),
            ("Prepare workflow directory".into(), "fg"),
            (
                "Download action repository 'actions/checkout@v4'".into(),
                "fg",
            ),
        ];
        v.push((format!("Complete job name: {name}"), "green"));
        return Some(v);
    }
    if n.starts_with("checkout") {
        return owned(&[
            ("##[group]Run actions/checkout@v4", "group"),
            ("  with:", "dim"),
            ("    fetch-depth: 0", "dim"),
            ("    persist-credentials: true", "dim"),
            ("##[endgroup]", "group"),
            ("Syncing repository: marasanz/tuikit", "fg"),
            ("/usr/bin/git init /home/runner/work/tuikit/tuikit", "dim"),
            (
                "/usr/bin/git fetch --prune --no-recurse-submodules origin",
                "dim",
            ),
            (
                "HEAD is now at 8e1c04b fix(layout): clamp sidebar width",
                "green",
            ),
        ]);
    }
    if n.starts_with("setup toolchain") {
        return owned(&[
            ("##[group]Run dtolnay/rust-toolchain@stable", "group"),
            ("  toolchain: 1.83.0  components: rustfmt, clippy", "dim"),
            ("##[endgroup]", "group"),
            ("info: downloading component 'clippy'", "fg"),
            (
                "info: default toolchain set to '1.83.0-x86_64-unknown-linux-gnu'",
                "fg",
            ),
            ("rustc 1.83.0 (90b35a623 2025-11-26)", "green"),
        ]);
    }
    if n.starts_with("cache") {
        return owned(&[
            ("##[group]Run actions/cache@v4", "group"),
            ("  path: ~/.cargo/registry  target/", "dim"),
            ("  key: cargo-linux-8e1c04b", "dim"),
            ("##[endgroup]", "group"),
            (
                "Received 214893120 of 214893120 (100.0%), 96.4 MBs/sec",
                "fg",
            ),
            ("Cache restored from key: cargo-linux-2f9ab01", "green"),
        ]);
    }
    if n.starts_with("cargo fmt") {
        return owned(&[
            ("##[group]Run cargo fmt --all --check", "group"),
            ("  cargo fmt --all -- --check", "dim"),
            ("##[endgroup]", "group"),
            ("no formatting differences found in 84 files", "green"),
        ]);
    }
    if n.starts_with("clippy") {
        return owned(&[
            (
                "##[group]Run cargo clippy --all-targets -- -D warnings",
                "group",
            ),
            (
                "    Checking tuikit v0.9.3 (/home/runner/work/tuikit)",
                "fg",
            ),
            ("##[endgroup]", "group"),
            (
                "warning: this `if` has identical blocks (allowed via #[allow])",
                "yellow",
            ),
            ("    Finished dev profile in 13.9s", "fg"),
            ("clippy: 0 errors, 1 allowed warning", "green"),
        ]);
    }
    if n.starts_with("upload coverage") || n.starts_with("upload artifacts") {
        return owned(&[
            ("##[group]Run actions/upload-artifact@v4", "group"),
            ("  name: coverage  path: lcov.info", "dim"),
            ("##[endgroup]", "group"),
            ("Uploaded 1 file (312 KB) in 3.1s", "fg"),
            (
                "Artifact coverage has been successfully uploaded, id: 3841192",
                "green",
            ),
        ]);
    }
    if n.starts_with("post job cleanup") {
        return owned(&[
            ("Post job cleanup.", "dim"),
            (
                "/usr/bin/git config --local --unset-all http.extraheader",
                "dim",
            ),
            ("Cleaning up orphan processes", "green"),
        ]);
    }
    if n.starts_with("sign binaries") {
        return owned(&[(
            "Waiting for the build step to finish before signing…",
            "dim",
        )]);
    }
    if n.starts_with("publish draft") {
        return owned(&[("Waiting for a runner to pick up this step…", "dim")]);
    }
    if n.starts_with("run vhs tapes") {
        return owned(&[("Waiting for the e2e-tui job to be scheduled…", "dim")]);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_repos_and_lists_line_up() {
        let accounts = accounts();
        assert_eq!(accounts.len(), 3);
        for (a, account) in accounts.iter().enumerate() {
            assert!(!account.repos.is_empty(), "account {a} has no repos");
            for r in 0..account.repos.len() {
                // every repo can build its three lists without panicking
                assert!(!issues(r).is_empty());
                assert!(!prs(r).is_empty());
                assert!(!runs(r).is_empty());
            }
        }
    }

    #[test]
    fn demo_pr_file_counts_match_their_lists() {
        for p in prs(0) {
            let pr = p.as_pr().expect("prs() only builds pull requests");
            assert_eq!(
                pr.files as usize,
                pr.file_list.len(),
                "PR #{} says {} files but lists {}",
                p.num,
                pr.files,
                pr.file_list.len()
            );
        }
    }
}
