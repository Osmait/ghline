//! Bottom status bar and the command / search line.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{bold, fill, hline, put, put_right};
use crate::actions::FlashKind;
use crate::app::{App, Cmd, Pane, View};
use crate::data::Kind;
use crate::theme;

pub fn draw(buf: &mut Buffer, area: Rect, app: &App) {
    hline(buf, area.x, area.y, area.width, theme::border());

    let y = area.y + 1;
    fill(
        buf,
        Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        },
        theme::panel(),
    );
    let base = Style::default().bg(theme::panel());

    let kind = app
        .current()
        .map(super::super::data::Item::kind)
        .unwrap_or(Kind::Issue);
    let (mode_label, mode_color) = match app.cmd {
        Some(Cmd::Colon) => ("COMMAND", theme::yellow()),
        Some(Cmd::Slash) => ("SEARCH", theme::yellow()),
        None => match app.view {
            View::Logs => ("LOGS", theme::purple()),
            View::Diff => ("DIFF", theme::green()),
            View::Detail => {
                if kind == Kind::Issue {
                    ("ISSUE", theme::cyan())
                } else {
                    ("PULL", theme::cyan())
                }
            }
            View::List => ("NORMAL", theme::cyan()),
        },
    };
    // the mode colour depends on whether a command line is open
    let mode_color = if app.cmd.is_some() {
        theme::yellow()
    } else {
        mode_color
    };
    // a pending confirmation overrides everything else
    let (mode_label, mode_color) = match app.prompt {
        Some(_) => ("CONFIRM", theme::orange()),
        None => (mode_label, mode_color),
    };

    let x = put(
        buf,
        area.x + 1,
        y,
        area.right(),
        &format!(" {mode_label} "),
        bold(Style::default().bg(mode_color).fg(theme::bg())),
    );

    let on_pr = app.actionable_pr();
    let pr_keys = if on_pr {
        " · d diff · m merge · c close · D branch"
    } else {
        ""
    };
    let hint = match app.pane {
        Pane::Repos => "j/k repo · l content · enter open · a account · : command".to_string(),
        Pane::List => format!("j/k move · h repos · enter open · 1/2/3 tabs · / filter{pr_keys}"),
        Pane::Body => {
            let right = if app.panes().contains(&Pane::Checks) {
                " · l checks"
            } else {
                ""
            };
            format!("j/k scroll · ^d/^u page · h repos{right} · esc back{pr_keys}")
        }
        Pane::Checks => format!("j/k check · enter logs · h body · esc back{pr_keys}"),
        Pane::Tree => "j/k node · o fold · enter/l log · esc back".to_string(),
        Pane::Log => "j/k scroll · f follow · / filter · e first error · h jobs · esc".to_string(),
        Pane::Files => "j/k file · enter/l diff · s split · w ignore ws · esc back".to_string(),
        Pane::DiffBody => {
            "j/k scroll · ^d/^u page · s split · w ignore ws · h files · esc".to_string()
        }
    };

    // the counter reflects the focused pane
    let counter = match app.pane {
        Pane::Repos => format!("{}/{}", app.repo_idx() + 1, app.repos().len()),
        Pane::Checks => format!("{}/{}", app.check + 1, app.jobs().len()),
        Pane::Tree => {
            let tree = app.flat_tree();
            format!("{}/{}", app.tree_sel_idx(tree.len()) + 1, tree.len())
        }
        Pane::Body => format!("+{}", app.detail_scroll),
        Pane::Log => format!("+{}", app.log_scroll),
        Pane::Files => format!("{}/{}", app.file_idx() + 1, app.diff_files().len()),
        Pane::DiffBody => format!("+{}", app.diff_scroll),
        Pane::List => {
            let items = app.visible();
            format!("{}/{}", app.item_idx(items.len()) + 1, items.len())
        }
    };
    let position = format!("{counter}  {}/{}", app.login(), app.repo_name());
    let pos_x = put_right(buf, area.right() - 1, y, &position, base.fg(theme::dim()));

    // the last action's notice replaces the help text while it lasts
    let (text, color) = match &app.flash {
        Some(f) => (
            f.text.as_str(),
            match f.kind {
                FlashKind::Ok => theme::green(),
                FlashKind::Warn => theme::yellow(),
            },
        ),
        None => (hint.as_str(), theme::dimmer()),
    };
    super::put_trunc(buf, x + 2, y, pos_x.saturating_sub(2), text, base.fg(color));

    // ---- command line
    let Some(mode) = app.cmd else { return };
    hline(buf, area.x, y + 1, area.width, theme::border());
    let cy = y + 2;
    fill(
        buf,
        Rect {
            x: area.x,
            y: cy,
            width: area.width,
            height: 1,
        },
        theme::bg(),
    );
    let cb = Style::default().bg(theme::bg());

    let prefix = if mode == Cmd::Colon { ":" } else { "/" };
    let mut cx = put(
        buf,
        area.x + 1,
        cy,
        area.right(),
        prefix,
        cb.fg(theme::yellow()),
    );
    cx = put(
        buf,
        cx,
        cy,
        area.right(),
        &app.cmd_text,
        cb.fg(theme::bright()),
    );
    if app.blink {
        put(buf, cx, cy, area.right(), "█", cb.fg(theme::cyan()));
    }

    let cmd_hint = if mode == Cmd::Colon {
        ":account :issues :prs :actions :logs :help :q"
    } else {
        "enter to keep · esc to clear"
    };
    put_right(buf, area.right() - 1, cy, cmd_hint, cb.fg(theme::dimmer()));
}
