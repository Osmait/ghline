//! Writing a note.

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::style::Style;

use crate::diffline::app::App;
use crate::tui::theme;
use crate::tui::{centered_over as centered, fill, frame, put, put_right, put_trunc, rule};

pub(crate) fn comment(buf: &mut Buffer, area: Rect, app: &App) {
    let anchors = app.selected_anchors();
    let m = centered(area, Size::new(72, 9));
    frame(buf, m, theme::yellow());
    let base = Style::default().bg(theme::panel());

    let head = Rect {
        x: m.x + 1,
        y: m.y + 1,
        width: m.width - 2,
        height: 1,
    };
    fill(buf, head, theme::yellow());
    let hs = Style::default().bg(theme::yellow()).fg(theme::panel());
    put(buf, head.x + 1, head.y, head.right(), "COMMENT", hs);
    let where_ = match (anchors.first(), anchors.last()) {
        (Some(a), Some(b)) if a.line == b.line => format!("{}:{}", short_path(&a.path), a.line),
        (Some(a), Some(b)) => format!(
            "{}:{}-{}  ({} lines)",
            short_path(&a.path),
            a.line.min(b.line),
            a.line.max(b.line),
            anchors.len()
        ),
        _ => "—".into(),
    };
    put_right(buf, head.right() - 1, head.y, &where_, hs);

    let snippet = app
        .diff_rows()
        .get(app.span().0)
        .map(|r| r.text.trim())
        .unwrap_or("");
    put_trunc(
        buf,
        m.x + 3,
        m.y + 3,
        m.right() - 2,
        snippet,
        base.fg(theme::dimmer()),
    );
    rule(buf, m, m.y + 4, theme::border());

    let x = put(
        buf,
        m.x + 2,
        m.y + 5,
        m.right() - 2,
        "❯ ",
        base.fg(theme::yellow()),
    );
    if app.draft.is_empty() {
        put_trunc(
            buf,
            x,
            m.y + 5,
            m.right() - 2,
            "what should the agent do here?",
            base.fg(theme::dimmer()),
        );
    } else {
        let end = put_trunc(
            buf,
            x,
            m.y + 5,
            m.right() - 2,
            &app.draft,
            base.fg(theme::bright()),
        );
        if app.blink {
            put(
                buf,
                end,
                m.y + 5,
                m.right() - 2,
                "█",
                base.fg(theme::yellow()),
            );
        }
    }

    rule(buf, m, m.bottom() - 3, theme::border());
    put(
        buf,
        m.x + 2,
        m.bottom() - 2,
        m.right() - 2,
        "↵ save to queue · esc discard",
        base.fg(theme::dimmer()),
    );
}

fn short_path(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}
