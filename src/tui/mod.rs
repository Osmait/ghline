//! The parts a terminal interface is made of.
//!
//! Both programs draw a grid of cells with the same hands, and this is where
//! the hands live. It is here rather than in either program because it belongs
//! to neither: these lived in `ui`, which is github-tui's, and diffline
//! imported ten of them out of it — with the result that `centered`, `frame`
//! and `rule` were written a second time in diffline and drifted, one copy
//! picking up a `saturating_sub` that the other never got.
//!
//! ## Four levels, and what each may call
//!
//! ```text
//!   ┌─────────────────────────────────────────────────────────────────┐
//!   │ organism   a whole region: Dialog, Section, rows                 │
//!   │            owns a Rect, hands back geometry                      │
//!   └──────────────────────────────┬──────────────────────────────────┘
//!                                  │ may call anything below,
//!                                  ▼ never another organism
//!   ┌─────────────────────────────────────────────────────────────────┐
//!   │ molecule   one nameable piece: frame, rule, query_line, empty    │
//!   └──────────────────────────────┬──────────────────────────────────┘
//!                                  ▼
//!   ┌─────────────────────────────────┐  ┌────────────────────────────┐
//!   │ atom   writes cells: put, fill, │  │ geom   where things go:    │
//!   │        hline, skel_bar, scrim   │  │        centered, pct,      │
//!   │                                 │◄─┤        scroll_into_view    │
//!   └─────────────────────────────────┘  └────────────────────────────┘
//! ```
//!
//! The rule that makes any of this worth the filing: **every level takes data,
//! never an app**. There are two unrelated `App` types in this crate and no
//! component here has heard of either. That is the whole reason a piece can be
//! used by both programs, and it is also why each level can be tested with a
//! `Buffer`, a `Rect` and no fixture at all — which is what the count below
//! is made of.
//!
//! ## What is here
//!
//! | | |
//! |---|---|
//! | [`geom`] | `centered`, `centered_over`, `pct`, `scroll_into_view` |
//! | [`atom`] | `put`, `put_trunc`, `put_right`, `fill`, `clear`, `hline`, `vline`, `skel_bar`, `scrim`, `bold`, `wrap`, `truncate_pad` |
//! | [`molecule`] | `frame`, `rule`, `modal_head`, `query_line`, `empty` |
//! | [`organism`] | `Dialog`, `Section`, `rows` |
//! | [`diff`] | pairing old and new lines for a split view |
//! | [`hit`] | what a click landed on |
//! | [`theme`] | the palette |
//! | [`run`] | the terminal and its event loop |
//! | [`probe`] | reading a drawn frame back, for tests |
//!
//! Everything is re-exported here, so `tui::put` and `tui::atom::put` are the
//! same function. Call sites use the short path; the level is for whoever is
//! deciding where a new component belongs.

// A toolkit with two consumers, which is the whole reason this directory
// exists — and a toolkit is documented or it is guessed at. Scoped here
// rather than set crate-wide in `Cargo.toml` because the two programs above
// are not in the same position: their types are read by whoever is changing
// them, this one's are read by whoever is calling it. Every item under `tui`
// carries a `///` today; the lint is what keeps the next one from not.
#![warn(missing_docs)]

pub mod diff;
pub mod hit;
pub mod probe;
pub mod run;
pub mod theme;

pub mod atom;
pub mod geom;
pub mod molecule;
pub mod organism;

pub use atom::{
    Seg, bold, clear, fill, hline, put, put_right, put_trunc, scrim, skel_bar, truncate_pad, vline,
    wrap,
};
pub use geom::{centered, centered_over, pct, scroll_into_view};
pub use molecule::{
    Empty, Query, agent_status, empty, frame, matched, modal_head, query_line, rule,
};
pub use organism::{AgentRow, Body, Dialog, RowSlot, Section, agent_row, rows};
