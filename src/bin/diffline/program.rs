//! The `diffline` adapter driven by the shared terminal loop.
//!
//! It owns timers and service draining; process setup and headless rendering
//! remain in `runtime`.

use std::time::{Duration, Instant};

use tui_kit::run::Program;

use diffline_app::app::{App, Modal};
use diffline_app::view as ui;
use diffline_app::watch::{Notice, Watch};

/// Cursor blink, and the beat the toast fades on.
const BLINK: Duration = Duration::from_millis(500);
/// Frame rate of the loading skeletons, fast enough to read as motion.
const ANIM: Duration = Duration::from_millis(110);
/// How often the toast is aged out.
const TICK: Duration = Duration::from_millis(1200);
/// Quiet time after an editor's burst of writes before Git is asked again.
const CHANGE_DEBOUNCE: Duration = Duration::from_millis(200);
/// Upper bound on how long a steady stream of writes may defer a refresh.
const CHANGE_MAX_WAIT: Duration = Duration::from_secs(2);

/// diffline's borrowed state and the timers its terminal runtime advances.
pub(super) struct Diffline<'a> {
    app: &'a mut App,
    blink: Instant,
    anim: Instant,
    tick: Instant,
    watch: Option<Watch>,
    /// `(first_event, last_event)` for the current burst: `last_event` extends
    /// the quiet window while `first_event` caps starvation under continuous
    /// writes.
    changed: Option<(Instant, Instant)>,
    dirty: bool,
}

impl<'a> Diffline<'a> {
    pub(super) fn new(app: &'a mut App, watch: Option<Watch>) -> Self {
        let now = Instant::now();
        Self {
            app,
            blink: now,
            anim: now,
            tick: now,
            watch,
            changed: None,
            dirty: true,
        }
    }

    fn shows_caret(&self) -> bool {
        matches!(
            self.app.modal,
            Some(Modal::Comment | Modal::Finder | Modal::Palette | Modal::History)
        )
    }
}

impl Program for Diffline<'_> {
    fn ensure(&mut self) {
        self.app.ensure();
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>) {
        ui::draw(frame, self.app);
    }

    fn on_key(&mut self, press: line_shared::key::Press) {
        line_shared::log::key(press);
        self.app.on_key(press);
    }

    fn on_mouse(&mut self, mouse: line_shared::key::Mouse) {
        line_shared::log::mouse(mouse);
        self.app.on_mouse(mouse);
    }

    fn drain(&mut self) {
        while let Some(response) = self.app.poll() {
            self.app.apply(response);
            self.dirty = true;
        }

        let notice = self.watch.as_ref().map(Watch::poll);
        match notice {
            Some(Notice::Changed) => {
                let now = Instant::now();
                self.changed = Some(match self.changed {
                    Some((first, _)) => (first, now),
                    None => (now, now),
                });
            }
            Some(Notice::Failed(error)) => {
                self.app.flash(format!("file watch stopped: {error}"));
                self.watch = None;
                self.dirty = true;
            }
            Some(Notice::Gone) => {
                self.app.flash("file watch stopped");
                self.watch = None;
                self.dirty = true;
            }
            Some(Notice::Quiet) | None => {}
        }
    }

    /// The soonest of the active timers: an idle program with no open input
    /// cursor, toast or pending work sleeps without waking to redraw.
    fn next_wake(&self) -> Duration {
        (if self.shows_caret() {
            BLINK.saturating_sub(self.blink.elapsed())
        } else {
            Duration::MAX
        })
        .min(if self.app.toast_ttl > 0 {
            TICK.saturating_sub(self.tick.elapsed())
        } else {
            Duration::MAX
        })
        .min(self.changed.map_or(Duration::MAX, |(first, last)| {
            CHANGE_DEBOUNCE
                .saturating_sub(last.elapsed())
                .min(CHANGE_MAX_WAIT.saturating_sub(first.elapsed()))
        }))
        .min(if self.app.waiting() {
            ANIM.saturating_sub(self.anim.elapsed())
        } else {
            Duration::MAX
        })
    }

    fn on_wake(&mut self) {
        if self.changed.is_some_and(|(first, last)| {
            last.elapsed() >= CHANGE_DEBOUNCE || first.elapsed() >= CHANGE_MAX_WAIT
        }) {
            self.changed = None;
            self.app.refresh_live();
            self.dirty = true;
        }
        if self.app.waiting() && self.anim.elapsed() >= ANIM {
            self.app.anim = self.app.anim.wrapping_add(1);
            self.anim = Instant::now();
            self.dirty = true;
        }
        if self.shows_caret() && self.blink.elapsed() >= BLINK {
            self.app.blink = !self.app.blink;
            self.blink = Instant::now();
            self.dirty = true;
        }
        if self.app.toast_ttl > 0 && self.tick.elapsed() >= TICK {
            self.app.tick();
            self.tick = Instant::now();
            self.dirty = true;
        }
    }

    fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.app.wants_redraw)
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}
