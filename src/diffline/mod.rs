//! diffline — review the diff in front of you, and hand notes to an agent.
//!
//! Four layers, and the directories are them:
//!
//!   `model`   what a diff is, and where a comment sits in one. Depends on
//!             nothing, which is what makes it worth having separately.
//!   `source`  where the diff comes from — a `Vcs`, the process that answers
//!             it, and the thread that keeps both off the render loop.
//!   `state`   what is on screen and what a key or a click does to it.
//!   `view`    drawing it, and nothing else: no file is opened and no process
//!             is started from in there.
//!
//! The arrows point one way: `view` reads `state`, `state` asks `source`,
//! everything knows `model`, and `model` knows none of them.
//!
//! One exception, named rather than quietly true: `source::service` calls
//! `state::keys::write_template`. Writing a file is infrastructure and
//! belongs here, but the file's shape is the keymap's, so the writer lives
//! with what it writes.

pub mod model;

pub mod source {
    pub mod git;
    pub mod service;
    pub mod vcs;
}

pub mod state {
    pub mod app;
    pub mod hit;
    pub mod input;
    pub mod keys;
    pub mod mouse;
}

pub mod view {
    pub mod ui;
}

// The layers are how it is filed, not how it is spoken about: `diffline::app`
// reads better at a call site than `diffline::state::app`, and the layer is
// still there in the path on disk for anyone who wants to know.
pub use source::{git, service, vcs};
pub use state::{app, hit, input, keys};
pub use view::ui;
