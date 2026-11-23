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

/// Steps `current` by `delta` over a list of `len` rows, clamping at both
/// ends. An empty list has nowhere to go, so `0` is returned when `len == 0`.
#[must_use]
pub fn step(current: usize, delta: i64, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let clamped = current.min(last);
    let by = usize::try_from(delta.unsigned_abs()).unwrap_or(usize::MAX);
    if delta >= 0 {
        clamped.saturating_add(by).min(last)
    } else {
        clamped.saturating_sub(by)
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
    fn a_direction_steps_one_either_way() {
        assert_eq!(Dir::Prev.step(), -1);
        assert_eq!(Dir::Next.step(), 1);
    }

    #[test]
    fn step_clamps_to_empty_and_bounds() {
        assert_eq!(step(3, 1, 0), 0);
        assert_eq!(step(0, -5, 5), 0);
        assert_eq!(step(4, 5, 5), 4);
        assert_eq!(step(2, -1, 5), 1);
        assert_eq!(step(2, 2, 5), 4);
        assert_eq!(step(2, i64::MIN, 5), 0);
        assert_eq!(step(2, i64::MAX, 5), 4);
    }

    #[test]
    fn step_moves_from_the_drawn_row_when_the_index_is_past_the_end() {
        assert_eq!(step(10, -1, 5), 3);
    }
}
