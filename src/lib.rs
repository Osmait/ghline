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

// The three directories are the three answers to "whose is this?": `shared`
// belongs to neither program, `tui` is the drawing toolkit both use, and the
// other two are one program each. It used to be twenty-four modules in a row
// with nothing to say which was which — two of them called `service`, two
// called `ui`, two called `hit`, told apart only by a path that named no
// program.

pub mod shared;
pub mod tui;

pub mod diffline;
pub mod github;
