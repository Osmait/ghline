//! Clicks, drags and the wheel.
//!
//! Everything here reads the regions the last frame recorded rather than
//! working the geometry out again. The renderer is the only part that knows
//! how wide the tree ended up and which rows survived the scroll, and a
//! second copy of that arithmetic would drift the first time a pane changed.
//!
//! Nothing here is a new capability, which is the same bargain ghline makes:
//! a click is `h`/`l` then `j`/`k`, a double click is `enter` — or `c`, on a
//! diff line, because commenting is what drilling into a line means here — a
//! drag is `V` plus the motion it sweeps, and the wheel is `^e`/`^y` over the
//! diff and `j`/`k` over the lists. What the mouse adds is aim: it acts on
//! what is under the pointer, which is the one thing a keystroke cannot say.

use std::time::{Duration, Instant};

use ratatui::layout::Position;

use crate::shared::key::{Button, Motion, Mouse};

use crate::app::{App, Modal, Pane};
use crate::hit::Target;

/// Rows the wheel moves per notch. Three is the common terminal default and
/// reads as a nudge rather than a jump.
const WHEEL: i64 = 3;

/// Columns a horizontal notch pans. Twice the vertical notch because a cell
/// is about half as wide as it is tall, so the two feel like the same
/// distance on screen.
const WHEEL_COLS: i64 = 6;

/// How close together two clicks have to be to count as one double click.
/// Roughly the usual desktop default; long enough to be deliberate, short
/// enough that two unrelated clicks on the same row do not open anything.
const DOUBLE: Duration = Duration::from_millis(400);

impl App {
    pub fn on_mouse(&mut self, ev: Mouse) {
        self.on_mouse_at(ev, Instant::now());
    }

    /// The clock is a parameter so that double-click timing can be tested
    /// without a test having to wait out a real four hundred milliseconds.
    pub fn on_mouse_at(&mut self, ev: Mouse, now: Instant) {
        // The one place the event's two `u16`s are read in order. Everything
        // below takes the pair as a `Position`, so this line is the only one
        // that could put them the wrong way round — and it is next to the
        // field names that say which is which.
        let at = Position::new(ev.col, ev.row);
        match ev.what {
            Motion::Down(Button::Left) => self.click(at, now),
            Motion::Drag(Button::Left) => self.drag(at),
            Motion::ScrollDown => self.wheel(at, WHEEL),
            Motion::ScrollUp => self.wheel(at, -WHEEL),
            Motion::ScrollRight => self.hwheel(at, WHEEL_COLS),
            Motion::ScrollLeft => self.hwheel(at, -WHEEL_COLS),
            _ => {}
        }
    }

    /// Newest region first, which is what makes a modal shadow the pane it is
    /// drawn over without anything having to say so.
    fn region_at(&self, at: Position) -> Option<crate::hit::Region> {
        self.hits.iter().rev().find(|r| r.contains(at)).copied()
    }

    /// Records this click and reports whether it completes a double click:
    /// the same cell, quickly enough.
    ///
    /// Position rather than entry, because a double click is a thing the hand
    /// does — two clicks that drifted onto different rows were two clicks.
    fn is_repeat_click(&mut self, at: Position, now: Instant) -> bool {
        let repeat = self
            .last_click
            .is_some_and(|(was, when)| was == at && now.duration_since(when) <= DOUBLE);
        // Cleared on a match so that three clicks are one double click and a
        // spare, not two overlapping ones.
        self.last_click = if repeat { None } else { Some((at, now)) };
        repeat
    }

    fn click(&mut self, at: Position, now: Instant) {
        let hit = self.region_at(at);

        // Clicking away from a modal closes it, on the same terms as `esc`.
        // While one is up it owns the mouse as it owns the keyboard: acting
        // on a pane it covers would move things under a box the reader is
        // still looking at.
        if self.modal.is_some() && !matches!(hit.map(|r| r.target), Some(Target::Modal)) {
            self.modal = None;
            self.query.clear();
            self.draft.clear();
            return;
        }

        let Some(hit) = hit else {
            return;
        };
        let index = hit.index_at(at.y);
        let repeat = self.is_repeat_click(at, now);

        match hit.target {
            Target::Scope(i) => {
                if let Some(s) = self.scopes.get(i).cloned() {
                    self.scope = s;
                    self.refresh();
                }
            }
            Target::QueueTab => self.toggle_queue_pane(),
            Target::Modal => {
                if let Some(i) = index {
                    self.sel = i;
                    // Landing on a row previews it exactly as `j` would —
                    // which for the theme picker means painting the frame in
                    // that theme.
                    self.preview_theme();
                    if repeat {
                        self.accept_modal();
                    }
                }
            }
            Target::Pane(pane) => {
                // Going somewhere is what a click means; what it selects
                // there depends on the pane, and a double click drills in the
                // way `enter` does.
                self.pane = pane;
                match (pane, index) {
                    (Pane::Tree, Some(i)) => {
                        self.goto_file(i);
                        if repeat {
                            self.pane = Pane::Diff;
                        }
                    }
                    (Pane::Diff, Some(i)) => {
                        self.click_row(i);
                        // Only when both clicks landed on the line: a double
                        // click on a hunk header has nothing to say.
                        if repeat && self.cursor == i {
                            self.open_comment();
                        }
                    }
                    (Pane::Queue, Some(i)) => {
                        self.queue_sel = i;
                        if repeat {
                            self.goto_comment(i);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// A drag over the diff sweeps a visual selection behind it: press on the
    /// first line, pull to the last, and `c` writes one note about the range.
    fn drag(&mut self, at: Position) {
        if self.modal.is_some() {
            return;
        }
        let Some(hit) = self.region_at(at) else {
            return;
        };
        if hit.target != Target::Pane(Pane::Diff) {
            return;
        }
        let Some(i) = hit.index_at(at.y) else {
            return;
        };
        if !self.diff_rows().get(i).is_some_and(|r| r.kind.is_code()) {
            return;
        }
        // The press already put the cursor on the first line; the selection
        // opens the moment the pointer leaves it, so a wobbly single click
        // does not leave visual mode behind.
        if self.anchor.is_none() {
            if i == self.cursor {
                return;
            }
            self.anchor = Some(self.cursor);
        }
        self.cursor = i;
    }

    /// The wheel acts on what is under the pointer, without taking focus:
    /// reading a pane you are not working in is a thing people do.
    fn wheel(&mut self, at: Position, d: i64) {
        let Some(hit) = self.region_at(at) else {
            return;
        };
        match hit.target {
            // The window moves and the cursor is dragged only at its edge —
            // the same motion as `^e`/`^y`, which is what makes it feel like
            // scrolling rather than like holding `j` down.
            Target::Pane(Pane::Diff) => self.scroll_view_by(d),
            Target::Pane(Pane::Tree) => {
                self.tree_scroll = step(self.tree_scroll, d, self.files.len());
            }
            Target::Pane(Pane::Queue) => {
                // One card per notch: a card is four rows tall, so one is
                // already the distance three lines are everywhere else.
                self.queue_sel = step(self.queue_sel, d.signum(), self.comments.len());
            }
            // A modal list scrolls by selection, since the selection is what
            // the list is for. The guard skips the frame around it, whose
            // region has no rows — stepping against its zero length would
            // yank the selection to the top.
            Target::Modal if hit.len > 0 => {
                self.sel = step(self.sel, d, hit.len);
                self.preview_theme();
            }
            _ => {}
        }
    }

    /// A horizontal notch pans long lines, the way `h` and `l` do, and stops
    /// at the longest one for the same reason they do: past it there is
    /// nothing to see.
    fn hwheel(&mut self, at: Position, d: i64) {
        if self.modal.is_some() {
            return;
        }
        let Some(hit) = self.region_at(at) else {
            return;
        };
        if hit.target != Target::Pane(Pane::Diff) {
            return;
        }
        let max = self.longest_line() as i64;
        self.hscroll = (self.hscroll as i64 + d).clamp(0, max) as usize;
    }

    /// Moving through the theme picker repaints in the theme under the
    /// selection, however the selection moved: the only way to judge one is
    /// to see it on the diff behind it.
    fn preview_theme(&mut self) {
        if self.modal == Some(Modal::Themes)
            && let Some(t) = crate::tui::theme::Theme::all().get(self.sel).copied()
        {
            crate::tui::theme::set(t);
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use crate::model::{ChangedFile, Kind, Row, Status};

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

    fn draw(a: &mut App) {
        let Ok(mut term) = Terminal::new(TestBackend::new(80, 12));
        let Ok(_) = term.draw(|f| crate::view::draw(f, a));
    }

    /// A frame wide enough for every pane, when a test needs real geometry.
    fn draw_wide(a: &mut App) {
        let Ok(mut term) = Terminal::new(TestBackend::new(160, 30));
        let Ok(_) = term.draw(|f| crate::view::draw(f, a));
    }

    fn app() -> App {
        crate::app::App::new(
            "/tmp/r".into(),
            crate::model::Scope::WorkingTree,
            vec![crate::model::Scope::WorkingTree],
            None,
        )
    }

    fn file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            status: Status::Modified,
            add: 1,
            del: 0,
        }
    }

    /// `n` context rows into the app's first file, cursor at the top.
    fn with_rows(a: &mut App, n: usize) {
        a.files.push(file("src/a.rs"));
        a.files_state = crate::app::Load::Ready;
        a.rows.insert(
            "src/a.rs".into(),
            (0..n as u32)
                .map(|i| Row {
                    kind: Kind::Context,
                    old: Some(i + 1),
                    new: Some(i + 1),
                    text: format!("line {i}"),
                })
                .collect(),
        );
        a.rows_state
            .insert("src/a.rs".into(), crate::app::Load::Ready);
    }

    /// Where the last frame drew row `i` of `pane`, from the regions it
    /// recorded — the same answer a real pointer would get.
    fn cell_of(a: &App, pane: Pane, i: usize) -> (u16, u16) {
        let r = a
            .hits
            .iter()
            .rev()
            .find(|r| r.target == Target::Pane(pane) && r.len > i && r.scroll <= i)
            .unwrap_or_else(|| panic!("no rows region for {pane:?} holds {i}"));
        (
            r.area.x + 2,
            r.area.y + ((i - r.scroll) as u16) * r.row_h.max(1),
        )
    }

    #[test]
    fn a_click_goes_to_the_pane_it_landed_on() {
        let mut a = app();
        a.pane = Pane::Diff;
        a.hits
            .push(crate::hit::Region::plain(Target::Pane(Pane::Queue), area()));
        a.on_mouse(ev(Motion::Down(Button::Left), 5, 5));
        assert_eq!(a.pane, Pane::Queue);
    }

    #[test]
    fn the_newest_region_wins() {
        // A modal is drawn after the panes, so it is later in the list — and
        // a click on it must not fall through to what it covers.
        let mut a = app();
        a.hits
            .push(crate::hit::Region::plain(Target::Pane(Pane::Tree), area()));
        a.hits
            .push(crate::hit::Region::plain(Target::Modal, area()));
        assert_eq!(
            a.region_at(Position::new(5, 5)).map(|r| r.target),
            Some(Target::Modal)
        );
    }

    #[test]
    fn a_click_on_nothing_does_nothing() {
        let mut a = app();
        let before = a.pane;
        a.on_mouse(ev(Motion::Down(Button::Left), 5, 5));
        assert_eq!(a.pane, before, "there was no region there");
    }

    #[test]
    fn a_click_on_a_row_selects_it_from_a_real_frame() {
        // The regression this pins: the pane's whole rectangle used to be
        // recorded *after* its rows, and newest-first hit-testing read the
        // pane instead — so a click focused the tree and selected nothing.
        let mut a = app();
        with_rows(&mut a, 20);
        a.files.push(file("src/b.rs"));
        a.rows.insert("src/b.rs".into(), Vec::new());

        draw_wide(&mut a);
        let (x, y) = cell_of(&a, Pane::Diff, 4);
        a.on_mouse(ev(Motion::Down(Button::Left), x, y));
        assert_eq!(a.cursor, 4, "a diff click puts the cursor on the row");

        draw_wide(&mut a);
        let (x, y) = cell_of(&a, Pane::Tree, 1);
        a.on_mouse(ev(Motion::Down(Button::Left), x, y));
        assert_eq!(a.file_idx, 1, "and a tree click chose the file under it");
    }

    #[test]
    fn the_wheel_scrolls_what_is_under_it_without_taking_focus() {
        let mut a = app();
        for i in 0..8 {
            a.files.push(file(&format!("src/f{i}.rs")));
        }
        a.pane = Pane::Diff;
        a.hits
            .push(crate::hit::Region::plain(Target::Pane(Pane::Tree), area()));
        a.on_mouse(ev(Motion::ScrollDown, 5, 5));
        assert!(a.tree_scroll > 0, "the tree scrolled");
        assert_eq!(a.pane, Pane::Diff, "and the focus stayed put");
    }

    #[test]
    fn the_wheel_moves_the_window_first_and_the_cursor_only_at_its_edge() {
        // The old feel, and the complaint: a notch moved the cursor three
        // rows while the window stood still, so scrolling did nothing visible
        // until the cursor hit an edge — and then you read pinned to it.
        let mut a = app();
        with_rows(&mut a, 40);
        a.cursor = 5;
        draw(&mut a);

        a.on_mouse(ev(Motion::ScrollDown, 40, 5));
        assert_eq!(a.diff_scroll, 3, "the window answered the first notch");
        assert_eq!(a.cursor, 5, "the cursor had room and stayed on its line");

        for _ in 0..2 {
            a.on_mouse(ev(Motion::ScrollDown, 40, 5));
            draw(&mut a);
        }
        assert_eq!(a.diff_scroll, 9);
        assert_eq!(a.cursor, 9, "dragged along the top edge, still on screen");
    }

    #[test]
    fn the_wheel_reaches_both_ends_of_a_diff() {
        // The wheel used to move only `diff_scroll`. On the next frame,
        // `scroll_into_view` moved it straight back to the stationary cursor,
        // so repeated wheel events could never reach either end.
        let mut a = app();
        a.tree_shown = false;
        with_rows(&mut a, 20);

        draw(&mut a);
        for _ in 0..10 {
            a.on_mouse(ev(Motion::ScrollDown, 40, 5));
            draw(&mut a);
        }
        assert_eq!(a.diff_scroll, 20 - a.view_height, "the bottom, exactly");
        assert!(a.cursor >= a.diff_scroll, "the cursor stayed on screen");

        for _ in 0..10 {
            a.on_mouse(ev(Motion::ScrollUp, 40, 5));
            draw(&mut a);
        }
        assert_eq!(a.diff_scroll, 0, "and back to the top");
        assert!(a.cursor < a.view_height, "with the cursor still on screen");
    }

    #[test]
    fn the_wheel_scrolls_the_split_view_in_its_own_units() {
        // Split lines are pairs of rows, and a window that scrolled in rows
        // would run out of diff halfway down the pane.
        let mut a = app();
        a.tree_shown = false;
        a.split = true;
        a.files.push(file("src/a.rs"));
        a.files_state = crate::app::Load::Ready;
        let rows: Vec<Row> = (0..12u32)
            .flat_map(|i| {
                [
                    Row {
                        kind: Kind::Deleted,
                        old: Some(i + 1),
                        new: None,
                        text: format!("old {i}"),
                    },
                    Row {
                        kind: Kind::Added,
                        old: None,
                        new: Some(i + 1),
                        text: format!("new {i}"),
                    },
                ]
            })
            .collect();
        a.rows.insert("src/a.rs".into(), rows);
        a.rows_state
            .insert("src/a.rs".into(), crate::app::Load::Ready);

        draw(&mut a);
        let pairs = crate::model::pair_rows(a.diff_rows()).len();
        for _ in 0..10 {
            a.on_mouse(ev(Motion::ScrollDown, 40, 5));
            draw(&mut a);
        }
        assert_eq!(a.diff_scroll, pairs - a.view_height, "pairs, not rows");
        assert!(a.diff_rows()[a.cursor].kind.is_code(), "still on a line");
    }

    #[test]
    fn a_split_click_lands_on_the_side_it_struck() {
        // The left half of a line is the old row and the right half the new
        // one — a click that ignored the side would anchor a note to the
        // wrong version of the line.
        let mut a = app();
        a.split = true;
        a.files.push(file("src/a.rs"));
        a.files_state = crate::app::Load::Ready;
        a.rows.insert(
            "src/a.rs".into(),
            vec![
                Row {
                    kind: Kind::Deleted,
                    old: Some(1),
                    new: None,
                    text: "old".into(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(1),
                    text: "new".into(),
                },
            ],
        );
        a.rows_state
            .insert("src/a.rs".into(), crate::app::Load::Ready);
        draw_wide(&mut a);

        let (x, y) = cell_of(&a, Pane::Diff, 0);
        a.on_mouse(ev(Motion::Down(Button::Left), x, y));
        assert_eq!(a.cursor, 0, "the left half is the deleted row");

        let (x, y) = cell_of(&a, Pane::Diff, 1);
        a.on_mouse(ev(Motion::Down(Button::Left), x, y));
        assert_eq!(a.cursor, 1, "and the right half is the added one");
    }

    #[test]
    fn wheeling_the_tree_reads_it_without_being_dragged_back() {
        // Scrolling the tree used to be undone a frame later: the renderer
        // snapped the list back to the selection on every draw, so the wheel
        // could never leave the selection's window.
        let mut a = app();
        with_rows(&mut a, 4);
        for i in 0..60 {
            a.files.push(file(&format!("src/f{i:02}.rs")));
        }
        draw_wide(&mut a);

        let (x, y) = cell_of(&a, Pane::Tree, 0);
        for _ in 0..4 {
            a.on_mouse(ev(Motion::ScrollDown, x, y));
            draw_wide(&mut a);
        }
        assert_eq!(a.tree_scroll, 12, "four notches held across four frames");
        assert_eq!(a.file_idx, 0, "reading the list chose nothing");

        // Moving the selection is what snaps it back into view.
        a.pane = Pane::Tree;
        a.on_key(crate::shared::key::Press::new(
            crate::shared::key::Key::Char('j'),
        ));
        draw_wide(&mut a);
        assert!(
            a.tree_scroll <= a.file_idx,
            "the selection pulled the list back to itself"
        );
    }

    #[test]
    fn a_drag_sweeps_a_selection_for_a_comment() {
        let mut a = app();
        with_rows(&mut a, 20);
        draw_wide(&mut a);

        let (x, y) = cell_of(&a, Pane::Diff, 2);
        a.on_mouse(ev(Motion::Down(Button::Left), x, y));
        assert_eq!(a.cursor, 2);
        assert!(!a.visual(), "a press alone selects nothing yet");

        let (x2, y2) = cell_of(&a, Pane::Diff, 6);
        a.on_mouse(ev(Motion::Drag(Button::Left), x2, y2));
        assert_eq!(a.span(), (2, 6), "the sweep is the selection");
        assert!(a.visual());

        // and dragging back shrinks it, as pulling `V` back does
        let (x3, y3) = cell_of(&a, Pane::Diff, 4);
        a.on_mouse(ev(Motion::Drag(Button::Left), x3, y3));
        assert_eq!(a.span(), (2, 4));
    }

    #[test]
    fn a_horizontal_notch_pans_the_code_and_stops_at_the_longest_line() {
        let mut a = app();
        with_rows(&mut a, 4);
        draw_wide(&mut a);

        let (x, y) = cell_of(&a, Pane::Diff, 1);
        a.on_mouse(ev(Motion::ScrollRight, x, y));
        assert_eq!(a.hscroll, 6, "one notch is six columns");
        for _ in 0..40 {
            a.on_mouse(ev(Motion::ScrollRight, x, y));
        }
        assert_eq!(a.hscroll, a.longest_line(), "blank columns read as broken");
        for _ in 0..40 {
            a.on_mouse(ev(Motion::ScrollLeft, x, y));
        }
        assert_eq!(a.hscroll, 0);
    }

    #[test]
    fn a_double_click_in_the_tree_lands_in_the_diff() {
        let mut a = app();
        with_rows(&mut a, 8);
        a.files.push(file("src/b.rs"));
        a.rows.insert("src/b.rs".into(), Vec::new());
        a.pane = Pane::Tree;
        draw_wide(&mut a);

        let now = Instant::now();
        let (x, y) = cell_of(&a, Pane::Tree, 1);
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.pane, Pane::Tree, "one click only chooses");
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.pane, Pane::Diff, "the second is `enter`");
        assert_eq!(a.file_idx, 1);
    }

    #[test]
    fn two_slow_clicks_are_two_clicks() {
        let mut a = app();
        with_rows(&mut a, 8);
        a.pane = Pane::Tree;
        draw_wide(&mut a);

        let now = Instant::now();
        let (x, y) = cell_of(&a, Pane::Tree, 0);
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        a.on_mouse_at(
            ev(Motion::Down(Button::Left), x, y),
            now + Duration::from_millis(600),
        );
        assert_eq!(a.pane, Pane::Tree, "too far apart to drill in");
    }

    #[test]
    fn a_double_click_on_a_line_opens_the_note_editor() {
        let mut a = app();
        with_rows(&mut a, 8);
        draw_wide(&mut a);

        let now = Instant::now();
        let (x, y) = cell_of(&a, Pane::Diff, 3);
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.modal, None);
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.modal, Some(Modal::Comment), "commenting is drilling in");
        assert_eq!(a.cursor, 3, "about the line that was struck");
    }

    #[test]
    fn a_double_click_on_a_queued_note_goes_back_to_its_lines() {
        let mut a = app();
        with_rows(&mut a, 12);
        a.queue_shown = true;
        let anchors = {
            a.cursor = 5;
            a.selected_anchors()
        };
        a.comments.push(crate::model::Comment {
            anchors,
            file: "src/a.rs".into(),
            snippet: "line 5".into(),
            body: "look here".into(),
            state: crate::model::State::Queued,
        });
        a.cursor = 0;
        draw_wide(&mut a);

        let now = Instant::now();
        let (x, y) = cell_of(&a, Pane::Queue, 0);
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.queue_sel, 0, "one click selects the note");
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.pane, Pane::Diff, "the second goes to the code");
        assert_eq!(a.cursor, 5, "on the line the note is about");
    }

    #[test]
    fn a_click_away_from_a_modal_closes_it_and_moves_nothing_under_it() {
        let mut a = app();
        with_rows(&mut a, 8);
        a.modal = Some(Modal::Finder);
        draw_wide(&mut a);

        let before = (a.pane, a.cursor, a.file_idx);
        a.on_mouse(ev(Motion::Down(Button::Left), 2, 5));
        assert_eq!(a.modal, None, "away means close, on `esc`'s terms");
        assert_eq!(
            (a.pane, a.cursor, a.file_idx),
            before,
            "and the pane it covered was not acted on"
        );
    }

    #[test]
    fn in_a_modal_one_click_selects_and_a_double_click_accepts() {
        let mut a = app();
        with_rows(&mut a, 8);
        a.files.push(file("src/b.rs"));
        a.rows.insert("src/b.rs".into(), Vec::new());
        a.modal = Some(Modal::Finder);
        draw_wide(&mut a);

        let list = a
            .hits
            .iter()
            .rev()
            .find(|r| r.target == Target::Modal && r.len > 0)
            .copied()
            .expect("the finder records its rows");
        let (x, y) = (list.area.x + 2, list.area.y + 1);
        let now = Instant::now();
        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.sel, 1, "chosen");
        assert_eq!(a.modal, Some(Modal::Finder), "and still open to look at");

        a.on_mouse_at(ev(Motion::Down(Button::Left), x, y), now);
        assert_eq!(a.modal, None, "the second click is `enter`");
        assert_eq!(a.path(), "src/b.rs", "and it jumped");
    }

    #[test]
    fn the_wheel_walks_a_modal_list_and_the_frame_around_it_does_not_reset_it() {
        // The finder rather than the theme picker, deliberately: walking the
        // themes repaints the process-wide palette, and a parallel test that
        // is comparing colours would see the swap mid-assertion.
        let mut a = app();
        with_rows(&mut a, 8);
        a.files.push(file("src/b.rs"));
        a.rows.insert("src/b.rs".into(), Vec::new());
        a.modal = Some(Modal::Finder);
        a.sel = 0;
        draw_wide(&mut a);

        let rows = a
            .hits
            .iter()
            .rev()
            .find(|r| r.target == Target::Modal && r.len > 0)
            .copied()
            .expect("the picker records its rows");
        a.on_mouse(ev(Motion::ScrollDown, rows.area.x + 2, rows.area.y));
        assert!(a.sel > 0, "the wheel moved the selection");
        assert!(a.sel < rows.len, "and a notch never leaves the list");

        let sel = a.sel;
        let outer = a
            .hits
            .iter()
            .find(|r| r.target == Target::Modal && r.len == 0)
            .copied()
            .expect("the frame is a region too");
        a.on_mouse(ev(Motion::ScrollDown, outer.area.x, outer.area.y));
        assert_eq!(a.sel, sel, "the frame is not a list");
    }
}
