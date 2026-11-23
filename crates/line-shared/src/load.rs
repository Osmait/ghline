//! Load state of one piece of data, shared across both applications.

use std::sync::Arc;

use crate::error::{Error, Failure};

/// Load state of one piece of data.
#[derive(Clone, Debug, Default)]
pub enum Load {
    /// Not requested yet.
    #[default]
    Idle,
    /// A request is currently in flight.
    Loading,
    /// Data arrived and is ready to display.
    Ready,
    /// Why it is not here, kept whole.
    ///
    /// The failure rather than a sentence about it: a `Failure` knows whether
    /// it is worth retrying and what caused it, and flattening it here threw
    /// both away. `Arc` because the state is cloned to be rendered and
    /// `io::Error` is not `Clone`; sharing is right anyway, since there is one
    /// failure and several places that describe it.
    Failed(Arc<Failure>),
}

impl Load {
    /// Whether a request is currently in flight.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        *self == Self::Loading
    }

    /// The failure, for anything that wants more than a sentence.
    #[must_use]
    pub fn failure(&self) -> Option<&Failure> {
        match self {
            Self::Failed(f) => Some(f),
            _ => None,
        }
    }

    /// One line for a status bar or an empty pane.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.failure().map(Failure::brief)
    }

    /// Whether trying again might work.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.failure().is_some_and(Failure::is_transient)
    }

    /// A failure this program decided on, with no error underneath.
    #[must_use]
    pub fn refused(msg: impl Into<String>) -> Self {
        Self::Failed(Arc::new(Failure::Refused(msg.into())))
    }
}

impl From<Error> for Load {
    fn from(e: Error) -> Self {
        Self::Failed(Arc::new(Failure::Ran(e)))
    }
}

impl PartialEq for Load {
    /// Two failures are the same state as far as the interface is concerned:
    /// it asks "loading, ready, or broken", not which error it was.
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Idle, Self::Idle)
                | (Self::Loading, Self::Loading)
                | (Self::Ready, Self::Ready)
                | (Self::Failed(_), Self::Failed(_))
        )
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
    fn failed_states_compare_equal_regardless_of_message() {
        let a = Load::refused("first");
        let b = Load::refused("second");
        assert_eq!(a, b);
        assert_eq!(a.error().as_deref(), Some("first"));
        assert!(!a.is_transient());
        assert!(!a.is_loading());
    }
}
