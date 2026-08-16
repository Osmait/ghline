//! `--snapshot` mode: renders a state to stdout as ANSI text, with no
//! interactive terminal. Useful to inspect or compare against the design.
//!
//! Printing to stdout *is* this module's job, so the lint against it is off.
#![allow(clippy::print_stdout, reason = "this mode's output is stdout")]

use crate::shared::key::parse_keys;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use crate::github::app::{App, Source};
use crate::github::ui;

// Only the ANSI text render uses this, and that one draws the fixture.
#[cfg(feature = "demo")]
fn ansi(c: Color, fg: bool) -> String {
    let lead = if fg { 38 } else { 48 };
    match c {
        Color::Rgb(r, g, b) => format!("\x1b[{lead};2;{r};{g};{b}m"),
        _ => format!("\x1b[{}m", if fg { 39 } else { 49 }),
    }
}

fn hex(c: Color, fallback: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => hex(fallback, Color::Rgb(0, 0, 0)),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Dumps the buffer as SVG (a layer of background rects plus text runs).
pub fn to_svg(buf: &ratatui::buffer::Buffer, width: u16, height: u16) -> String {
    const CW: f32 = 9.6;
    const CH: f32 = 20.0;
    let w = CW * width as f32;
    let h = CH * height as f32;

    // the ground and the default ink come from whichever theme is active, or
    // the export would always look like the design's
    let ground = crate::tui::theme::bg();
    let ink = crate::tui::theme::fg();
    let ground_hex = hex(ground, ground);

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{ground_hex}\"/>\n\
         <g font-family=\"JetBrainsMono Nerd Font, JetBrains Mono, monospace\" font-size=\"16\">\n"
    );

    // backgrounds: one rect per contiguous run of the same colour
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let bg = hex(buf[(x, y)].style().bg.unwrap_or(ground), ground);
            let start = x;
            while x < width && hex(buf[(x, y)].style().bg.unwrap_or(ground), ground) == bg {
                x += 1;
            }
            if bg != ground_hex {
                svg.push_str(&format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{CH}\" fill=\"{bg}\"/>\n",
                    start as f32 * CW,
                    y as f32 * CH,
                    (x - start) as f32 * CW
                ));
            }
        }
    }

    // text: one <text> per contiguous run of the same foreground colour
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let fg = hex(buf[(x, y)].style().fg.unwrap_or(ink), ink);
            let start = x;
            let mut run = String::new();
            while x < width && hex(buf[(x, y)].style().fg.unwrap_or(ink), ink) == fg {
                run.push_str(buf[(x, y)].symbol());
                x += 1;
            }
            if !run.trim().is_empty() {
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{fg}\" xml:space=\"preserve\">{}</text>\n",
                    start as f32 * CW,
                    y as f32 * CH + 15.0,
                    xml_escape(&run)
                ));
            }
        }
    }

    svg.push_str("</g></svg>\n");
    svg
}

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

#[cfg(feature = "demo")]
/// The demo, with the keys applied and the clock wound on.
///
/// Four callers built this the same way and one of them would eventually have
/// stopped matching: a snapshot is only worth having if the frame is the same
/// on any machine, and that starts with reading nobody's config.
pub fn demo(keys: &str, ticks: usize) -> App {
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));
    let mut app = App::new(Source::Demo, None);
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

#[cfg(feature = "demo")]
/// One frame of the demo as plain text, a row per line and no colour.
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
    let mut app = demo(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    crate::tui::probe::screen(&term)
}

/// Render with real data: applies the keys, waiting on `gh` between each one.
fn build_live(keys: &str, ticks: usize) -> App {
    let mut app = App::new(Source::Live, None);
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

#[cfg(feature = "demo")]
/// Renders the demo with every pane held in its loading state, which is the
/// only way to look at the skeletons without racing the network.
pub fn svg_loading(keys: &str, width: u16, height: u16, frame: u64) {
    // A snapshot has to be the same frame on any machine, so it reads
    // nobody's config.
    let _ =
        crate::shared::settings::use_store(Box::new(crate::shared::settings::Memory::default()));
    let mut app = App::new(Source::Demo, None);
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    app.hold_loading(frame);

    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    print!("{}", to_svg(term.backend().buffer(), width, height));
}

#[cfg(feature = "demo")]
pub fn svg(keys: &str, width: u16, height: u16, ticks: usize) {
    let mut app = demo(keys, ticks);
    let Ok(mut term) = Terminal::new(TestBackend::new(width, height));
    let Ok(_) = term.draw(|f| ui::draw(f, &mut app));
    print!("{}", to_svg(term.backend().buffer(), width, height));
}

#[cfg(feature = "demo")]
pub fn run(keys: &str, width: u16, height: u16, ticks: usize) {
    let mut app = demo(keys, ticks);
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
