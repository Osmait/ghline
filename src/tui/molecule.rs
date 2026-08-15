//! Molecules: one recognisable piece, made of atoms.
//!
//! A rule, a frame, a modal's title row, a query line, the message a pane
//! shows when it has nothing to show. Each is a thing you could point at on
//! screen and name, and each is several atoms with the arithmetic between them
//! already worked out.
//!
//! The rule: a molecule owns a small `Rect`, calls atoms and geometry, and
//! never calls an organism. It takes data — never an app — which is what lets
//! both programs use the same one.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::atom::{clear, hline, put, put_right, put_trunc, skel_bar};
use super::geom::pct;
use super::theme;
use crate::shared::mux::AgentStatus;

/// A single-line frame in the modal's accent colour.
///
/// `saturating_sub` throughout: one copy of this had a plain subtraction and
/// was saved only by a guard elsewhere refusing to draw below forty columns.
/// A drawing primitive should not depend on somebody else's guard.
pub fn frame(buf: &mut Buffer, area: Rect, color: Color) {
    clear(buf, area, theme::panel());
    let s = Style::default().bg(theme::panel()).fg(color);
    let run = "─".repeat(area.width.saturating_sub(2) as usize);
    put(buf, area.x, area.y, area.right(), &format!("┌{run}┐"), s);
    put(
        buf,
        area.x,
        area.bottom().saturating_sub(1),
        area.right(),
        &format!("└{run}┘"),
        s,
    );
    for y in area.y + 1..area.bottom().saturating_sub(1) {
        put(buf, area.x, y, area.right(), "│", s);
        put(buf, area.right().saturating_sub(1), y, area.right(), "│", s);
    }
}

/// The modal's inner horizontal rule.
///
/// With the `├` and `┤` that join it to the frame — diffline's copy drew a
/// plain line and left a gap at both ends, which is what you get when a fix
/// lands on one of two copies.
pub fn rule(buf: &mut Buffer, area: Rect, y: u16, color: Color) {
    hline(
        buf,
        area.x + 1,
        y,
        area.width.saturating_sub(2),
        theme::border(),
    );
    let s = Style::default().bg(theme::panel()).fg(color);
    put(buf, area.x, y, area.right(), "├", s);
    put(buf, area.right().saturating_sub(1), y, area.right(), "┤", s);
    // the rule's own background is the modal's, not the pane's underneath
    for x in area.x + 1..area.right().saturating_sub(1) {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(theme::panel());
        }
    }
}

/// The heading every modal starts with: a frame, a title, a hint on the
/// right, and the rule under both.
///
/// Returns the first row of the body, so a caller never counts the chrome.
pub fn modal_head(buf: &mut Buffer, m: Rect, title: &str, hint: &str, accent: Color) -> u16 {
    frame(buf, m, accent);
    let base = Style::default().bg(theme::panel());
    put(buf, m.x + 2, m.y + 1, m.right() - 2, title, base.fg(accent));
    if !hint.is_empty() {
        put_right(buf, m.right() - 2, m.y + 1, hint, base.fg(theme::dimmer()));
    }
    rule(buf, m, m.y + 2, accent);
    m.y + 3
}

/// What a query line is made of.
///
/// A struct rather than five more parameters. Clippy drew the line at seven
/// arguments and it was right to: a call site reading `(…, "❯ ", "find…",
/// true, cyan)` says nothing about which of those is the placeholder, and this
/// is the shape every component below will want as it grows.
#[derive(Clone, Copy)]
pub struct Query<'a> {
    /// What has been typed so far.
    pub text: &'a str,
    /// The prompt in front of it — `"❯ "` in both programs.
    pub lead: &'a str,
    /// Shown instead of the text while it is empty, and never alongside it.
    pub placeholder: &'a str,
    /// Whether the caret is in its visible half. The blink belongs to the
    /// program's clock, not to this.
    pub caret: bool,
    pub accent: Color,
}

/// The line a searching modal types into.
///
/// Takes the query rather than the app that holds it — the only thing it ever
/// wanted from one, and the reason this could not be shared before.
/// `accent` is a field and not `theme::yellow()` because the two programs
/// do not share an accent: diffline's modals are yellow and github-tui's are
/// cyan, and that is a decision about the program rather than about a query
/// line. It was hardcoded while only diffline called this, which is exactly
/// how github-tui's finder ended up with its own copy of these twenty lines.
pub fn query_line(buf: &mut Buffer, m: Rect, y: u16, q: &Query<'_>) {
    let Query {
        text: query,
        lead,
        placeholder,
        caret,
        accent,
    } = *q;
    let base = Style::default().bg(theme::panel());
    let x = put(buf, m.x + 2, y, m.right() - 2, lead, base.fg(accent));
    if query.is_empty() {
        put_trunc(
            buf,
            x,
            y,
            m.right() - 2,
            placeholder,
            base.fg(theme::dimmer()),
        );
        return;
    }
    let end = put_trunc(buf, x, y, m.right() - 2, query, base.fg(theme::bright()));
    if caret {
        // `█` and not `▌`: the half block is the selection mark on a row —
        // both programs use it that way in eleven places — and the full block
        // is the text caret, in four of the five places one is drawn. This
        // was the fifth.
        put(buf, end, y, m.right() - 2, "█", base.fg(accent));
    }
}

/// How an agent's state looks: a glyph and a colour.
///
/// One answer, where there were two. github-tui drew a distinct glyph per
/// status and painted `Idle` grey; diffline drew `●` for every status and
/// painted `Idle` and `Done` alike in green. Nothing about a status is a
/// property of the program looking at it, so the pair that carries the most
/// information wins: five glyphs, and `Idle` told apart from `Done`.
pub fn agent_status(s: AgentStatus) -> (&'static str, Color) {
    match s {
        AgentStatus::Working => ("◐", theme::yellow()),
        AgentStatus::Idle => ("○", theme::dimmer()),
        AgentStatus::Blocked => ("◼", theme::red()),
        AgentStatus::Done => ("●", theme::green()),
        AgentStatus::Unknown => ("·", theme::dimmer()),
    }
}

/// Writes `text`, brightening the characters the query matched.
///
/// `at` is the x, y and right edge to write between, and `positions` are
/// indices into `text` as `fuzzy::score` hands them back.
///
/// Shared because only one of the two programs was doing it. diffline's
/// finder ranks with the same matcher and has had the positions all along; it
/// simply had no code to draw them, and writing that code a second time was
/// the only thing standing in the way.
pub fn matched(
    buf: &mut Buffer,
    at: (u16, u16, u16),
    text: &str,
    positions: &[usize],
    base: Style,
    accent: Color,
) {
    let (x, y, max) = at;
    if positions.is_empty() {
        put_trunc(buf, x, y, max, text, base);
        return;
    }
    let hit = base.fg(accent).add_modifier(Modifier::BOLD);
    let mut cx = x;
    for (i, c) in text.chars().enumerate() {
        let style = if positions.contains(&i) { hit } else { base };
        cx = put(buf, cx, y, max, &c.to_string(), style);
        if cx >= max {
            break;
        }
    }
}

/// What a pane shows when it has nothing to show.
///
/// Three states rather than two, because they read differently and the code
/// kept collapsing them: still coming, came back empty, and went wrong. An
/// error painted the same grey as "nothing here" is an error nobody notices.
pub fn empty(buf: &mut Buffer, area: Rect, state: &Empty<'_>, ground: Color) {
    match state {
        Empty::Loading { widths, phase } => {
            for (row, w) in widths.iter().enumerate() {
                let y = area.y + row as u16;
                if y >= area.bottom() {
                    break;
                }
                skel_bar(
                    buf,
                    area.x + 2,
                    y,
                    pct(area.width.saturating_sub(4), *w),
                    row,
                    *phase,
                );
            }
        }
        Empty::Nothing(msg) => {
            put_trunc(
                buf,
                area.x + 2,
                area.y,
                area.right() - 1,
                msg,
                Style::default().bg(ground).fg(theme::dimmer()),
            );
        }
        Empty::Failed(msg) => {
            put_trunc(
                buf,
                area.x + 2,
                area.y,
                area.right() - 1,
                msg,
                Style::default().bg(ground).fg(theme::red()),
            );
        }
    }
}

/// Why a pane is empty.
pub enum Empty<'a> {
    /// On its way. `widths` are percentages, so a skeleton keeps its
    /// proportions at any pane size; `phase` travels the highlight down them,
    /// which is what separates "coming" from "stuck".
    Loading { widths: &'a [u16], phase: u64 },
    /// Came back with nothing, which is an answer.
    Nothing(&'a str),
    /// Went wrong.
    Failed(&'a str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::probe::{buffer, row};

    #[test]
    fn a_modal_head_hands_back_the_first_row_of_the_body() {
        // So a caller never counts the chrome, which is how two copies of it
        // drifted apart in the first place.
        let mut buf = buffer(40, 10);
        let m = Rect::new(0, 0, 40, 10);
        let top = modal_head(&mut buf, m, "TITLE", "hint", theme::cyan());
        assert_eq!(top, 3, "frame, title, rule");
        assert!(row(&buf, 1).contains("TITLE"));
        assert!(row(&buf, 1).contains("hint"));
    }

    #[test]
    fn the_query_line_shows_a_placeholder_only_while_it_is_empty() {
        let m = Rect::new(0, 0, 40, 3);
        let mut buf = buffer(40, 3);
        query_line(
            &mut buf,
            m,
            0,
            &Query {
                text: "",
                lead: "> ",
                placeholder: "type...",
                caret: false,
                accent: theme::yellow(),
            },
        );
        assert!(row(&buf, 0).contains("type..."));

        let mut buf = buffer(40, 3);
        query_line(
            &mut buf,
            m,
            0,
            &Query {
                text: "abc",
                lead: "> ",
                placeholder: "type...",
                caret: false,
                accent: theme::yellow(),
            },
        );
        assert!(row(&buf, 0).contains("abc"));
        assert!(!row(&buf, 0).contains("type..."));
    }

    #[test]
    fn the_caret_blinks_rather_than_being_drawn_always() {
        let m = Rect::new(0, 0, 40, 3);
        let mut on = buffer(40, 3);
        query_line(
            &mut on,
            m,
            0,
            &Query {
                text: "abc",
                lead: "> ",
                placeholder: "",
                caret: true,
                accent: theme::yellow(),
            },
        );
        let mut off = buffer(40, 3);
        query_line(
            &mut off,
            m,
            0,
            &Query {
                text: "abc",
                lead: "> ",
                placeholder: "",
                caret: false,
                accent: theme::yellow(),
            },
        );
        assert_ne!(row(&on, 0), row(&off, 0));
    }

    #[test]
    fn an_error_is_not_painted_the_same_as_having_nothing() {
        // They were collapsing into one grey line, and an error nobody
        // notices is an error nobody fixes.
        let area = Rect::new(0, 0, 30, 3);
        let mut nothing = buffer(30, 3);
        empty(
            &mut nothing,
            area,
            &Empty::Nothing("no files"),
            theme::panel(),
        );
        let mut failed = buffer(30, 3);
        empty(
            &mut failed,
            area,
            &Empty::Failed("no files"),
            theme::panel(),
        );
        assert_eq!(row(&nothing, 0), row(&failed, 0), "same words");
        assert_ne!(
            nothing.cell((2u16, 0u16)).map(|c| c.fg),
            failed.cell((2u16, 0u16)).map(|c| c.fg),
            "different colours"
        );
    }

    #[test]
    fn a_loading_pane_draws_a_skeleton_rather_than_a_word() {
        let mut buf = buffer(30, 4);
        empty(
            &mut buf,
            Rect::new(0, 0, 30, 4),
            &Empty::Loading {
                widths: &[60, 40],
                phase: 0,
            },
            theme::panel(),
        );
        assert!(!row(&buf, 0).trim().is_empty(), "something was drawn");
        assert!(!row(&buf, 1).trim().is_empty());
    }

    #[test]
    fn a_skeleton_taller_than_the_pane_stops_at_the_bottom() {
        let mut buf = buffer(30, 2);
        empty(
            &mut buf,
            Rect::new(0, 0, 30, 2),
            &Empty::Loading {
                widths: &[60, 40, 80, 30, 70],
                phase: 0,
            },
            theme::panel(),
        );
        // it did not panic, which is the assertion
    }
    /// A golden frame cannot see this: it compares symbols, and a highlight
    /// is a style. So the assertion is about the cells' colours.
    #[test]
    fn matched_brightens_the_letters_the_query_hit() {
        let mut buf = buffer(20, 1);
        let base = Style::default().bg(theme::bg()).fg(theme::fg());
        matched(
            &mut buf,
            (0, 0, 20),
            "sidebar",
            &[0, 1, 2],
            base,
            theme::cyan(),
        );
        assert_eq!(row(&buf, 0), "sidebar");
        let fg = |x: u16| buf[(x, 0)].style().fg;
        assert_eq!(fg(0), Some(theme::cyan()), "s is a hit");
        assert_eq!(fg(2), Some(theme::cyan()), "d is a hit");
        assert_eq!(fg(3), Some(theme::fg()), "e is not");
    }

    #[test]
    fn matched_with_nothing_to_mark_is_plain_text() {
        let mut buf = buffer(20, 1);
        let base = Style::default().bg(theme::bg()).fg(theme::fg());
        matched(&mut buf, (0, 0, 20), "sidebar", &[], base, theme::cyan());
        assert_eq!(row(&buf, 0), "sidebar");
        for x in 0..7 {
            assert_eq!(buf[(x, 0)].style().fg, Some(theme::fg()), "at {x}");
        }
    }

    /// The right edge is the pane's, not the text's.
    #[test]
    fn matched_stops_at_the_edge_it_was_given() {
        let mut buf = buffer(20, 1);
        let base = Style::default().bg(theme::bg()).fg(theme::fg());
        matched(&mut buf, (0, 0, 4), "sidebar", &[0, 6], base, theme::cyan());
        assert_eq!(row(&buf, 0), "side");
    }
}
