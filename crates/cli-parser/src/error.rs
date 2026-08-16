//! Typed failures shared by every command-line grammar.

use std::error;
use std::ffi::OsString;
use std::fmt;

use crate::Arg;

/// A command line that cannot be interpreted safely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// An application supplied a contradictory or ambiguous schema.
    InvalidDefinition {
        /// The invariant the schema violated.
        detail: String,
    },
    /// An option begins with `-` but its name is not valid Unicode.
    InvalidOption(OsString),
    /// A long option has no name, as in `--=value`.
    MissingOptionName(OsString),
    /// An option or positional field has no required value.
    MissingValue {
        /// The option or field whose value is absent.
        argument: String,
    },
    /// A value cannot be converted to the type its application expects.
    InvalidValue {
        /// The option or positional field being converted.
        argument: String,
        /// The value exactly as the process received it.
        value: OsString,
        /// A short description such as `u16` or `UTF-8 text`.
        expected: &'static str,
    },
    /// The application's grammar does not accept this argument here.
    UnexpectedArgument(Arg),
    /// An option whose grammar permits one occurrence appeared again.
    DuplicateArgument {
        /// The repeated option or field.
        argument: String,
    },
}

impl Error {
    /// Builds an error for an argument the application's grammar rejects.
    #[must_use]
    pub fn unexpected(argument: Arg) -> Self {
        Self::UnexpectedArgument(argument)
    }

    /// Builds an error for an option that may occur only once.
    #[must_use]
    pub fn duplicate(argument: impl Into<String>) -> Self {
        Self::DuplicateArgument {
            argument: argument.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition { detail } => {
                write!(f, "invalid command-line definition: {detail}")
            }
            Self::InvalidOption(option) => {
                write!(f, "option is not valid UTF-8: {}", option.to_string_lossy())
            }
            Self::MissingOptionName(option) => {
                write!(f, "option has no name: {}", option.to_string_lossy())
            }
            Self::MissingValue { argument } => write!(f, "{argument} needs a value"),
            Self::InvalidValue {
                argument,
                value,
                expected,
            } => write!(
                f,
                "{argument} has invalid value '{}'; expected {expected}",
                value.to_string_lossy()
            ),
            Self::UnexpectedArgument(argument) => {
                write!(f, "unexpected argument '{argument}'")
            }
            Self::DuplicateArgument { argument } => {
                write!(f, "{argument} was provided more than once")
            }
        }
    }
}

impl error::Error for Error {}
