//! The words for moving around.
//!
//! Both programs step through lists and jump to ends, and both had been
//! spelling it with integers and booleans: `-1` and `1` are only a direction
//! by agreement, and `goto(true)` is only the top if you go and read the
//! signature. Neither is a fact the compiler was checking.
//!
//! Small enough to look like ceremony, and it is not: `Dir` has two values
//! where an `i64` has eighteen quintillion, and a call site that says
//! `Dir::Prev` needs no comment.

/// Backwards or forwards through a list.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dir {
    /// Towards the start of the list, which on screen is upwards.
    Prev,
    /// Towards the end, which on screen is downwards.
    Next,
}

impl Dir {
    /// For arithmetic that still counts in rows.
    pub fn step(self) -> i64 {
        match self {
            Self::Prev => -1,
            Self::Next => 1,
        }
    }
}

/// One of the ends, or the middle.
///
/// `Middle` is only meaningful where there is a window to be in the middle
/// of; the ends mean the same everywhere.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    /// The first row — of the list, for a jump to the start, or of the
    /// window, for the screen-relative moves.
    Top,
    /// The middle row of the window. Only the screen-relative moves and the
    /// scroll-cursor-here ones pass this; a jump through a list never does.
    Middle,
    /// The last row, read the same two ways as `Top`.
    Bottom,
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
    fn a_direction_steps_one_either_way() {
        assert_eq!(Dir::Prev.step(), -1);
        assert_eq!(Dir::Next.step(), 1);
    }
}
