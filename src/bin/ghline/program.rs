//! The `ghline` adapter driven by the shared terminal loop.
//!
//! It owns timers, service draining and terminal handovers; process setup and
//! command dispatch remain in `runtime`.

use std::io;
use std::time::{Duration, Instant};

use tui_kit::run::{Handover, Program};

use ghline_app::app::App;
use ghline_app::service::{Request, Response, Service};
use ghline_app::ui;
use line_shared::worker::Worker;

/// Heartbeat for the log stream (the design's 1400 ms `setInterval`).
const TICK: Duration = Duration::from_millis(1400);
/// Command-line cursor blink (`@keyframes om-blink`).
const BLINK: Duration = Duration::from_millis(500);
/// Frame rate of the loading skeletons, fast enough to read as motion.
const ANIM: Duration = Duration::from_millis(110);
/// How long typing has to pause before the finder asks GitHub. Long enough
/// that a word is one request, short enough not to feel deliberate.
const FIND: Duration = Duration::from_millis(260);

/// ghline's state and the timers its terminal runtime advances.
pub(super) struct Ghline {
    app: App,
    tick: Instant,
    blink: Instant,
    anim: Instant,
    find: Instant,
}

impl Ghline {
    pub(super) fn new() -> Self {
        let now = Instant::now();
        let service: Option<Box<dyn Worker<Request, Response>>> = Some(Box::new(Service::spawn()));
        Self {
            app: App::new(service),
            tick: now,
            blink: now,
            anim: now,
            find: now,
        }
    }
}

impl Program for Ghline {
    fn ensure(&mut self) {
        self.app.ensure();
    }

    fn draw(&mut self, frame: &mut ratatui::Frame<'_>) {
        ui::draw(frame, &mut self.app);
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
        while let Some(response) = self.app.poll_service() {
            self.app.apply(response);
        }
    }

    /// The soonest of four, two of them conditional: skeletons only travel
    /// while something is on its way, and the finder's debounce only matters
    /// while the finder is open.
    fn next_wake(&self) -> Duration {
        TICK.saturating_sub(self.tick.elapsed())
            .min(BLINK.saturating_sub(self.blink.elapsed()))
            .min(if self.app.waiting() {
                ANIM.saturating_sub(self.anim.elapsed())
            } else {
                Duration::MAX
            })
            .min(if self.app.finder_open {
                FIND.saturating_sub(self.find.elapsed())
            } else {
                Duration::MAX
            })
    }

    fn on_wake(&mut self) {
        if self.tick.elapsed() >= TICK {
            self.app.tick();
            self.tick = Instant::now();
        }
        if self.find.elapsed() >= FIND {
            self.app.finder_tick();
            self.find = Instant::now();
        }
        if self.app.waiting() && self.anim.elapsed() >= ANIM {
            self.app.anim = self.app.anim.wrapping_add(1);
            self.anim = Instant::now();
        }
        if self.blink.elapsed() >= BLINK {
            self.app.blink = !self.app.blink;
            self.blink = Instant::now();
        }
    }

    fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.app.wants_redraw)
    }

    /// The editor wants the whole terminal, so the runtime gives it back
    /// between frames rather than in the middle of one.
    fn take_handover(&mut self) -> Option<Handover> {
        let (path, line) = self.app.edit_request.take()?;
        Some(Box::new(move || edit(&path, line)))
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}

/// Runs the reader's editor on a file, at a line.
///
/// `$VISUAL` then `$EDITOR` then nvim then vim: the first two are what the
/// reader has said they want, and the last two are what is most likely to be
/// there when they have said nothing.
fn edit(path: &std::path::Path, line: usize) -> io::Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if which("nvim") {
                "nvim".into()
            } else {
                "vim".into()
            }
        });

    // `+N` is understood by vi, vim, nvim, emacs, nano and kak alike. An
    // editor that does not know it is handed a file it can still open.
    let mut parts = editor.split_whitespace();
    let Some(bin) = parts.next() else {
        return Ok(());
    };
    std::process::Command::new(bin)
        .args(parts)
        .arg(format!("+{line}"))
        .arg(path)
        .status()
        .map(|_| ())
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| directory.join(bin).is_file())
    })
}
