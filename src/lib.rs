//! Two terminal interfaces over one set of parts.
//!
//! `github-tui` browses GitHub through the `gh` CLI; `diffline` reviews the
//! diff in front of you and hands notes to a coding agent. One asks a server
//! what exists and the other asks the working tree what changed, so their
//! state has nothing in common — but what draws that state, and what they each
//! need on the way to drawing it, is the same thing twice.
//!
//! ```text
//!         github-tui                          diffline
//!         ──────────                          ────────
//!   view    draws, and nothing else     view    likewise
//!     │                                   │
//!   state   what a key or a click does  state   what a key or a click does
//!     │                                   │
//!   source  `gh`, on a worker thread    source  `git`, on a worker thread
//!     │                                   │
//!   data    what a repository, an       model   what a diff is, and where a
//!   subject issue, a run is                     comment sits in one
//!     │                                   │
//!   ══╪═══════════════════════════════════╪══════════════════════════════
//!     │                                   │
//!     └───────────────┬───────────────────┘
//!                     ▼
//!            tui ◄────────► shared     tui:    cells, panes, modals, and the
//!                └── one ──┘                   terminal and its loop
//!                   edge               shared: palette, fuzzy matcher, lexer,
//!                                              agents, config, worker threads
//! ```
//!
//! Within a program every arrow points down and none point back up: `view`
//! reads `state`, `state` asks `source`, `source` knows `data`, and `data`
//! knows none of them — pinned by a test in that file. The two columns never
//! touch: all they share is what is below the line.
//!
//! Below the line the two are not stacked, and the diagram says so because it
//! is the one place the rule does not hold. `tui::run` needs `shared::key` to
//! say what a keystroke is, and `shared::config` needs `tui::theme::Theme` to
//! say which theme was saved — a settings file storing a value that belongs to
//! the drawing toolkit. It is one edge in each direction and neither is load
//! bearing, so they are drawn side by side rather than as a layer that is not
//! one.
//!
//! Splitting it this way, rather than growing one binary two heads, is what
//! keeps each program's state about its own subject. Four directories, and
//! each one answers "whose is this?": `shared` belongs to neither program,
//! `tui` is the drawing toolkit both use, and the other two are one program
//! each — which matters because each program has a `service`, a `ui` and a
//! `hit`, and the path is what tells you which one you are reading.

pub mod shared;
pub mod tui;

pub mod diffline;
pub mod github;
