//! Rectangles and scroll offsets. No buffer, no drawing.
//!
//! The level below the atoms: these answer "where does it go" so that
//! everything above can be about "what does it look like". Being pure
//! arithmetic over `Rect` and `usize`, they are also the cheapest things here
//! to test — no terminal, no fixture, just numbers in and numbers out.

use ratatui::layout::{Rect, Size};

/// A percentage of the available width, so a skeleton keeps its proportions at
/// any pane size.
///
/// The arithmetic goes through `u32` because the obvious `avail * p / 100`
/// overflows at any width past 655 — a pane wider than that is unusual and a
/// skeleton bar that wraps to nothing in it is not worth the surprise.
///
/// ```rust
/// use github_tui::tui::geom::pct;
///
/// assert_eq!(pct(80, 25), 20);
/// assert_eq!(pct(u16::MAX, 100), u16::MAX);
/// ```
#[must_use]
pub fn pct(avail: u16, p: u16) -> u16 {
    (u32::from(avail) * u32::from(p) / 100) as u16
}

/// Keeps `sel` visible inside a window of `height` rows.
///
/// Moves `offset` as little as it can: a selection already on screen leaves it
/// alone, so paging through a list does not jump the rows under the cursor.
///
/// ```rust
/// use github_tui::tui::geom::scroll_into_view;
///
/// let mut offset = 0;
/// scroll_into_view(&mut offset, 12, 5, 100);
/// assert_eq!(offset, 8, "row 12 is the last of the five, not the first");
/// ```
pub fn scroll_into_view(offset: &mut usize, sel: usize, height: usize, len: usize) {
    if height == 0 {
        return;
    }
    if sel < *offset {
        *offset = sel;
    } else if sel >= *offset + height {
        *offset = sel + 1 - height;
    }
    let max = len.saturating_sub(height);
    *offset = (*offset).min(max);

    // What this function is *for*: after it, the selection is on screen. It
    // is easy to write a version that clamps the offset and quietly loses
    // that, which is a cursor you cannot see and cannot find.
    debug_assert!(
        sel >= len || (sel >= *offset && sel < *offset + height),
        "selection {sel} is outside the window {}..{} of {len}",
        *offset,
        *offset + height
    );
}

/// Centred, never larger than what it is centred in.
///
/// `want` is what the modal would like; what comes back is what the terminal
/// has. Asking for more than fits is the ordinary case, not an error — a help
/// modal wants eighty columns and gets whatever the window is.
///
/// ```rust
/// use github_tui::tui::geom::centered;
/// use ratatui::layout::{Rect, Size};
///
/// let screen = Rect { x: 0, y: 0, width: 40, height: 10 };
/// assert_eq!(centered(screen, Size::new(80, 30)), screen, "asked for more than there is");
/// ```
#[must_use]
pub fn centered(area: Rect, want: Size) -> Rect {
    inset(area, want, Size::new(0, 0))
}

/// Centred with a gutter kept either side, so the thing underneath still
/// shows and the modal reads as floating over it rather than replacing it.
///
/// The difference from `centered` used to be the difference between the two
/// programs' copies of this function, which is to say it was an accident.
/// Named, it is a choice: diffline's modals sit over a diff you are still
/// reading, github-tui's cover a list you are done with.
///
/// The gutter is four columns and two rows, and it is taken off the room
/// available rather than off the size asked for — so the two differ only where
/// the request would have filled the screen.
///
/// ```rust
/// use github_tui::tui::geom::{centered, centered_over};
/// use ratatui::layout::{Rect, Size};
///
/// let screen = Rect { x: 0, y: 0, width: 80, height: 24 };
/// let want = Size::new(100, 30);
/// assert_eq!(centered(screen, want).width, 80);
/// assert_eq!(centered_over(screen, want).width, 76, "two columns each side");
/// ```
#[must_use]
pub fn centered_over(area: Rect, want: Size) -> Rect {
    inset(area, want, Size::new(4, 2))
}

/// Both pairs are `Size` rather than four `u16`s: this took five numbers in a
/// row, of which the middle four were the same type and only their order said
/// which was which.
#[must_use]
fn inset(area: Rect, want: Size, gutter: Size) -> Rect {
    let w = want.width.min(area.width.saturating_sub(gutter.width));
    let h = want.height.min(area.height.saturating_sub(gutter.height));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_into_view_does_nothing_when_the_selection_is_visible() {
        let mut off = 5;
        scroll_into_view(&mut off, 7, 10, 100);
        assert_eq!(off, 5);
    }

    #[test]
    fn scroll_into_view_follows_the_selection_up_and_down() {
        let mut off = 10;
        scroll_into_view(&mut off, 3, 5, 100);
        assert_eq!(off, 3, "scrolls up to reveal the selection");

        let mut off = 0;
        scroll_into_view(&mut off, 12, 5, 100);
        assert_eq!(off, 8, "scrolls down just enough");
    }

    #[test]
    fn scroll_into_view_never_scrolls_past_the_end() {
        let mut off = 90;
        scroll_into_view(&mut off, 5, 10, 20);
        assert!(off <= 10, "offset stays within len - height");
    }

    #[test]
    fn scroll_into_view_copes_with_a_list_shorter_than_the_window() {
        let mut off = 3;
        scroll_into_view(&mut off, 0, 20, 2);
        assert_eq!(off, 0);
    }

    #[test]
    fn scroll_into_view_ignores_a_zero_height_pane() {
        let mut off = 4;
        scroll_into_view(&mut off, 9, 0, 50);
        assert_eq!(off, 4, "a pane with no rows cannot scroll");
    }

    #[test]
    fn pct_keeps_proportions_and_never_overflows() {
        assert_eq!(pct(100, 50), 50);
        assert_eq!(pct(0, 80), 0);
        assert_eq!(pct(u16::MAX, 100), u16::MAX);
    }
}
