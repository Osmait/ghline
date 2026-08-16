//! The `diffline` adapter driven by the shared terminal loop.
//!
//! It owns timers and service draining; process setup and headless rendering
//! remain in `runtime`.

use std::time::{Duration, Instant};

use github_tui::tui::run::Program;

use github_tui::diffline::app::App;
use github_tui::diffline::view as ui;

/// Cursor blink, and the beat the toast fades on.
const BLINK: Duration = Duration::from_millis(500);
/// Frame rate of the loading skeletons, fast enough to read as motion.
const ANIM: Duration = Duration::from_millis(110);
/// How often the toast is aged out.
const TICK: Duration = Duration::from_millis(1200);

/// diffline's borrowed state and the timers its terminal runtime advances.
pub(super) struct Diffline<'a> {
    app: &'a mut App,
    blink: Instant,
    anim: Instant,
    tick: Instant,
}

impl<'a> Diffline<'a> {
    pub(super) fn new(app: &'a mut App) -> Self {
        let now = Instant::now();
        Self {
            app,
            blink: now,
            anim: now,
            tick: now,
        }
    }
}

impl Program for Diffline<'_> {
    fn ensure(&mut self) {
        self.app.ensure();
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>) {
        ui::draw(frame, self.app);
    }

    fn on_key(&mut self, press: github_tui::shared::key::Press) {
        self.app.on_key(press);
    }

    fn on_mouse(&mut self, mouse: github_tui::shared::key::Mouse) {
        self.app.on_mouse(mouse);
    }

    fn drain(&mut self) {
        while let Some(response) = self.app.poll() {
            self.app.apply(response);
        }
    }

    /// The soonest of the three, and only the skeletons when something is
    /// actually on its way: an idle program has nothing to animate.
    fn next_wake(&self) -> Duration {
        BLINK
            .saturating_sub(self.blink.elapsed())
            .min(TICK.saturating_sub(self.tick.elapsed()))
            .min(if self.app.waiting() {
                ANIM.saturating_sub(self.anim.elapsed())
            } else {
                Duration::MAX
            })
    }

    fn on_wake(&mut self) {
        if self.app.waiting() && self.anim.elapsed() >= ANIM {
            self.app.anim = self.app.anim.wrapping_add(1);
            self.anim = Instant::now();
        }
        if self.blink.elapsed() >= BLINK {
            self.app.blink = !self.app.blink;
            self.blink = Instant::now();
        }
        if self.tick.elapsed() >= TICK {
            self.app.tick();
            self.tick = Instant::now();
        }
    }

    fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.app.wants_redraw)
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}
