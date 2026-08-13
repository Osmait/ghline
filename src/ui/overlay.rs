//! Modals: the account picker (`a`) and the keybinding help (`?`).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use super::{clear, fill, hline, put, put_right, put_trunc};
use crate::app::App;
use crate::data::HELP;
use crate::theme;

/// Dims what is underneath, like the design's `background: #0b0e14bb`.
pub fn scrim(buf: &mut Buffer, area: Rect) {
    let shade = |c: Color| match c {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * 0.42) as u8,
            (g as f32 * 0.42) as u8,
            (b as f32 * 0.42) as u8,
        ),
        other => other,
    };
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let s = cell.style();
                let fg = s.fg.unwrap_or(theme::FG);
                let bg = s.bg.unwrap_or(theme::BG);
                cell.set_style(Style::default().fg(shade(fg)).bg(shade(bg)));
            }
        }
    }
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// A single-line frame in the modal's accent colour.
pub fn frame(buf: &mut Buffer, area: Rect, color: Color) {
    clear(buf, area, theme::PANEL);
    let s = Style::default().bg(theme::PANEL).fg(color);
    let top = format!("┌{}┐", "─".repeat(area.width as usize - 2));
    let bottom = format!("└{}┘", "─".repeat(area.width as usize - 2));
    put(buf, area.x, area.y, area.right(), &top, s);
    put(buf, area.x, area.bottom() - 1, area.right(), &bottom, s);
    for y in area.y + 1..area.bottom() - 1 {
        put(buf, area.x, y, area.right(), "│", s);
        put(buf, area.right() - 1, y, area.right(), "│", s);
    }
}

/// The modal's inner horizontal rule.
pub fn rule(buf: &mut Buffer, area: Rect, y: u16, color: Color) {
    hline(buf, area.x + 1, y, area.width - 2, theme::BORDER);
    let s = Style::default().bg(theme::PANEL).fg(color);
    put(buf, area.x, y, area.right(), "├", s);
    put(buf, area.right() - 1, y, area.right(), "┤", s);
    // restore the rule's background to the modal's own
    for x in area.x + 1..area.right() - 1 {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(theme::PANEL);
        }
    }
}

pub fn accounts(buf: &mut Buffer, area: Rect, app: &App) {
    scrim(buf, area);

    let rows = app.accounts.len() as u16 * 2;
    let modal = centered(area, 66, rows + 7);
    frame(buf, modal, theme::CYAN);

    let base = Style::default().bg(theme::PANEL);
    put(
        buf,
        modal.x + 2,
        modal.y + 1,
        modal.right(),
        "SWITCH ACCOUNT",
        base.fg(theme::CYAN),
    );
    put_right(
        buf,
        modal.right() - 2,
        modal.y + 1,
        "j/k · enter · esc",
        base.fg(theme::DIMMER),
    );
    rule(buf, modal, modal.y + 2, theme::CYAN);

    for (i, a) in app.accounts.iter().enumerate() {
        let y = modal.y + 3 + i as u16 * 2;
        let sel = i == app.acc_sel;
        let bg = if sel { theme::SEL } else { theme::PANEL };
        fill(
            buf,
            Rect {
                x: modal.x + 1,
                y,
                width: modal.width - 2,
                height: 2,
            },
            bg,
        );
        let s = Style::default().bg(bg);
        if sel {
            put(buf, modal.x + 1, y, modal.right(), "▌", s.fg(theme::CYAN));
            put(
                buf,
                modal.x + 1,
                y + 1,
                modal.right(),
                "▌",
                s.fg(theme::CYAN),
            );
        }

        let active = i == app.acc;
        put(
            buf,
            modal.x + 3,
            y,
            modal.right(),
            if active { "●" } else { "○" },
            s.fg(if active { theme::GREEN } else { theme::DIMMER }),
        );

        let repos = format!("{} repos", a.repos.len());
        let rx = put_right(buf, modal.right() - 2, y, &repos, s.fg(theme::DIMMER));

        let fg = if active { theme::BRIGHT } else { theme::FG };
        let mut cx = put(buf, modal.x + 5, y, rx - 1, &a.login, s.fg(fg));
        cx = put(buf, cx, y, rx - 1, " ", s);
        put_trunc(buf, cx, y, rx - 1, &a.kind, s.fg(theme::DIMMER));
        put_trunc(
            buf,
            modal.x + 5,
            y + 1,
            modal.right() - 2,
            &a.sub,
            s.fg(theme::DIMMER),
        );
    }

    let foot_y = modal.bottom() - 2;
    rule(buf, modal, foot_y - 1, theme::CYAN);
    put_trunc(
        buf,
        modal.x + 2,
        foot_y,
        modal.right() - 1,
        "gh auth status · hosts: github.com, ghe.acme.dev",
        base.fg(theme::DIMMER),
    );
}

pub fn help(buf: &mut Buffer, area: Rect) {
    scrim(buf, area);

    let per_col = HELP.len().div_ceil(2) as u16;
    let modal = centered(area, 79, per_col + 5);
    frame(buf, modal, theme::YELLOW);

    let base = Style::default().bg(theme::PANEL);
    put(
        buf,
        modal.x + 2,
        modal.y + 1,
        modal.right(),
        "KEYBINDINGS",
        base.fg(theme::YELLOW),
    );
    put_right(
        buf,
        modal.right() - 2,
        modal.y + 1,
        "esc to close",
        base.fg(theme::DIMMER),
    );
    rule(buf, modal, modal.y + 2, theme::YELLOW);

    let col_w = (modal.width - 4) / 2;
    for (i, (k, d)) in HELP.iter().enumerate() {
        let col = (i as u16) / per_col;
        let row = (i as u16) % per_col;
        let x = modal.x + 2 + col * col_w;
        let y = modal.y + 3 + row;
        if y >= modal.bottom() - 1 {
            continue;
        }
        let max = if col == 0 {
            x + col_w
        } else {
            modal.right() - 2
        };
        put_right(buf, x + 13, y, k, base.fg(theme::PURPLE));
        put_trunc(buf, x + 15, y, max, d, base.fg(theme::BODY));
    }
}
