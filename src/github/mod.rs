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
//! The arrows point one way: `view` reads `state`, `state` asks `source`,
//! everything knows `data`, and `data` knows none of them — pinned by a test
//! in that file since the first week.
//!
//! Two exceptions, named rather than quietly true. `source::service` imports
//! `state::finder::Source` to know what a search is for; it is request
//! vocabulary that happens to live with the finder, and moving it collides
//! with `app::Source`, which means something else entirely. And `app`'s tests
//! call `view::ui::draw` — a test that renders a frame is crossing the layers
//! on purpose, which is what a test is for.

pub mod data;
pub mod subject;

pub mod source {
    pub mod demo;
    pub mod demo_diffs;
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
    pub mod snapshot;
    pub mod ui;
}

// Filed by layer, spoken about by name.
pub use source::{demo, demo_diffs, forge, gh, service};
pub use state::{actions, app, finder};
pub use view::{snapshot, ui};
