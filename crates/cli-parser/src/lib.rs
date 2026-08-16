//! Small command-line parsing primitives with no application policy.
//!
//! The parser separates option syntax from an application's command model.
//! It recognises long options, short-option groups, positional values and the
//! `--` separator; callers decide which names exist and what each value means.

mod arg;
mod cli;
mod error;
mod help;
mod matches;
mod parser;
mod spec;

pub use arg::Arg;
pub use cli::Cli;
pub use error::Error;
pub use matches::{Matches, Outcome};
pub use parser::{Parser, value_as, value_text};
pub use spec::{ArgSpec, ValueSpec};
