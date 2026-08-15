//! What else touches this file.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::diffline::app::App;
use crate::tui::theme;
use crate::tui::{centered_over as centered, frame, put_trunc, rule};

pub(crate) fn deps(buf: &mut Buffer, area: Rect, app: &App) {
    let m = centered(area, 86, 14);
    frame(buf, m, theme::cyan());
    let base = Style::default().bg(theme::panel());
    put_trunc(
        buf,
        m.x + 2,
        m.y + 1,
        m.right() - 2,
        &format!("BLAST RADIUS — {}", app.path()),
        base.fg(theme::cyan()),
    );
    rule(buf, m, m.y + 2, theme::border());

    // Said rather than guessed. An import graph needs a parser per language,
    // which is the trade this program declined for colour and declines again
    // here; claiming to know what depends on a file when nothing has been
    // parsed would be worse than saying so.
    for (n, line) in [
        "No import graph.",
        "",
        "Working it out means a parser per language, which is the",
        "same cost this program declined for syntax colour. Until",
        "there is one, the honest answer is that it does not know.",
        "",
        "What it can tell you is what else this change touches:",
    ]
    .iter()
    .enumerate()
    {
        put_trunc(
            buf,
            m.x + 3,
            m.y + 4 + n as u16,
            m.right() - 2,
            line,
            base.fg(if n == 0 { theme::fg() } else { theme::dimmer() }),
        );
    }
    for (y, f) in (m.y + 11..m.bottom() - 1).zip(app.files.iter().take(2)) {
        put_trunc(
            buf,
            m.x + 5,
            y,
            m.right() - 2,
            &format!("└─ {}", f.path),
            base.fg(theme::fg()),
        );
    }
}
