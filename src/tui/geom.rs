//! Rectangles and scroll offsets. No buffer, no drawing.
//!
//! The level below the atoms: these answer "where does it go" so that
//! everything above can be about "what does it look like". Being pure
//! arithmetic over `Rect` and `usize`, they are also the cheapest things here
//! to test — no terminal, no fixture, just numbers in and numbers out.

use ratatui::layout::Rect;

/// A percentage of the available width, so a skeleton keeps its proportions at
/// any pane size.
pub fn pct(avail: u16, p: u16) -> u16 {
    (u32::from(avail) * u32::from(p) / 100) as u16
}

/// Keeps `sel` visible inside a window of `height` rows.
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
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 0, 0)
}

/// Centred with a gutter kept either side, so the thing underneath still
/// shows and the modal reads as floating over it rather than replacing it.
///
/// The difference from `centered` used to be the difference between the two
/// programs' copies of this function, which is to say it was an accident.
/// Named, it is a choice: diffline's modals sit over a diff you are still
/// reading, github-tui's cover a list you are done with.
pub fn centered_over(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 4, 2)
}

fn inset(area: Rect, w: u16, h: u16, mx: u16, my: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(mx));
    let h = h.min(area.height.saturating_sub(my));
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
