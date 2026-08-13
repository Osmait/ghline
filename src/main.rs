//! gh-tui — a GitHub TUI built from the `GitHub TUI.dc.html` design.

mod actions;
mod app;
mod data;
mod demo;
mod demo_diffs;
mod error;
mod gh;
mod service;
mod snapshot;
mod theme;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, Source};

/// Heartbeat for the log stream (the design's 1400 ms `setInterval`).
const TICK: Duration = Duration::from_millis(1400);
/// Command-line cursor blink (`@keyframes om-blink`).
const BLINK: Duration = Duration::from_millis(500);

fn main() -> io::Result<()> {
    // `--snapshot [keys] [width] [height] [ticks]` prints one render and exits.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    if mode == "--snapshot" || mode == "--svg" || mode == "--svg-live" {
        let keys = args.get(1).cloned().unwrap_or_default();
        let w = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(160);
        let h = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(44);
        let ticks = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        return match mode {
            "--svg" => snapshot::svg(&keys, w, h, ticks),
            "--svg-live" => snapshot::svg_live(&keys, w, h, ticks),
            _ => snapshot::run(&keys, w, h, ticks),
        };
    }

    // `--demo` forces the design's data; with no signed-in `gh` we fall back to
    // demo anyway rather than starting an empty interface.
    let source = if args.iter().any(|a| a == "--demo") {
        Source::Demo
    } else if gh::available() {
        Source::Live
    } else {
        eprintln!("gh is unavailable or not signed in; starting in demo mode.");
        eprintln!("run `gh auth login` to use real data.");
        Source::Demo
    };

    let mut term = TerminalGuard::enter()?;
    let res = run(term.inner(), source);
    // the guard restores the terminal even if `run` returns an error
    drop(term);

    if let Err(e) = &res {
        eprintln!("gh-tui: {e}");
    }
    res
}

/// Holds the terminal in the alternate screen while alive and restores it on
/// drop, even if the thread panics. Without it a panic would leave the console
/// in raw mode with no echo.
struct TerminalGuard {
    term: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        if let Err(e) = execute!(out, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        // a panic only skips the guard's Drop when it aborts; on unwind the
        // hook leaves the terminal usable before the message is printed
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));

        let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        term.hide_cursor()?;
        Ok(Self { term })
    }

    fn inner(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.term
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
        let _ = self.term.show_cursor();
    }
}

/// Puts the terminal back to normal. Idempotent and infallible: it is called
/// from the panic hook, where there is nobody to return an error to.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn run(term: &mut Terminal<CrosstermBackend<io::Stdout>>, source: Source) -> io::Result<()> {
    let mut app = App::new(source);
    let mut last_tick = Instant::now();
    let mut last_blink = Instant::now();

    loop {
        // request whatever the current view needs (non-blocking: it goes to the gh thread)
        app.ensure();
        term.draw(|f| ui::draw(f, &mut app))?;

        // a short wait while requests are in flight, so the response is drawn
        // as soon as it arrives
        let waiting = app.waiting();
        let timeout = TICK
            .saturating_sub(last_tick.elapsed())
            .min(BLINK.saturating_sub(last_blink.elapsed()))
            .min(if waiting {
                Duration::from_millis(60)
            } else {
                Duration::MAX
            })
            .max(Duration::from_millis(16));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        while let Some(res) = app.poll_service() {
            app.apply(res);
        }

        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }
        if last_blink.elapsed() >= BLINK {
            app.blink = !app.blink;
            last_blink = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
