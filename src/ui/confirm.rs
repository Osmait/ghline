//! Confirmation modal for merge, close / reopen and branch deletion.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::overlay::{centered, frame, rule, scrim};
use super::{fill, put, put_right, put_trunc};
use crate::actions::Prompt;
use crate::app::App;
use crate::data::MERGE_METHODS;
use crate::data::ReviewState;
use crate::theme;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App, prompt: &Prompt) {
    // Dispatching is not about a pull request, so it does not fit the layout
    // below and does not need a selection to still be there.
    if let Prompt::Dispatch { who, text, .. } = prompt {
        dispatch(buf, area, who, text);
        return;
    }
    let Some(cur) = app.current() else { return };
    let pr = cur.as_pr();
    scrim(buf, area);

    let is_merge = matches!(prompt, Prompt::Merge(_));
    let (title, accent) = match prompt {
        Prompt::Merge(_) => ("MERGE PULL REQUEST", theme::purple()),
        Prompt::Close => ("CLOSE PULL REQUEST", theme::yellow()),
        Prompt::Reopen => ("REOPEN PULL REQUEST", theme::green()),
        Prompt::DeleteBranch { .. } => ("DELETE BRANCH", theme::red()),
        // drawn above, before the selection is needed
        Prompt::Dispatch { .. } => return,
    };

    let height = if is_merge { 12 } else { 7 };
    let modal = centered(area, 68, height);
    frame(buf, modal, accent);

    let base = Style::default().bg(theme::panel());
    let x = modal.x + 2;
    let max = modal.right() - 2;

    let tx = put(buf, x, modal.y + 1, max, title, base.fg(accent));
    if app.live() {
        let cx = put(buf, tx, modal.y + 1, max, "  ", base);
        put_trunc(
            buf,
            cx,
            modal.y + 1,
            max - 22,
            &app.item_repo_key(),
            base.fg(theme::dimmer()),
        );
    }
    put_right(
        buf,
        max,
        modal.y + 1,
        if is_merge {
            "1/2/3 · enter · esc"
        } else {
            "y confirm · esc cancel"
        },
        base.fg(theme::dimmer()),
    );
    rule(buf, modal, modal.y + 2, accent);

    // the PR reference; branch deletion uses its own, not the selection's
    let num = match prompt {
        Prompt::DeleteBranch { num, .. } => *num,
        _ => cur.num,
    };
    let mut y = modal.y + 3;
    let nx = put(
        buf,
        x,
        y,
        max,
        &format!("#{num}  "),
        base.fg(theme::dimmer()),
    );
    put_trunc(buf, nx, y, max, &cur.title, base.fg(theme::bright()));
    y += 1;

    match prompt {
        Prompt::DeleteBranch { branch, .. } => {
            put_trunc(
                buf,
                x,
                y,
                max,
                &format!("delete {branch} — this cannot be undone"),
                base.fg(theme::body()),
            );
        }
        Prompt::Reopen => {
            put(
                buf,
                x,
                y,
                max,
                &format!("reopen against main from {}", cur.branch()),
                base.fg(theme::body()),
            );
        }
        _ => {
            put(
                buf,
                x,
                y,
                max,
                &format!(
                    "{} → main · {} {} across {} files",
                    cur.branch(),
                    pr.map_or("", |p| p.add.as_str()),
                    pr.map_or("", |p| p.del.as_str()),
                    pr.map_or(0, |p| p.files)
                ),
                base.fg(theme::body()),
            );
        }
    }
    y += 1;

    // check and review state, shown as a warning before you commit
    if is_merge {
        let reviews = pr.map_or(&[][..], |p| p.reviews.as_slice());
        let approvals = reviews
            .iter()
            .filter(|r| r.state == ReviewState::Approved)
            .count();
        let blocking = reviews
            .iter()
            .filter(|r| r.state == ReviewState::ChangesRequested)
            .count();
        let cx = put(
            buf,
            x,
            y,
            max,
            &format!(
                "{} {} checks",
                theme::state_icon(cur.checks()),
                cur.checks()
            ),
            base.fg(theme::state_color(cur.checks())),
        );
        let cx = put(buf, cx, y, max, "  ·  ", base.fg(theme::dimmer()));
        let cx = put(
            buf,
            cx,
            y,
            max,
            &format!("{approvals} approvals"),
            base.fg(if approvals > 0 {
                theme::green()
            } else {
                theme::dimmer()
            }),
        );
        if blocking > 0 {
            let cx = put(buf, cx, y, max, "  ·  ", base.fg(theme::dimmer()));
            put(
                buf,
                cx,
                y,
                max,
                &format!("{blocking} requesting changes"),
                base.fg(theme::red()),
            );
        }
        y += 2;

        if let Prompt::Merge(sel) = prompt {
            let sel = *sel;
            for (i, m) in MERGE_METHODS.iter().enumerate() {
                let row = Rect {
                    x: modal.x + 1,
                    y,
                    width: modal.width - 2,
                    height: 1,
                };
                let bg = if i == sel {
                    theme::sel()
                } else {
                    theme::panel()
                };
                fill(buf, row, bg);
                let s = Style::default().bg(bg);
                if i == sel {
                    put(buf, modal.x + 1, y, max, "▌", s.fg(accent));
                }
                let dot = if i == sel { "●" } else { "○" };
                let cx = put(
                    buf,
                    x,
                    y,
                    max,
                    dot,
                    s.fg(if i == sel { accent } else { theme::dimmer() }),
                );
                let cx = put(buf, cx, y, max, " ", s);
                let cx = put(
                    buf,
                    cx,
                    y,
                    max,
                    &format!("{} ", i + 1),
                    s.fg(theme::dimmer()),
                );
                put_trunc(
                    buf,
                    cx,
                    y,
                    max,
                    m.label(),
                    s.fg(if i == sel {
                        theme::bright()
                    } else {
                        theme::fg()
                    }),
                );
                y += 1;
            }
        }
    }
}

/// "Send this to that agent?", with the first lines of what it would receive.
///
/// The preview is the point. This starts a machine working somewhere outside
/// this program, and the cheapest way to catch a template that renders badly
/// is to show what is about to be sent.
fn dispatch(buf: &mut Buffer, area: Rect, who: &str, text: &str) {
    scrim(buf, area);

    let accent = theme::cyan();
    let width = area.width.saturating_sub(8).min(76);
    let preview: Vec<String> = super::wrap(text, width.saturating_sub(6) as usize)
        .into_iter()
        .take(6)
        .collect();
    let modal = centered(area, width, preview.len() as u16 + 8);
    frame(buf, modal, accent);

    let base = Style::default().bg(theme::panel());
    let x = modal.x + 2;
    let max = modal.right() - 2;

    put(buf, x, modal.y + 1, max, "SEND TO AGENT", base.fg(accent));
    put_right(
        buf,
        max,
        modal.y + 1,
        "y confirm · esc cancel",
        base.fg(theme::dimmer()),
    );
    rule(buf, modal, modal.y + 2, accent);

    let mut y = modal.y + 3;
    put_trunc(buf, x, y, max, who, base.fg(theme::bright()));
    y += 2;
    for line in &preview {
        put_trunc(buf, x + 2, y, max, line, base.fg(theme::dimmer()));
        y += 1;
    }
    rule(buf, modal, modal.bottom() - 2, accent);
    put(
        buf,
        x,
        modal.bottom() - 1,
        max,
        "it starts working immediately",
        base.fg(theme::dimmer()),
    );
}
