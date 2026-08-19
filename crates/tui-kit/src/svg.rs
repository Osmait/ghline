//! SVG export of a terminal buffer.
//!
//! Both applications have headless renderers. Keeping the conversion here
//! prevents either renderer from reaching through the other application for a
//! format that belongs to the terminal toolkit.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

use crate::theme;

fn hex(color: Color, fallback: Color) -> String {
    match color {
        Color::Rgb(red, green, blue) => format!("#{red:02x}{green:02x}{blue:02x}"),
        _ => hex(fallback, Color::Rgb(0, 0, 0)),
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Dumps a terminal buffer as background rectangles and text runs.
#[must_use]
pub fn render(buffer: &Buffer, width: u16, height: u16) -> String {
    const CELL_WIDTH: f32 = 9.6;
    const CELL_HEIGHT: f32 = 20.0;
    let svg_width = CELL_WIDTH * f32::from(width);
    let svg_height = CELL_HEIGHT * f32::from(height);

    let ground = theme::bg();
    let ink = theme::fg();
    let ground_hex = hex(ground, ground);

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{svg_width}\" height=\"{svg_height}\" viewBox=\"0 0 {svg_width} {svg_height}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{ground_hex}\"/>\n\
         <g font-family=\"JetBrainsMono Nerd Font, JetBrains Mono, monospace\" font-size=\"16\">\n"
    );

    for y in 0..height {
        let mut x = 0;
        while x < width {
            let background = hex(buffer[(x, y)].style().bg.unwrap_or(ground), ground);
            let start = x;
            while x < width
                && hex(buffer[(x, y)].style().bg.unwrap_or(ground), ground) == background
            {
                x += 1;
            }
            if background != ground_hex {
                svg.push_str(&format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{CELL_HEIGHT}\" fill=\"{background}\"/>\n",
                    f32::from(start) * CELL_WIDTH,
                    f32::from(y) * CELL_HEIGHT,
                    f32::from(x - start) * CELL_WIDTH
                ));
            }
        }
    }

    for y in 0..height {
        let mut x = 0;
        while x < width {
            let foreground = hex(buffer[(x, y)].style().fg.unwrap_or(ink), ink);
            let start = x;
            let mut run = String::new();
            while x < width && hex(buffer[(x, y)].style().fg.unwrap_or(ink), ink) == foreground {
                run.push_str(buffer[(x, y)].symbol());
                x += 1;
            }
            if !run.trim().is_empty() {
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{foreground}\" xml:space=\"preserve\">{}</text>\n",
                    f32::from(start) * CELL_WIDTH,
                    f32::from(y) * CELL_HEIGHT + 15.0,
                    xml_escape(&run)
                ));
            }
        }
    }

    svg.push_str("</g></svg>\n");
    svg
}
