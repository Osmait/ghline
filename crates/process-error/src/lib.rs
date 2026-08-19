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

/// A program we ran did not give us what we asked for.
///
/// Four cases because they are fixed four different ways, and the reader is
/// the one doing the fixing: a `Spawn` is an install, a `Command` is the
/// program's own complaint and belongs to them, and `Json` or `Field` mean
/// the output moved under us and belongs to this crate.
#[derive(Debug)]
pub enum Error {
    /// The program could not be launched: it is missing or not executable.
    Spawn {
        /// `gh`, `git`, `herdr` — whichever one this was.
        program: &'static str,
        /// Application-supplied advice for a missing executable.
        ///
        /// `None` gets the generic installation question; a caller that knows
        /// the canonical package can name it without putting that policy in
        /// this crate.
        install: Option<&'static str>,
        /// What the operating system said. `NotFound` is the case worth its
        /// own wording, since it is the only one the reader can act on.
        source: io::Error,
    },
    /// The program exited with a non-zero status.
    Command {
        /// The subcommand that was run, so the failure can be placed.
        args: String,
        /// The exit code, or `None` when a signal ended it instead.
        status: Option<i32>,
        /// Everything it wrote to stderr, unedited. `brief` takes the first
        /// non-blank line, which for `gh` and `git` is usually the sentence
        /// worth showing; the rest is kept because a log wants all of it.
        stderr: String,
    },
    /// The output was not the JSON we expected.
    Json {
        /// The subcommand whose output would not parse.
        args: String,
        /// Where serde gave up. Carried so `source()` can reach the line and
        /// column, which is the only thing that makes a schema drift findable.
        source: serde_json::Error,
    },
    /// A required field is missing from an otherwise valid response.
    Field {
        /// The subcommand that answered.
        args: String,
        /// The field we asked for and did not get. Our spelling of it, taken
        /// from the request — the response, by definition, does not have it.
        field: &'static str,
    },
}

impl Error {
    /// Short text for the status bar: one line, with no nested context.
    pub fn brief(&self) -> String {
        match self {
            Self::Spawn {
                program,
                install,
                source,
            } if source.kind() == io::ErrorKind::NotFound => install.map_or_else(
                || format!("{program} not found — is it installed?"),
                |advice| format!("{program} not found — {advice}"),
            ),
            Self::Spawn {
                program, source, ..
            } => format!("could not run {program}: {source}"),
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
            Self::Spawn {
                program, source, ..
            } => write!(f, "could not run {program}: {source}"),
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

/// `std::result::Result` with this module's `Error` already filled in.
///
/// Imported under an alias where it would shadow the standard one — `Result
/// as Res` in `mux` — because a module with both kinds in it is a module
/// where the bare name has to be looked up rather than read.
pub type Result<T> = std::result::Result<T, Error>;

/// Why a piece of data is not here.
///
/// Wider than `Error` on purpose. Most failures are a program we ran saying
/// no, and those keep their `Error` whole — `is_transient` and the cause
/// chain are the reason it exists. But some are this program declining: a
/// file too large to open, a worker thread that is gone. Those have no
/// subprocess underneath, and inventing one to fit — a `Spawn` error for a
/// dead thread — reads as a lie the first time somebody prints the cause.
#[derive(Debug)]
pub enum Failure {
    /// Something we ran failed.
    Ran(Error),
    /// Something we decided, with nothing underneath it.
    Refused(String),
}

impl Failure {
    /// One line, for a status bar or an empty pane.
    pub fn brief(&self) -> String {
        match self {
            Self::Ran(e) => e.brief(),
            Self::Refused(msg) => msg.clone(),
        }
    }

    /// Is trying again worthwhile? A decision is never transient — it will be
    /// made the same way next time.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Ran(e) => e.is_transient(),
            Self::Refused(_) => false,
        }
    }

    /// The error underneath, when there is one.
    pub fn error(&self) -> Option<&Error> {
        match self {
            Self::Ran(e) => Some(e),
            Self::Refused(_) => None,
        }
    }
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        Self::Ran(e)
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ran(e) => write!(f, "{e}"),
            Self::Refused(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ran(e) => Some(e),
            Self::Refused(_) => None,
        }
    }
}

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
        let gone = |program, install| Error::Spawn {
            program,
            install,
            source: io::Error::from(io::ErrorKind::NotFound),
        };
        assert_eq!(
            gone("gh", Some("install the GitHub CLI")).brief(),
            "gh not found — install the GitHub CLI"
        );
        assert_eq!(
            gone("git", None).brief(),
            "git not found — is it installed?"
        );
        assert_eq!(
            gone("herdr", None).brief(),
            "herdr not found — is it installed?"
        );
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
            install: None,
            source: io::Error::from(io::ErrorKind::PermissionDenied),
        };
        assert!(e.source().is_some(), "the chain has to survive");
    }
}
