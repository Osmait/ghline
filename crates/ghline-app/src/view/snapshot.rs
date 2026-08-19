//! `--snapshot` mode: renders a state to stdout as ANSI text, with no
//! interactive terminal. Useful to inspect or compare against the design.
//!
//! Printing to stdout *is* this module's job, so the lint against it is off.
#![allow(clippy::print_stdout, reason = "this mode's output is stdout")]

use crate::shared::key::parse_keys;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use crate::app::App;
use crate::fixture;
use crate::ui;

// Only the ANSI text render uses this.
fn ansi(c: Color, fg: bool) -> String {
    let lead = if fg { 38 } else { 48 };
    match c {
        Color::Rgb(r, g, b) => format!("\x1b[{lead};2;{r};{g};{b}m"),
        _ => format!("\x1b[{}m", if fg { 39 } else { 49 }),
    }
}

pub use crate::tui::svg::render as to_svg;

/// Lets the `gh` thread finish whatever is pending, with a time limit.
fn settle(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        app.ensure();
        // the finder is driven by a beat in the main loop, which is not
        // running here
        app.finder_tick();
        while let Some(res) = app.poll_service() {
            app.apply(res);
        }
        if !app.waiting() || std::time::Instant::now() > deadline {
            // one more pass in case `ensure` chained another request
            app.ensure();
            if !app.waiting() || std::time::Instant::now() > deadline {
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The fixture, with the keys applied and the clock wound on.
///
/// Four callers built this the same way and one of them would eventually have
/// stopped matching: a snapshot is only worth having if the frame is the same
/// on any machine, and that starts with reading nobody's config.
pub fn seeded(keys: &str, ticks: usize) -> App {
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));
    let mut app = fixture::app();
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    for _ in 0..ticks {
        app.tick();
    }
    // A blinking cursor is half the frames it is in; this is the half that is
    // worth looking at.
    app.blink = true;
    app
}

/// One frame of the fixture as plain text, a row per line and no colour.
///
/// What the golden tests in `tests/` compare against. The SVG below is the
/// same frame for a human to look at; this is the one a diff can be read on.
///
/// No `Result`, because there is nothing here that can fail. ratatui 0.30 gave
/// `Backend` an associated error type and `TestBackend`'s is `Infallible`:
/// drawing into a buffer in memory has nowhere to go wrong, and the type says
/// so now. The `io::Result` this used to return described a failure that could
/// not happen, which is the kind of signature CODE-STYLE is against.
///
/// `let Ok(…) =` with no `else` is how an uninhabited error is discharged —
/// the pattern is irrefutable precisely because the other variant cannot be
/// constructed, so it costs nothing and panics nowhere.
pub fn frame(keys: &str, width: u16, height: u16, ticks: usize) -> String {
    let mut app = seeded(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    crate::tui::probe::screen(&term)
}

/// Render with real data: applies the keys, waiting on `gh` between each one.
///
/// The worker is the point. This used to hand `App::new` a `None` and rely on
/// a `Source::Live` that only said what the data was *meant* to be, so every
/// request was dropped on the floor and `settle` spun until its 45-second
/// deadline and drew skeletons. Asking for a thread is what makes it live.
fn build_live(keys: &str, ticks: usize) -> App {
    let mut app = App::new(Some(Box::new(crate::service::Service::spawn())));
    settle(&mut app);
    for k in parse_keys(keys) {
        app.on_key(k);
        settle(&mut app);
    }
    for _ in 0..ticks {
        app.tick();
    }
    app.blink = true;
    app
}

pub fn svg_live(keys: &str, width: u16, height: u16, ticks: usize) {
    // A snapshot has to be the same frame on any machine, so it reads
    // nobody's config.
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));
    let mut app = build_live(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    print!("{}", to_svg(term.backend().buffer(), width, height));
}

/// Renders the fixture with every pane held in its loading state, which is the
/// only way to look at the skeletons without racing the network.
pub fn svg_loading(keys: &str, width: u16, height: u16, frame: u64) {
    // A snapshot has to be the same frame on any machine, so it reads
    // nobody's config.
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));
    let mut app = fixture::app();
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    app.hold_loading(frame);

    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    print!("{}", to_svg(term.backend().buffer(), width, height));
}

pub fn svg(keys: &str, width: u16, height: u16, ticks: usize) {
    let mut app = seeded(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    print!("{}", to_svg(term.backend().buffer(), width, height));
}

pub fn run(keys: &str, width: u16, height: u16, ticks: usize) {
    let mut app = seeded(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));

    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            let s = cell.style();
            out.push_str(&ansi(s.fg.unwrap_or(Color::Reset), true));
            out.push_str(&ansi(s.bg.unwrap_or(Color::Reset), false));
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m\n");
    }
    print!("{out}");
}
