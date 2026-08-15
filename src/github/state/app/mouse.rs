//! The mouse, mapped onto the same moves the keyboard makes.
//!
//! Nothing here is a new capability: a click is `h`/`l` followed by `j`/`k`, a
//! double click is `enter`, the wheel is `j`/`k` held down. Keeping it that
//! way means the mouse cannot get the interface into a state the keyboard
//! could not, and that every rule about what selecting something implies is
//! still written in exactly one place.
//!
//! What the mouse does add is aim: it acts on the pane under the pointer,
//! which is the one thing a keystroke cannot express.

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::hit::{Region, Target};
use super::{App, Pane};

/// Lines the wheel moves per notch. Three is the common terminal default and
/// reads as a nudge rather than a jump.
const WHEEL: i64 = 3;

/// How close together two clicks have to be to count as one double click.
/// Roughly the usual desktop default; long enough to be deliberate, short
/// enough that two unrelated clicks on the same row do not open anything.
const DOUBLE: Duration = Duration::from_millis(400);

impl App {
    pub fn on_mouse(&mut self, ev: MouseEvent) {
        self.on_mouse_at(ev, Instant::now());
    }

    /// The clock is a parameter so that double-click timing can be tested
    /// without a test having to wait out a real four hundred milliseconds.
    pub fn on_mouse_at(&mut self, ev: MouseEvent, now: Instant) {
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(ev.column, ev.row, now),
            MouseEventKind::ScrollUp => self.wheel(ev.column, ev.row, -WHEEL),
            MouseEventKind::ScrollDown => self.wheel(ev.column, ev.row, WHEEL),
            _ => {}
        }
    }

    /// The topmost region under the pointer.
    ///
    /// Last drawn wins, which is what puts a modal in front of the panes it
    /// covers without either of them having to know about the other.
    fn region_at(&self, col: u16, row: u16) -> Option<Region> {
        self.hits
            .iter()
            .rev()
            .find(|r| r.contains(col, row))
            .copied()
    }

    /// Is something up that owns the input while it is there?
    fn modal_open(&self) -> bool {
        self.finder_open
            || self.themes_open
            || self.accounts_open
            || self.help_open
            || self.dispatch_open
    }

    fn click(&mut self, col: u16, row: u16, now: Instant) {
        // A confirmation is a question that wants a deliberate answer. A stray
        // click is not one, so it neither answers nor dismisses it.
        if self.prompt.is_some() {
            return;
        }

        let region = self.region_at(col, row);

        // Clicking away from a modal closes it, on the same terms as `esc` —
        // which for the theme picker means putting back the theme it opened on.
        if self.modal_open()
            && !matches!(
                region.map(|r| r.target),
                Some(Target::Finder | Target::Themes | Target::Accounts | Target::Dispatch)
            )
        {
            self.back();
            return;
        }

        let Some(region) = region else { return };
        let index = region.index_at(row);
        let repeat = self.is_repeat_click(col, row, now);

        match region.target {
            Target::Tab(i) => self.pick_tab(i),

            Target::Finder => {
                if let Some(i) = index {
                    self.finder_sel = i;
                    if repeat {
                        self.finder_accept();
                    }
                }
            }
            Target::Themes => {
                if let Some(i) = index {
                    self.theme_sel = i;
                    self.preview_theme();
                    if repeat {
                        self.themes_open = false;
                        self.accept_theme();
                    }
                }
            }
            Target::Accounts => {
                if let Some(i) = index {
                    self.acc_sel = i;
                    if repeat {
                        self.pick_account(i);
                    }
                }
            }
            Target::Dispatch => {
                if let Some(i) = index {
                    self.dispatch_sel = i;
                    if repeat {
                        self.dispatch_accept();
                    }
                }
            }

            Target::Pane(pane) => {
                self.pane = pane;
                if let Some(i) = index {
                    self.select_in(pane, i);
                }
                // A double click drills in, the way `enter` does. On a pane of
                // flowing text there is nothing to drill into, so the click
                // just leaves the focus there.
                if repeat && (index.is_some() || is_text(pane)) {
                    self.enter();
                }
            }
        }
    }

    /// Records this click and reports whether it completes a double click:
    /// the same cell, quickly enough.
    ///
    /// Position rather than entry, because a double click is a thing the hand
    /// does — two clicks that drifted onto different rows were two clicks.
    fn is_repeat_click(&mut self, col: u16, row: u16, now: Instant) -> bool {
        let repeat = self
            .last_click
            .is_some_and(|(c, r, at)| c == col && r == row && now.duration_since(at) <= DOUBLE);
        // Cleared on a match so that three clicks are one double click and a
        // spare, not two overlapping ones.
        self.last_click = if repeat { None } else { Some((col, row, now)) };
        repeat
    }

    /// The wheel turns whatever is under the pointer, focused or not: reaching
    /// for it to read a pane is not a decision to work in that pane.
    fn wheel(&mut self, col: u16, row: u16, d: i64) {
        let Some(region) = self.region_at(col, row) else {
            return;
        };
        match region.target {
            Target::Pane(pane) => self.move_pane_by(pane, d),
            Target::Finder => {
                self.finder_sel = step_sel(self.finder_sel, d, self.finder_len());
            }
            Target::Themes => {
                self.theme_sel =
                    step_sel(self.theme_sel, d, crate::shared::theme::Theme::all().len());
                self.preview_theme();
            }
            Target::Accounts => {
                self.acc_sel = step_sel(self.acc_sel, d, self.accounts.len());
            }
            Target::Dispatch => {
                self.dispatch_sel = step_sel(self.dispatch_sel, d, self.dispatch_dests().len());
            }
            // a tab bar is one row tall; there is nothing to scroll through
            Target::Tab(_) => {}
        }
    }
}

/// Panes that hold text rather than entries.
fn is_text(pane: Pane) -> bool {
    matches!(pane, Pane::Body | Pane::Log | Pane::DiffBody)
}

fn step_sel(current: usize, d: i64, len: usize) -> usize {
    (current as i64 + d).clamp(0, (len as i64 - 1).max(0)) as usize
}
