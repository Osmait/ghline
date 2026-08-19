//! Source text prepared for a terminal without application policy.
//!
//! `syntax` identifies lightweight colour spans while `text` makes command
//! output safe to measure and draw. Both operate on borrowed strings and know
//! nothing about repositories, diffs or terminal widgets.

pub mod syntax;
pub mod text;
