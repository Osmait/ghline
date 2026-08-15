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
    Prev,
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

    pub fn flip(self) -> Self {
        match self {
            Self::Prev => Self::Next,
            Self::Next => Self::Prev,
        }
    }
}

/// One of the ends, or the middle.
///
/// `Middle` is only meaningful where there is a window to be in the middle
/// of; the ends mean the same everywhere.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Top,
    Middle,
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

    #[test]
    fn flipping_twice_is_where_you_started() {
        for d in [Dir::Prev, Dir::Next] {
            assert_eq!(d.flip().flip(), d);
        }
    }
}
