//! `--snapshot` mode: renders a state to stdout as ANSI text, with no
//! interactive terminal. Useful to inspect or compare against the design.
//!
//! Printing to stdout *is* this module's job, so the lint against it is off.
#![allow(clippy::print_stdout, reason = "this mode's output is stdout")]

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use crate::github::app::{App, Source};
use crate::github::ui;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// Turns `"jj<enter>k"` into the equivalent key sequence.
pub fn parse_keys(spec: &str) -> Vec<KeyEvent> {
    let mut out = Vec::new();
    let mut rest = spec;
    while !rest.is_empty() {
        if let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
            let name = &rest[1..end + 1];
            let code = match name {
                "enter" => Some(KeyCode::Enter),
                "esc" => Some(KeyCode::Esc),
                "tab" => Some(KeyCode::Tab),
                "bs" => Some(KeyCode::Backspace),
                "down" => Some(KeyCode::Down),
                "up" => Some(KeyCode::Up),
                "left" => Some(KeyCode::Left),
                "right" => Some(KeyCode::Right),
                _ => None,
            };
            if let Some(c) = code {
                out.push(key(c));
                rest = &rest[end + 2..];
                continue;
            }
        }
        let Some(c) = rest.chars().next() else { break };
        out.push(key(KeyCode::Char(c)));
        rest = &rest[c.len_utf8()..];
    }
    out
}

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
    let ground = crate::shared::theme::bg();
    let ink = crate::shared::theme::fg();
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

/// Render with real data: applies the keys, waiting on `gh` between each one.
fn build_live(keys: &str, ticks: usize) -> App {
    let mut app = App::new(Source::Live);
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

pub fn svg_live(keys: &str, width: u16, height: u16, ticks: usize) -> std::io::Result<()> {
    let mut app = build_live(keys, ticks);
    let mut term = Terminal::new(TestBackend::new(width, height))?;
    term.draw(|f| ui::draw(f, &mut app))?;
    print!("{}", to_svg(term.backend().buffer(), width, height));
    Ok(())
}

/// Renders the demo with every pane held in its loading state, which is the
/// only way to look at the skeletons without racing the network.
pub fn svg_loading(keys: &str, width: u16, height: u16, frame: u64) -> std::io::Result<()> {
    let mut app = App::new(Source::Demo);
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    app.hold_loading(frame);

    let mut term = Terminal::new(TestBackend::new(width, height))?;
    term.draw(|f| ui::draw(f, &mut app))?;
    print!("{}", to_svg(term.backend().buffer(), width, height));
    Ok(())
}

pub fn svg(keys: &str, width: u16, height: u16, ticks: usize) -> std::io::Result<()> {
    let mut app = App::new(Source::Demo);
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    for _ in 0..ticks {
        app.tick();
    }
    app.blink = true;

    let mut term = Terminal::new(TestBackend::new(width, height))?;
    term.draw(|f| ui::draw(f, &mut app))?;
    print!("{}", to_svg(term.backend().buffer(), width, height));
    Ok(())
}

pub fn run(keys: &str, width: u16, height: u16, ticks: usize) -> std::io::Result<()> {
    let mut app = App::new(Source::Demo);
    for k in parse_keys(keys) {
        app.on_key(k);
    }
    for _ in 0..ticks {
        app.tick();
    }
    app.blink = true;

    let mut term = Terminal::new(TestBackend::new(width, height))?;
    term.draw(|f| ui::draw(f, &mut app))?;

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
    Ok(())
}
