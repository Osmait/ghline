//! Two terminal interfaces over one set of parts.
//!
//! `github-tui` browses GitHub through the `gh` CLI; `diffline` reviews the
//! diff in front of you and hands notes to a coding agent. They are different
//! programs — one asks a server what exists, the other asks the working tree
//! what changed — but most of what they are made of is the same: a palette, a
//! fuzzy matcher, a lexer, the agents on this machine, and the small drawing
//! primitives a cell grid needs.
//!
//! Splitting them this way rather than growing one binary two heads keeps each
//! one's state about its own subject. The shared modules below are the seam.

// --- shared by both ---
pub mod clones;
pub mod config;
pub mod error;
pub mod fuzzy;
pub mod herdr;
pub mod icons;
pub mod syntax;
pub mod theme;

// --- diffline ---
pub mod diffline;

// --- github-tui ---
pub mod actions;
pub mod app;
pub mod data;
pub mod demo;
pub mod demo_diffs;
pub mod finder;
pub mod gh;
pub mod service;
pub mod snapshot;
pub mod subject;
pub mod ui;
