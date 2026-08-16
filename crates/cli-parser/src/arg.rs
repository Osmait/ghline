//! The syntax-level argument produced by [`crate::Parser`].

use std::ffi::OsString;
use std::fmt;

/// One command-line argument, classified without assigning it application
/// semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    /// A `--name` option, with the value from `--name=value` when present.
    Long {
        /// The name without its leading `--`.
        name: String,
        /// The value following `=`, including an explicitly empty value.
        value: Option<OsString>,
    },
    /// Everything after the leading `-` of a short option or option group.
    ///
    /// The parser deliberately keeps groups intact: only the application can
    /// know whether `-abc` means three flags or option `a` with value `bc`.
    Short(String),
    /// A positional value, including every argument after `--`.
    Value(OsString),
}

impl Arg {
    /// Reconstructs the argument as it appeared on the command line.
    #[must_use]
    pub fn into_raw(self) -> OsString {
        match self {
            Self::Long { name, value } => {
                let mut raw = OsString::from("--");
                raw.push(name);
                if let Some(value) = value {
                    raw.push("=");
                    raw.push(value);
                }
                raw
            }
            Self::Short(name) => {
                let mut raw = OsString::from("-");
                raw.push(name);
                raw
            }
            Self::Value(value) => value,
        }
    }
}

impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Long { name, value } => {
                write!(f, "--{name}")?;
                if let Some(value) = value {
                    write!(f, "={}", value.to_string_lossy())?;
                }
                Ok(())
            }
            Self::Short(name) => write!(f, "-{name}"),
            Self::Value(value) => write!(f, "{}", value.to_string_lossy()),
        }
    }
}
