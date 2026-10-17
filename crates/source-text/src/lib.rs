//! Source text prepared for a terminal without application policy.
//!
//! `syntax` identifies lightweight colour spans, `text` makes command output
//! safe to measure and draw, and `words` finds what changed between two
//! lines. All three operate on borrowed strings and know nothing about
//! repositories, diffs or terminal widgets.

pub mod syntax;
pub mod text;
pub mod words;
