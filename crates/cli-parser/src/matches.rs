//! Typed results produced from an application schema.

use std::ffi::{OsStr, OsString};

/// The values associated with one application-defined argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Occurrence<I> {
    pub(crate) id: I,
    pub(crate) values: Vec<OsString>,
}

/// Successfully validated arguments indexed by the IDs from their schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matches<I> {
    pub(crate) occurrences: Vec<Occurrence<I>>,
}

impl<I> Matches<I>
where
    I: PartialEq,
{
    /// Reports whether an argument occurred.
    #[must_use]
    pub fn contains(&self, id: &I) -> bool {
        self.occurrences
            .iter()
            .any(|occurrence| occurrence.id == *id)
    }

    /// Returns the first value carried by an argument.
    #[must_use]
    pub fn value(&self, id: &I) -> Option<&OsStr> {
        self.values(id)
            .and_then(|values| values.first().map(OsString::as_os_str))
    }

    /// Returns every value carried by an argument.
    #[must_use]
    pub fn values(&self, id: &I) -> Option<&[OsString]> {
        self.occurrences
            .iter()
            .find(|occurrence| occurrence.id == *id)
            .map(|occurrence| occurrence.values.as_slice())
    }
}

/// A parsed command line or one of the two built-in informational responses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<I> {
    /// Application arguments ready to map into its command model.
    Matches(Matches<I>),
    /// Complete help text requested by `-h` or `--help`.
    Help(String),
    /// Complete version text requested by `-V` or `--version`.
    Version(String),
}
