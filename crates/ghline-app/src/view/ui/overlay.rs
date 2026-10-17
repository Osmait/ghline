//! Modals: the account picker (`a`), the theme picker (`t`), the commit
//! picker (`v` in a diff), the note editor (`i` in a diff), the review
//! dialog (`R`) and the keybinding help (`?`).

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::style::Style;

use crate::app::App;
use crate::app::hit::{Region, Target};
use crate::data::HELP;
use crate::tui::Dialog;
use crate::tui::theme;
use crate::tui::{centered, frame, rule, scrim};
use crate::tui::{fill, put, put_right, put_trunc};

pub fn accounts(buf: &mut Buffer, area: Rect, app: &mut App) {
    scrim(buf, area);

    let rows = app.accounts.len() as u16 * 2;
    let modal = centered(area, Size::new(66, rows + 7));
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
    use crate::tui::theme::Theme;

    let all = Theme::all();
    let body = Dialog::new("THEME")
        .hint("j/k previews · enter · esc")
        .accent(theme::purple())
        .size(Size::new(60, all.len() as u16 * 2 + 7))
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

/// The commit picker: every change the pull request makes, then each of its
/// commits on its own, oldest first as they were made.
pub fn commits(buf: &mut Buffer, area: Rect, app: &mut App) {
    let commits = app.commits().to_vec();
    let rows = commits.len() + 1;
    // as tall as the list, but never past the screen: the rows scroll
    let tall = (rows as u16 * 2 + 7).min(area.height.saturating_sub(2));
    let body = Dialog::new("COMMITS")
        .hint("j/k · enter · esc")
        .accent(theme::yellow())
        .size(Size::new(76, tall))
        .open(buf, area);

    let fit = (body.inner.height / 2).max(1) as usize;
    let scroll = app.commit_sel.saturating_sub(fit.saturating_sub(1));
    app.hits.push(Region::plain(Target::Commits, body.outer));
    app.hits
        .push(Region::rows(Target::Commits, body.inner, 2, scroll, rows));

    let shown = app.diff_commit.clone();
    for slot in body.rows(buf, rows, 2, app.commit_sel, scroll) {
        let s = slot.style;
        let (on, first, second) = match slot.index.checked_sub(1).and_then(|i| commits.get(i)) {
            None => (
                shown.is_none(),
                "All changes".to_string(),
                format!("{} commits, as one diff", commits.len()),
            ),
            Some(c) => (
                shown.as_deref() == Some(c.sha.as_str()),
                format!("{}  {}", c.short(), c.headline),
                format!("{} · {}", c.author, c.when),
            ),
        };
        put(
            buf,
            slot.area.x + 2,
            slot.area.y,
            slot.area.right(),
            if on { "●" } else { "○" },
            s.fg(if on { theme::green() } else { theme::dimmer() }),
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
            &first,
            s.fg(fg),
        );
        put_trunc(
            buf,
            slot.area.x + 4,
            slot.area.y + 1,
            slot.area.right() - 1,
            &second,
            s.fg(theme::dimmer()),
        );
    }
}

/// The note editor: where the note goes, then what it says so far, folded to
/// the box and ending in the cursor.
pub fn note(buf: &mut Buffer, area: Rect, app: &mut App) {
    let Some(at) = app.note_open.clone() else {
        return;
    };
    let width: u16 = 76;
    let text_w = width.saturating_sub(6) as usize;
    let mut lines: Vec<String> = Vec::new();
    for para in app.note_text.split('\n') {
        let folded = crate::tui::wrap(para, text_w);
        if folded.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(folded);
        }
    }
    let tall = (lines.len() as u16 + 8).clamp(10, area.height.saturating_sub(2));
    let body = Dialog::new("NOTE")
        .hint("enter keep · ^j new line · esc")
        .accent(theme::orange())
        .size(Size::new(width, tall))
        .open(buf, area);
    app.hits.push(Region::plain(Target::Note, body.outer));

    let base = Style::default().bg(theme::panel());
    let side = match at.side {
        crate::data::DiffSide::Left => "  · the old side",
        crate::data::DiffSide::Right => "",
    };
    let x = body.inner.x + 2;
    let max = body.inner.right().saturating_sub(1);
    put_trunc(
        buf,
        x,
        body.inner.y,
        max,
        &format!("{}:{}{side}", at.path, at.line),
        base.fg(theme::dimmer()),
    );
    // the last lines, if there are more than the box holds: the end is where
    // the typing is
    let room = body.inner.height.saturating_sub(2) as usize;
    let skip = lines.len().saturating_sub(room);
    let last = lines.len().saturating_sub(1);
    for (y, (i, line)) in (body.inner.y + 2..).zip(lines.iter().enumerate().skip(skip)) {
        let cx = put_trunc(buf, x, y, max, line, base.fg(theme::bright()));
        if i == last {
            put(buf, cx, y, max, "█", base.fg(theme::orange()));
        }
    }
}

/// The review dialog: what goes with it, the verdict, and the body — and,
/// while it cannot be sent, why not, so enter is never a guess.
pub fn review(buf: &mut Buffer, area: Rect, app: &mut App) {
    use crate::data::Verdict;
    let width: u16 = 76;
    let text_w = width.saturating_sub(6) as usize;
    let mut lines: Vec<String> = Vec::new();
    for para in app.review_text.split('\n') {
        let folded = crate::tui::wrap(para, text_w);
        if folded.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(folded);
        }
    }
    let tall = (lines.len() as u16 + 14).clamp(16, area.height.saturating_sub(2));
    let body = Dialog::new("SEND REVIEW")
        .hint("tab verdict · enter send · ^j new line · esc")
        .accent(theme::green())
        .size(Size::new(width, tall))
        .open(buf, area);
    app.hits.push(Region::plain(Target::Note, body.outer));

    let base = Style::default().bg(theme::panel());
    let x = body.inner.x + 2;
    let max = body.inner.right().saturating_sub(1);
    let mut y = body.inner.y;

    let drafts = app.drafts();
    let files = {
        let mut f: Vec<&str> = drafts.iter().map(|d| d.path.as_str()).collect();
        f.sort_unstable();
        f.dedup();
        f.len()
    };
    let summary = match (drafts.len(), files) {
        (0, _) => "no notes: the review is its verdict and its words".to_string(),
        (1, _) => "1 note".to_string(),
        (n, 1) => format!("{n} notes on 1 file"),
        (n, f) => format!("{n} notes on {f} files"),
    };
    put_trunc(buf, x, y, max, &summary, base.fg(theme::dim()));
    y += 2;

    app.hits.push(Region::rows(
        Target::Review,
        Rect {
            x: body.inner.x,
            y,
            width: body.inner.width,
            height: Verdict::ALL.len() as u16,
        },
        1,
        0,
        Verdict::ALL.len(),
    ));
    for v in Verdict::ALL {
        let on = v == app.review_verdict;
        let color = match v {
            Verdict::Comment => theme::cyan(),
            Verdict::Approve => theme::green(),
            Verdict::RequestChanges => theme::red(),
        };
        let cx = put(buf, x, y, max, if on { "● " } else { "○ " }, base.fg(color));
        let style = if on {
            crate::tui::bold(base.fg(theme::bright()))
        } else {
            base.fg(theme::dim())
        };
        put_trunc(buf, cx, y, max, v.label(), style);
        y += 1;
    }
    y += 1;

    // the body, its last lines if it outgrows the box: the end is where the
    // typing is
    let room = body.inner.bottom().saturating_sub(y + 2) as usize;
    let skip = lines.len().saturating_sub(room.max(1));
    let last = lines.len().saturating_sub(1);
    for (ly, (i, line)) in (y..).zip(lines.iter().enumerate().skip(skip)) {
        let cx = put_trunc(buf, x, ly, max, line, base.fg(theme::bright()));
        if i == last {
            put(buf, cx, ly, max, "█", base.fg(theme::green()));
        }
    }

    if let Some(why) = app.review_refusal() {
        put_trunc(
            buf,
            x,
            body.inner.bottom().saturating_sub(1),
            max,
            why,
            base.fg(theme::orange()),
        );
    }
}

pub fn help(buf: &mut Buffer, area: Rect, app: &mut App) {
    scrim(buf, area);

    let per_col = HELP.len().div_ceil(2) as u16;
    let modal = centered(area, Size::new(79, per_col + 5));
    frame(buf, modal, theme::yellow());
    // a click inside the help is a click on the help, not a click away
    app.hits.push(Region::plain(Target::Help, modal));

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
