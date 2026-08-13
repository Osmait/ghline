//! The application error type.
//!
//! A purpose-built enum is preferred over `Box<dyn Error>` or `String` so that
//! callers can tell the cases apart: a missing `gh` is not fixed the same way
//! as a repository you cannot read. It implements `Display` and
//! `std::error::Error`, so it works with `?` and the usual tooling.

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    /// `gh` could not be launched: it is missing or not executable.
    Spawn(io::Error),
    /// `gh` exited with a non-zero status.
    Command {
        /// The subcommand that was run, so the failure can be placed.
        args: String,
        status: Option<i32>,
        stderr: String,
    },
    /// The output was not the JSON we expected.
    Json {
        args: String,
        source: serde_json::Error,
    },
    /// A required field is missing from an otherwise valid response.
    Field { args: String, field: &'static str },
}

impl Error {
    /// Short text for the status bar: one line, with no nested context.
    pub fn brief(&self) -> String {
        match self {
            Self::Spawn(e) if e.kind() == io::ErrorKind::NotFound => {
                "gh not found — install the GitHub CLI".to_string()
            }
            Self::Spawn(e) => format!("could not run gh: {e}"),
            Self::Command { stderr, status, .. } => {
                let first = stderr
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("");
                if first.is_empty() {
                    match status {
                        Some(c) => format!("gh exited with code {c}"),
                        None => "gh was killed by a signal".to_string(),
                    }
                } else {
                    first.to_string()
                }
            }
            Self::Json { .. } => "unexpected output from gh".to_string(),
            Self::Field { field, .. } => format!("missing field `{field}` in gh output"),
        }
    }

    /// Is retrying worthwhile? A network blip is; a missing `gh` is not.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Spawn(_) => false,
            Self::Command { stderr, .. } => {
                let s = stderr.to_lowercase();
                s.contains("timeout")
                    || s.contains("connection")
                    || s.contains("try again")
                    || s.contains("rate limit")
            }
            Self::Json { .. } | Self::Field { .. } => false,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not run gh: {e}"),
            Self::Command {
                args,
                status,
                stderr,
            } => {
                write!(f, "`gh {args}` failed")?;
                if let Some(c) = status {
                    write!(f, " with code {c}")?;
                }
                let first = stderr
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("");
                if !first.is_empty() {
                    write!(f, ": {first}")?;
                }
                Ok(())
            }
            Self::Json { args, source } => {
                write!(f, "unexpected JSON from `gh {args}`: {source}")
            }
            Self::Field { args, field } => {
                write!(f, "`gh {args}` returned no `{field}`")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(e) => Some(e),
            Self::Json { source, .. } => Some(source),
            Self::Command { .. } | Self::Field { .. } => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
