//! Modals: the account picker (`a`) and the keybinding help (`?`).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::github::app::App;
use crate::github::app::hit::{Region, Target};
use crate::github::data::HELP;
use crate::shared::theme;
use crate::tui::Dialog;
use crate::tui::{centered, frame, rule, scrim};
use crate::tui::{fill, put, put_right, put_trunc};

pub fn accounts(buf: &mut Buffer, area: Rect, app: &mut App) {
    scrim(buf, area);

    let rows = app.accounts.len() as u16 * 2;
    let modal = centered(area, 66, rows + 7);
    frame(buf, modal, theme::cyan());

    let base = Style::default().bg(theme::panel());
    put(
        buf,
        modal.x + 2,
        modal.y + 1,
        modal.right(),
        "SWITCH ACCOUNT",
        base.fg(theme::cyan()),
    );
    put_right(
        buf,
        modal.right() - 2,
        modal.y + 1,
        "j/k · enter · esc",
        base.fg(theme::dimmer()),
    );
    rule(buf, modal, modal.y + 2, theme::cyan());

    // The modal as a whole first, so a click on its chrome is absorbed
    // rather than falling through to the pane behind it; the rows go on top.
    app.hits.push(Region::plain(Target::Accounts, modal));
    app.hits.push(Region::rows(
        Target::Accounts,
        Rect {
            x: modal.x + 1,
            y: modal.y + 3,
            width: modal.width - 2,
            height: modal.bottom().saturating_sub(modal.y + 3),
        },
        2,
        0,
        app.accounts.len(),
    ));

    for (i, a) in app.accounts.iter().enumerate() {
        let y = modal.y + 3 + i as u16 * 2;
        let sel = i == app.acc_sel;
        let bg = if sel { theme::sel() } else { theme::panel() };
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
            put(buf, modal.x + 1, y, modal.right(), "▌", s.fg(theme::cyan()));
            put(
                buf,
                modal.x + 1,
                y + 1,
                modal.right(),
                "▌",
                s.fg(theme::cyan()),
            );
        }

        let active = i == app.acc;
        put(
            buf,
            modal.x + 3,
            y,
            modal.right(),
            if active { "●" } else { "○" },
            s.fg(if active {
                theme::green()
            } else {
                theme::dimmer()
            }),
        );

        let repos = format!("{} repos", a.repos.len());
        let rx = put_right(buf, modal.right() - 2, y, &repos, s.fg(theme::dimmer()));

        let fg = if active { theme::bright() } else { theme::fg() };
        let mut cx = put(buf, modal.x + 5, y, rx - 1, &a.login, s.fg(fg));
        cx = put(buf, cx, y, rx - 1, " ", s);
        put_trunc(buf, cx, y, rx - 1, &a.kind, s.fg(theme::dimmer()));
        put_trunc(
            buf,
            modal.x + 5,
            y + 1,
            modal.right() - 2,
            &a.sub,
            s.fg(theme::dimmer()),
        );
    }

    let foot_y = modal.bottom() - 2;
    rule(buf, modal, foot_y - 1, theme::cyan());
    put_trunc(
        buf,
        modal.x + 2,
        foot_y,
        modal.right() - 1,
        "gh auth status · hosts: github.com, ghe.acme.dev",
        base.fg(theme::dimmer()),
    );
}

/// The theme picker. It previews live, so the point of the modal is to be
/// small enough that the interface behind it is what you are judging.
pub fn themes(buf: &mut Buffer, area: Rect, app: &mut App) {
    use crate::shared::theme::Theme;

    let all = Theme::all();
    let body = Dialog::new("THEME")
        .hint("j/k previews · enter · esc")
        .accent(theme::purple())
        .size(60, all.len() as u16 * 2 + 7)
        .open(buf, area);

    // The box as a whole first, so a click on its chrome is absorbed rather
    // than falling through to the pane behind it; the rows go on top.
    app.hits.push(Region::plain(Target::Themes, body.outer));
    app.hits
        .push(Region::rows(Target::Themes, body.inner, 2, 0, all.len()));

    for slot in body.rows(buf, all.len(), 2, app.theme_sel, 0) {
        let Some(t) = all.get(slot.index) else {
            continue;
        };
        let s = slot.style;
        let active = *t == theme::current();
        put(
            buf,
            slot.area.x + 2,
            slot.area.y,
            slot.area.right(),
            if active { "●" } else { "○" },
            s.fg(if active {
                theme::green()
            } else {
                theme::dimmer()
            }),
        );
        let fg = if slot.selected {
            theme::bright()
        } else {
            theme::fg()
        };
        put_trunc(
            buf,
            slot.area.x + 4,
            slot.area.y,
            slot.area.right() - 1,
            t.name(),
            s.fg(fg),
        );
        put_trunc(
            buf,
            slot.area.x + 4,
            slot.area.y + 1,
            slot.area.right() - 1,
            t.about(),
            s.fg(theme::dimmer()),
        );
    }
}

pub fn help(buf: &mut Buffer, area: Rect) {
    scrim(buf, area);

    let per_col = HELP.len().div_ceil(2) as u16;
    let modal = centered(area, 79, per_col + 5);
    frame(buf, modal, theme::yellow());

    let base = Style::default().bg(theme::panel());
    put(
        buf,
        modal.x + 2,
        modal.y + 1,
        modal.right(),
        "KEYBINDINGS",
        base.fg(theme::yellow()),
    );
    put_right(
        buf,
        modal.right() - 2,
        modal.y + 1,
        "esc to close",
        base.fg(theme::dimmer()),
    );
    rule(buf, modal, modal.y + 2, theme::yellow());

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
        put_right(buf, x + 13, y, k, base.fg(theme::purple()));
        put_trunc(buf, x + 15, y, max, d, base.fg(theme::body()));
    }
}
