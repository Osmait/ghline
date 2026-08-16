//! github-tui: repositories, issues, pull requests and Actions, over `gh`.
//!
//! The same four layers diffline has, and the directories are them:
//!
//!   `data`, `subject`   what the things are, and what an agent is told
//!                       about one. Neither depends on anything here.
//!   `source`            where they come from — `gh`, the thread that asks
//!                       it, and the fixture that stands in when it is not
//!                       signed in.
//!   `state`             what is on screen, what a key or a click does to
//!                       it, and what is pending — a confirmation, a toast.
//!   `view`              drawing it, and nothing else.
//!
//! `cli` sits at the process boundary rather than in that stack: it turns raw
//! operating-system arguments into commands before any source or view exists.
//!
//! The arrows point one way: `view` reads `state`, `state` asks `source`,
//! everything knows `data`, and `data` knows none of them — pinned by a test
//! in that file since the first week.
//!
//! ## What a keystroke costs
//!
//! Nothing on the drawing thread waits for the network. Pressing `2` marks the
//! pull request list as wanted and returns; the frame drawn a moment later
//! shows a skeleton, and the rows replace it whenever they arrive.
//!
//! ```text
//!   drawing thread                        │  worker thread
//!   ──────────────────────────────────────┼────────────────────────────────
//!   on_key(2)      tab = pull requests    │
//!   ensure()       marks it Loading, ─────┼──► Request::List { repo, tab }
//!                  then asks              │       │
//!   draw()         skeleton, animating    │    `gh pr list --json …`
//!      ⋮           (frames keep going)    │       │
//!   drain()        ◄──────────────────────┼── Response::List { rows, … }
//!   apply()        rows into state        │
//!   draw()         the rows               │
//! ```
//!
//! `ensure` runs before every frame and is idempotent because of the state it
//! sets: each thing it can ask for is `Idle`, `Loading` or ready, and only
//! `Idle` is asked. That is what makes it safe to call on every pass, and it
//! is why no view has to remember to start its own load — a view that needs
//! something says so by existing.
//!
//! Two exceptions, named rather than quietly true. `source::service` imports
//! `data::Source` to know what a search is for; it is request vocabulary that
//! happens to live with the data. It used to collide with an `app::Source`
//! that said where the data came from — that one is gone with the demo mode,
//! and the name means one thing now. And `app`'s tests call `view::ui::draw`
//! — a test that renders a frame is crossing the layers on purpose, which is
//! what a test is for.

pub mod cli;
pub mod data;
pub mod subject;

pub mod source {
    // Deterministic data for the tests and the golden frames. Not behind a
    // feature: it used to be nine hundred lines of designed GitHub backing a
    // `--demo` mode, and hiding that from the released binary was worth a
    // flag. What is left is a page of scaffolding, which is cheaper to carry
    // than the `#[cfg]` branches were.
    pub mod fixture;
    pub mod forge;
    pub mod gh;
    pub mod service;
}

pub mod state {
    pub mod actions;
    pub mod app;
    pub mod finder;
}

pub mod view {
    // Drawing into an off-screen terminal: the fixture for the golden frames,
    // and `to_svg` and the live render for whatever is on screen, which is
    // how `--svg-live` replays a real session and how diffline draws a real
    // diff.
    pub mod snapshot;
    pub mod ui;
}

// Filed by layer, spoken about by name.
pub use source::{fixture, forge, gh, service};
pub use state::{actions, app, finder};
pub use view::{snapshot, ui};
