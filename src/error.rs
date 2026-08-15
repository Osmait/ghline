//! The application error type.
//!
//! A purpose-built enum is preferred over `Box<dyn Error>` or `String` so that
//! callers can tell the cases apart: a missing program is not fixed the same
//! way as a repository you cannot read. It implements `Display` and
//! `std::error::Error`, so it works with `?` and the usual tooling.
//!
//! Three programs are driven through this: `gh`, `git` and `herdr`. Every
//! case therefore names which one it was — a type shared by three callers
//! that assumes one of them will tell somebody to install the wrong thing.

use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    /// The program could not be launched: it is missing or not executable.
    Spawn {
        /// `gh`, `git`, `herdr` — whichever one this was.
        program: &'static str,
        source: io::Error,
    },
    /// The program exited with a non-zero status.
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
            Self::Spawn { program, source } if source.kind() == io::ErrorKind::NotFound => {
                match *program {
                    "gh" => "gh not found — install the GitHub CLI".to_string(),
                    p => format!("{p} not found — is it installed?"),
                }
            }
            Self::Spawn { program, source } => format!("could not run {program}: {source}"),
            Self::Command {
                args,
                stderr,
                status,
            } => {
                let first = stderr
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("");
                if first.is_empty() {
                    let prog = args.split_whitespace().next().unwrap_or("the command");
                    match status {
                        Some(c) => format!("{prog} exited with code {c}"),
                        None => format!("{prog} was killed by a signal"),
                    }
                } else {
                    first.to_string()
                }
            }
            Self::Json { args, .. } => {
                let prog = args.split_whitespace().next().unwrap_or("the command");
                format!("unexpected output from {prog}")
            }
            Self::Field { args, field } => {
                let prog = args.split_whitespace().next().unwrap_or("the command");
                format!("missing field `{field}` in {prog} output")
            }
        }
    }

    /// Is retrying worthwhile? A network blip is; a missing `gh` is not.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Spawn { .. } => false,
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
            Self::Spawn { program, source } => write!(f, "could not run {program}: {source}"),
            Self::Command {
                args,
                status,
                stderr,
            } => {
                write!(f, "`{args}` failed")?;
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
                write!(f, "unexpected JSON from `{args}`: {source}")
            }
            Self::Field { args, field } => {
                write!(f, "`{args}` returned no `{field}`")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Command { .. } | Self::Field { .. } => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_names_the_one_that_is_missing() {
        // Three programs share this type. It used to tell everybody to
        // install the GitHub CLI, including when what was missing was git.
        let gone = |p| Error::Spawn {
            program: p,
            source: io::Error::from(io::ErrorKind::NotFound),
        };
        assert_eq!(gone("gh").brief(), "gh not found — install the GitHub CLI");
        assert_eq!(gone("git").brief(), "git not found — is it installed?");
        assert_eq!(gone("herdr").brief(), "herdr not found — is it installed?");
    }

    #[test]
    fn a_failed_command_is_named_by_the_program_that_ran_it() {
        let e = Error::Command {
            args: "git diff".into(),
            status: Some(128),
            stderr: String::new(),
        };
        assert_eq!(e.brief(), "git exited with code 128");
        assert_eq!(e.to_string(), "`git diff` failed with code 128");
    }

    #[test]
    fn stderr_speaks_for_itself_when_it_has_something_to_say() {
        let e = Error::Command {
            args: "git diff".into(),
            status: Some(128),
            stderr: "\nfatal: not a git repository\n".into(),
        };
        assert_eq!(e.brief(), "fatal: not a git repository");
    }

    #[test]
    fn the_cause_is_reachable_rather_than_flattened_away() {
        use std::error::Error as _;
        let e = Error::Spawn {
            program: "git",
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert!(e.source().is_some(), "the chain has to survive");
    }
}
