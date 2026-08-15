//! Where the last frame put things, so a click can know what it hit.
//!
//! The renderer is the only part of the program that knows the geometry: how
//! wide the sidebar ended up, which rows survived the scroll, how tall a card
//! is. Rather than duplicate that arithmetic in the input layer — where it
//! would drift out of step the first time a pane changed — each pane records
//! the rectangle it drew and the little that is needed to turn a row back into
//! an index.
//!
//! Regions are rebuilt every frame and consulted newest-first, which is what
//! makes a modal shadow the panes underneath it without anything having to say
//! so.

use ratatui::layout::{Position, Rect};

use super::Pane;

/// What a click can land on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A pane of the interface proper.
    Pane(Pane),
    /// One tab of the tab bar, by index into `data::TABS`.
    Tab(usize),
    /// A row of a modal. These are not panes: while one is up it owns the
    /// keyboard, and it owns the mouse on the same terms.
    Finder,
    Themes,
    Accounts,
    Dispatch,
}

/// A rectangle the last frame drew, and enough about its contents to map a
/// row to an entry.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub target: Target,
    pub area: Rect,
    /// Rows one entry occupies: list cards are two tall, log lines one.
    pub row_h: u16,
    /// Index of the first entry drawn, so a click reads through the scroll.
    pub scroll: usize,
    /// How many entries exist, so a click past the last one selects nothing
    /// rather than an entry that is not there.
    pub len: usize,
}

impl Region {
    /// A region whose rows are entries that can be selected.
    pub fn rows(target: Target, area: Rect, row_h: u16, scroll: usize, len: usize) -> Self {
        Self {
            target,
            area,
            row_h: row_h.max(1),
            scroll,
            len,
        }
    }

    /// A region that is only worth focusing and scrolling — flowing text
    /// rather than a list of entries.
    pub fn plain(target: Target, area: Rect) -> Self {
        Self::rows(target, area, 1, 0, 0)
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        self.area.contains(Position::new(col, row))
    }

    /// Which entry sits at this row, if any.
    ///
    /// `None` covers both a region with no entries and the empty space below
    /// the last one — clicking there should leave the selection where it is,
    /// not drag it to the end of the list.
    pub fn index_at(&self, row: u16) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let offset = row.checked_sub(self.area.y)? / self.row_h;
        let i = self.scroll + offset as usize;
        (i < self.len).then_some(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect {
            x: 10,
            y: 5,
            width: 40,
            height: 10,
        }
    }

    fn region(row_h: u16, scroll: usize, len: usize) -> Region {
        Region::rows(Target::Pane(Pane::List), area(), row_h, scroll, len)
    }

    #[test]
    fn a_point_outside_the_rectangle_is_not_in_it() {
        let r = region(1, 0, 50);
        assert!(r.contains(10, 5), "the top left corner is inside");
        assert!(r.contains(49, 14), "and so is the bottom right");
        assert!(!r.contains(9, 5));
        assert!(!r.contains(50, 5), "right is exclusive");
        assert!(!r.contains(10, 15), "bottom is exclusive");
    }

    #[test]
    fn a_row_maps_to_the_entry_drawn_on_it() {
        let r = region(1, 0, 50);
        assert_eq!(r.index_at(5), Some(0));
        assert_eq!(r.index_at(6), Some(1));
    }

    #[test]
    fn the_scroll_offset_is_added_back_in() {
        let r = region(1, 20, 50);
        assert_eq!(r.index_at(5), Some(20), "the first row drawn is entry 20");
        assert_eq!(r.index_at(8), Some(23));
    }

    #[test]
    fn both_rows_of_a_two_row_card_are_the_same_entry() {
        let r = region(2, 0, 50);
        assert_eq!(r.index_at(5), Some(0));
        assert_eq!(r.index_at(6), Some(0), "the subtitle line is still row 0");
        assert_eq!(r.index_at(7), Some(1));
    }

    #[test]
    fn the_empty_space_past_the_last_entry_is_nothing() {
        // three entries drawn into a ten-row pane, from y = 5
        let r = region(1, 0, 3);
        assert_eq!(r.index_at(7), Some(2), "the last of the three");
        assert_eq!(r.index_at(8), None, "and the blank space below it");
    }

    #[test]
    fn an_empty_region_has_no_entry_anywhere() {
        assert_eq!(region(1, 0, 0).index_at(5), None);
        assert_eq!(
            Region::plain(Target::Pane(Pane::Body), area()).index_at(5),
            None
        );
    }

    #[test]
    fn a_row_above_the_region_is_nothing_rather_than_a_wrapped_index() {
        // subtraction here would wrap into an enormous index
        assert_eq!(region(1, 0, 50).index_at(4), None);
    }

    #[test]
    fn a_zero_row_height_cannot_divide_by_zero() {
        let r = Region::rows(Target::Pane(Pane::List), area(), 0, 0, 50);
        assert_eq!(r.index_at(6), Some(1));
    }
}
