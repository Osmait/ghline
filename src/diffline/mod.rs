//! Diffline: review the diff in front of you, and hand notes to an agent.
//!
//! A different program from the GitHub browser next door, and deliberately so.
//! That one asks a server what exists; this one asks the working tree what
//! changed, anchors comments to the lines it finds, and sends the lot to a
//! coding agent. What they share — the palette, the fuzzy matcher, the lexer,
//! the agents on this machine — they share through the library rather than by
//! being the same binary.

pub mod app;
pub mod git;
pub mod input;
pub mod keys;
pub mod model;
pub mod service;
pub mod ui;
pub mod vcs;
