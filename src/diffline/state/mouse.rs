//! Clicks and the wheel.
//!
//! Everything here reads the regions the last frame recorded rather than
//! working the geometry out again. The renderer is the only part that knows
//! how wide the tree ended up and which rows survived the scroll, and a
//! second copy of that arithmetic would drift the first time a pane changed.

use crate::shared::key::{Button, Motion, Mouse};

use crate::diffline::app::{App, Pane};
use crate::diffline::hit::Target;

impl App {
    pub fn on_mouse(&mut self, ev: Mouse) {
        let (col, row) = (ev.col, ev.row);
        match ev.what {
            Motion::Down(Button::Left) => self.click(col, row),
            Motion::ScrollDown => self.wheel(col, row, 3),
            Motion::ScrollUp => self.wheel(col, row, -3),
            _ => {}
        }
    }

    /// Newest region first, which is what makes a modal shadow the pane it is
    /// drawn over without anything having to say so.
    fn at(&self, col: u16, row: u16) -> Option<crate::diffline::hit::Region> {
        self.hits
            .iter()
            .rev()
            .find(|r| r.contains(col, row))
            .copied()
    }

    fn click(&mut self, col: u16, row: u16) {
        let Some(hit) = self.at(col, row) else { return };
        match hit.target {
            Target::Scope(i) => {
                if let Some(s) = self.scopes.get(i).cloned() {
                    self.scope = s;
                    self.refresh();
                }
            }
            Target::QueueTab => self.toggle_queue_pane(),
            Target::Modal => {
                if let Some(i) = hit.index_at(row) {
                    self.sel = i;
                    self.accept_modal();
                }
            }
            Target::Pane(pane) => {
                // Going somewhere is what a click means; what it selects
                // there depends on the pane.
                self.pane = pane;
                match (pane, hit.index_at(row)) {
                    (Pane::Tree, Some(i)) => self.goto_file(i),
                    (Pane::Diff, Some(i)) => self.click_row(i),
                    (Pane::Queue, Some(i)) => self.queue_sel = i,
                    _ => {}
                }
            }
        }
    }

    fn wheel(&mut self, col: u16, row: u16, d: i64) {
        // The wheel acts on what is under the pointer, without taking focus:
        // reading a pane you are not working in is a thing people do.
        let Some(hit) = self.at(col, row) else { return };
        match hit.target {
            Target::Pane(Pane::Tree) => {
                self.tree_scroll = (self.tree_scroll as i64 + d).max(0) as usize;
            }
            Target::Pane(Pane::Queue) => {
                self.queue_sel = step(self.queue_sel, d, self.comments.len());
            }
            _ => {
                self.diff_scroll = (self.diff_scroll as i64 + d).max(0) as usize;
            }
        }
    }
}

fn step(current: usize, d: i64, len: usize) -> usize {
    (current as i64 + d).clamp(0, (len as i64 - 1).max(0)) as usize
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
    use ratatui::layout::Rect;

    fn ev(what: Motion, col: u16, row: u16) -> Mouse {
        Mouse { col, row, what }
    }

    fn area() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 30,
            height: 10,
        }
    }

    #[test]
    fn a_click_goes_to_the_pane_it_landed_on() {
        let mut a = crate::diffline::app::App::new(
            "/tmp/r".into(),
            crate::diffline::model::Scope::WorkingTree,
            vec![crate::diffline::model::Scope::WorkingTree],
            None,
        );
        a.pane = Pane::Diff;
        a.hits.push(crate::diffline::hit::Region::plain(
            Target::Pane(Pane::Queue),
            area(),
        ));
        a.on_mouse(ev(Motion::Down(Button::Left), 5, 5));
        assert_eq!(a.pane, Pane::Queue);
    }

    #[test]
    fn the_newest_region_wins() {
        // A modal is drawn after the panes, so it is later in the list — and
        // a click on it must not fall through to what it covers.
        let mut a = crate::diffline::app::App::new(
            "/tmp/r".into(),
            crate::diffline::model::Scope::WorkingTree,
            vec![crate::diffline::model::Scope::WorkingTree],
            None,
        );
        a.hits.push(crate::diffline::hit::Region::plain(
            Target::Pane(Pane::Tree),
            area(),
        ));
        a.hits
            .push(crate::diffline::hit::Region::plain(Target::Modal, area()));
        assert_eq!(a.at(5, 5).map(|r| r.target), Some(Target::Modal));
    }

    #[test]
    fn a_click_on_nothing_does_nothing() {
        let mut a = crate::diffline::app::App::new(
            "/tmp/r".into(),
            crate::diffline::model::Scope::WorkingTree,
            vec![crate::diffline::model::Scope::WorkingTree],
            None,
        );
        let before = a.pane;
        a.on_mouse(ev(Motion::Down(Button::Left), 5, 5));
        assert_eq!(a.pane, before, "there was no region there");
    }

    #[test]
    fn the_wheel_scrolls_what_is_under_it_without_taking_focus() {
        let mut a = crate::diffline::app::App::new(
            "/tmp/r".into(),
            crate::diffline::model::Scope::WorkingTree,
            vec![crate::diffline::model::Scope::WorkingTree],
            None,
        );
        a.pane = Pane::Diff;
        a.hits.push(crate::diffline::hit::Region::plain(
            Target::Pane(Pane::Tree),
            area(),
        ));
        a.on_mouse(ev(Motion::ScrollDown, 5, 5));
        assert!(a.tree_scroll > 0, "the tree scrolled");
        assert_eq!(a.pane, Pane::Diff, "and the focus stayed put");
    }
}
