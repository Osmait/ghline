//! Organisms: a whole region, laid out and handed back.
//!
//! A modal with its frame, title, rule and body. A pane with its heading and
//! its rows. A list that knows which entry is selected and how far it has
//! scrolled. These own a `Rect`, put molecules in it, and return the geometry
//! the caller needs — where the body starts, which row landed where.
//!
//! Returning geometry rather than taking a closure is the choice that makes
//! them shareable. What goes *in* the body is the caller's, and the caller is
//! one of two programs with nothing in common but this crate.
//!
//! The rule: an organism may call anything below it and never another
//! organism.

use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::style::{Color, Modifier, Style};

use super::atom::{fill, hline, put, put_right, put_trunc, scrim};
use super::geom::{centered, centered_over};
use super::molecule::{Query, agent_status, modal_head, query_line};
use super::theme;

/// One row of a list, as laid out.
#[derive(Clone, Copy, Debug)]
pub struct RowSlot {
    /// Which entry it is, counting through the scroll.
    pub index: usize,
    /// Where to draw it. The selection bar is already painted in the first
    /// column, so start past it unless you mean to cover it.
    pub area: Rect,
    /// The background it was filled with, to build styles from.
    pub style: Style,
    /// Whether this is the row the cursor is on. Its ground and its bar are
    /// already painted; this is here so the caller can brighten its text too,
    /// which is the one part of a selected row that varies by caller.
    pub selected: bool,
}

/// Lays out a list of selectable rows and paints what they all have in
/// common: the background of the one selected, and the bar down its edge.
///
/// It deliberately does not know what a row contains. That varies — a theme
/// shows a name and a swatch, an agent a status dot and a working directory —
/// and a component that tried to cover both would take five optional
/// parameters and still be wrong for the third caller. This paints the part
/// that was written out thirty-one times, and hands back where to put the
/// rest.
pub fn rows(
    buf: &mut Buffer,
    area: Rect,
    count: usize,
    row_h: u16,
    sel: usize,
    scroll: usize,
    accent: Color,
) -> Vec<RowSlot> {
    let row_h = row_h.max(1);
    let mut out = Vec::new();
    for (n, index) in (scroll..count).enumerate() {
        let y = area.y + n as u16 * row_h;
        if y + row_h > area.bottom() {
            break;
        }
        let selected = index == sel;
        let bg = if selected {
            theme::sel()
        } else {
            theme::panel()
        };
        let slot = Rect {
            x: area.x,
            y,
            width: area.width,
            height: row_h,
        };
        fill(buf, slot, bg);
        let style = Style::default().bg(bg);
        if selected {
            for dy in 0..row_h {
                put(buf, area.x, y + dy, area.right(), "▌", style.fg(accent));
            }
        }
        debug_assert!(
            slot.bottom() <= area.bottom() && slot.right() <= area.right(),
            "a row was laid out past the list it belongs to"
        );
        out.push(RowSlot {
            index,
            area: slot,
            style,
            selected,
        });
    }
    out
}

/// The box a modal is drawn in.
///
/// `Dialog` rather than `Modal` because both programs already call the
/// question "which modal is open" a `Modal`, and one word cannot be both the
/// state and the box it is drawn in.
///
/// A builder rather than a function with eight arguments, because most modals
/// want most of the defaults and a call site reading `.hint("esc")` says what
/// `("", "esc", false, true, 60, 12)` does not.
pub struct Dialog<'a> {
    title: &'a str,
    hint: &'a str,
    accent: Color,
    width: u16,
    height: u16,
    scrim: bool,
    over_content: bool,
    footer: u16,
}

impl<'a> Dialog<'a> {
    /// A dialog with only a title, and defaults for the rest.
    ///
    /// The defaults are the commonest modal in either program: 60 by 12,
    /// diffline's yellow, no scrim, no footer, centred in the whole area. Each
    /// builder method below names what it changes.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            hint: "",
            accent: theme::yellow(),
            width: 60,
            height: 12,
            scrim: false,
            over_content: false,
            footer: 0,
        }
    }

    /// What to press, shown opposite the title.
    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    /// The frame, title and caret colour. ghline's modals are cyan and
    /// diffline's are yellow, which is a fact about the program rather than
    /// about a dialog.
    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    /// The box's size in cells, before it is centred.
    ///
    /// Clamped by `centered` to what the area can hold, so asking for more
    /// than the terminal has gives a smaller box rather than one drawn off the
    /// edge. A box too short for its own four rows of chrome gets an empty
    /// body, not a body outside the frame — see `open`.
    pub fn size(mut self, size: Size) -> Self {
        self.width = size.width;
        self.height = size.height;
        self
    }

    /// Dim what is behind it, so the eye stops at the modal.
    pub fn scrim(mut self) -> Self {
        self.scrim = true;
        self
    }

    /// Keep a gutter either side, for a modal over something still being
    /// read rather than over something finished with.
    pub fn over_content(mut self) -> Self {
        self.over_content = true;
        self
    }

    /// Reserve `rows` at the bottom, kept out of the body.
    pub fn footer(mut self, rows: u16) -> Self {
        self.footer = rows;
        self
    }

    /// Draws the box and says where its parts ended up.
    pub fn open(self, buf: &mut Buffer, area: Rect) -> Body {
        if self.scrim {
            scrim(buf, area);
        }
        let want = Size::new(self.width, self.height);
        let outer = if self.over_content {
            centered_over(area, want)
        } else {
            centered(area, want)
        };
        let top = modal_head(buf, outer, self.title, self.hint, self.accent);
        let bottom = outer.bottom().saturating_sub(1 + self.footer);
        // A box too short for its own chrome — four rows of frame, title and
        // rule in three rows of height. The body is empty either way, but
        // without this it is empty *below the frame*, so anything drawn into
        // it lands outside the modal. Found by the assertion under it.
        let top = top.min(bottom);
        debug_assert!(top <= bottom, "the body starts below where it ends");
        Body {
            outer,
            accent: self.accent,
            inner: Rect {
                x: outer.x + 1,
                y: top,
                width: outer.width.saturating_sub(2),
                height: bottom.saturating_sub(top),
            },
            footer: Rect {
                x: outer.x + 1,
                y: bottom,
                width: outer.width.saturating_sub(2),
                height: self.footer,
            },
        }
    }
}

/// One agent, as both programs draw one.
///
/// Two lines: what it is on the first, where it is or why it cannot be used
/// on the second.
///
/// ```text
///   ▌ ◐ ✳ claude       rewriting the reducer                   working
///   ▌     /home/you/work/tuikit                                wK:p1
///   │ │ │ │           │                                        │
///   0 2 4 6           19                        right-aligned ─┘
/// ```
///
/// The columns are the component's, which is the point: they were `+0 +2 +4
/// +6` in ghline's pane and `+1 +3 +5 +7` in diffline's modal, one column
/// apart for no reason either file could give.
pub struct AgentRow<'a> {
    /// What is running — `claude`, `codex`.
    pub kind: &'a str,
    /// The glyph for that kind. Passed in rather than looked up, so this
    /// stays out of the settings file: `line_shared::config::agent_icon` is what
    /// both callers use.
    pub icon: &'a str,
    /// What it is doing. Drives both the glyph on the left of the first line
    /// and the word right-aligned on it, which are the same fact twice on
    /// purpose: the glyph is readable at a glance down a column of rows, the
    /// word is readable at all.
    pub status: &'a str,
    /// The rest of the first line, after the kind. Empty draws nothing —
    /// diffline's modal has no room for it and ghline's pane does.
    pub title: &'a str,
    /// The second line: a working directory, or why this agent cannot be sent
    /// to. Whichever the caller thinks is worth the row.
    pub detail: &'a str,
    /// The right of the second line — ghline puts the multiplexer's own
    /// name for the pane there. Empty draws nothing.
    pub trailing: &'a str,
    /// Paints the row's ground as selected, and brightens its text.
    pub selected: bool,
    /// The mark down the left edge, when the caller wants one. `None` draws
    /// no mark: a modal marks the agent the queue would go to, a pane marks
    /// the row the cursor is on, and they are not always the same row.
    pub mark: Option<Color>,
    /// What the row sits on when it is not selected — a pane's background or
    /// a modal's panel.
    pub ground: Color,
}

/// Draws one into the two lines at the top of `area`.
pub fn agent_row(buf: &mut Buffer, area: Rect, r: &AgentRow<'_>) {
    let bg = if r.selected { theme::sel() } else { r.ground };
    fill(
        buf,
        Rect {
            height: 2.min(area.height),
            ..area
        },
        bg,
    );
    let base = Style::default().bg(bg);
    let y = area.y;

    if let Some(mark) = r.mark {
        put(buf, area.x, y, area.right(), "▌", base.fg(mark));
        if area.height > 1 {
            put(buf, area.x, y + 1, area.right(), "▌", base.fg(mark));
        }
    }

    let (glyph, colour) = agent_status(r.status);
    put(buf, area.x + 2, y, area.right(), glyph, base.fg(colour));
    // Two marks, two meanings: the one to the left is what it is doing and is
    // coloured by state, this one is who it is and never changes.
    put(
        buf,
        area.x + 4,
        y,
        area.right(),
        r.icon,
        base.fg(theme::purple()),
    );
    put_trunc(
        buf,
        area.x + 6,
        y,
        area.right().saturating_sub(12),
        r.kind,
        base.fg(theme::cyan_soft()).add_modifier(Modifier::BOLD),
    );
    let state_x = put_right(
        buf,
        area.right().saturating_sub(2),
        y,
        r.status,
        base.fg(colour),
    );
    if !r.title.is_empty() {
        put_trunc(
            buf,
            area.x + 19,
            y,
            state_x.saturating_sub(2),
            r.title,
            base.fg(if r.selected {
                theme::bright()
            } else {
                theme::fg()
            }),
        );
    }

    if area.height > 1 {
        // The right of the second line is laid out first, because where it
        // starts is where the detail has to stop.
        let end = if r.trailing.is_empty() {
            area.right().saturating_sub(2)
        } else {
            put_right(
                buf,
                area.right().saturating_sub(2),
                y + 1,
                r.trailing,
                base.fg(theme::dimmer()),
            )
            .saturating_sub(2)
        };
        put_trunc(
            buf,
            area.x + 6,
            y + 1,
            end,
            r.detail,
            base.fg(if r.selected {
                theme::bright()
            } else {
                theme::dimmer()
            }),
        );
    }
}

/// A modal that has been drawn, and where its parts are.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    /// The whole box, frame included — what a click on the chrome hits.
    pub outer: Rect,
    /// Between the rule and the footer. Everything a caller draws goes here.
    pub inner: Rect,
    /// Empty when none was asked for.
    pub footer: Rect,
    /// The dialog's accent, carried through so that what a caller draws inside
    /// the body matches the frame around it without being told twice.
    pub accent: Color,
}

impl Body {
    /// The body as a list of selectable rows.
    pub fn rows(
        &self,
        buf: &mut Buffer,
        count: usize,
        row_h: u16,
        sel: usize,
        scroll: usize,
    ) -> Vec<RowSlot> {
        rows(buf, self.inner, count, row_h, sel, scroll, self.accent)
    }

    /// Draws a query line at the top of the body and returns what is left
    /// below it.
    ///
    /// Two calls rather than one that does both: a `search(…)` taking the
    /// query *and* the list took ten arguments, which is the same smell as a
    /// component with five optional parameters. Chained, it reads as what it
    /// is — a query line, and then rows under it.
    pub fn query(
        &self,
        buf: &mut Buffer,
        query: &str,
        lead: &str,
        placeholder: &str,
        caret: bool,
    ) -> Self {
        query_line(
            buf,
            self.outer,
            self.inner.y,
            &Query {
                text: query,
                lead,
                placeholder,
                caret,
                accent: self.accent,
            },
        );
        Self {
            inner: Rect {
                y: self.inner.y + 2,
                height: self.inner.height.saturating_sub(2),
                ..self.inner
            },
            ..*self
        }
    }
}

/// A titled region of the interface: a filled ground, a heading with a title
/// and a count opposite, and a rule under it.
///
/// `Section` and not `Pane` for the same reason the dialog is not a `Modal` —
/// both programs already call the question "which pane has focus" a `Pane`,
/// and one word cannot be the focus and the furniture.
///
/// The fixed-furniture twin of `Dialog`, and the same bargain — it draws the
/// chrome and hands back the body, so no caller counts rows.
pub struct Section<'a> {
    title: &'a str,
    count: Option<String>,
    ground: Color,
    head_bg: Color,
    focused: bool,
}

impl<'a> Section<'a> {
    /// A section with only a title: no count, unfocused, on the darker of the
    /// two panel grounds.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            count: None,
            ground: theme::panel_alt(),
            head_bg: theme::panel(),
            focused: false,
        }
    }

    /// The number opposite the title — how many files, how many results.
    pub fn count(mut self, n: usize) -> Self {
        self.count = Some(n.to_string());
        self
    }

    /// Anything else opposite the title, when it is not a plain count.
    pub fn note(mut self, text: impl Into<String>) -> Self {
        self.count = Some(text.into());
        self
    }

    /// What the pane sits on. The trees are darker than the diff beside them.
    pub fn ground(mut self, bg: Color) -> Self {
        self.ground = bg;
        self
    }

    /// A focused pane says so in its title rather than with a border, which
    /// would cost a column the content wants.
    pub fn focused(mut self, yes: bool) -> Self {
        self.focused = yes;
        self
    }

    /// Draws it and returns the body below the rule.
    pub fn open(self, buf: &mut Buffer, area: Rect) -> Rect {
        fill(buf, area, self.ground);
        let head = Rect { height: 1, ..area };
        fill(buf, head, self.head_bg);
        let hs = Style::default().bg(self.head_bg);
        put(
            buf,
            area.x + 1,
            area.y,
            area.right(),
            self.title,
            hs.fg(if self.focused {
                theme::yellow()
            } else {
                theme::dim()
            }),
        );
        if let Some(count) = &self.count {
            put_right(buf, area.right() - 1, area.y, count, hs.fg(theme::dimmer()));
        }
        hline(buf, area.x, area.y + 1, area.width, theme::border_soft());
        Rect {
            y: area.y + 2,
            height: area.height.saturating_sub(2),
            ..area
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{buffer, row};

    #[test]
    fn rows_lays_out_every_entry_that_fits_and_no_more() {
        let mut buf = buffer(20, 4);
        let slots = rows(
            &mut buf,
            Rect::new(0, 0, 20, 4),
            10,
            2,
            0,
            0,
            theme::yellow(),
        );
        assert_eq!(slots.len(), 2, "two two-row entries fit in four rows");
        assert_eq!(slots[1].area.y, 2);
    }

    #[test]
    fn rows_counts_through_the_scroll() {
        let mut buf = buffer(20, 4);
        let slots = rows(
            &mut buf,
            Rect::new(0, 0, 20, 4),
            100,
            1,
            0,
            30,
            theme::yellow(),
        );
        assert_eq!(slots[0].index, 30);
    }

    #[test]
    fn only_the_selected_row_gets_the_bar() {
        let mut buf = buffer(20, 3);
        let slots = rows(
            &mut buf,
            Rect::new(0, 0, 20, 3),
            3,
            1,
            1,
            0,
            theme::yellow(),
        );
        assert!(slots[1].selected);
        assert!(!slots[0].selected);
        assert!(row(&buf, 1).starts_with('\u{258c}'), "{:?}", row(&buf, 1));
        assert!(!row(&buf, 0).starts_with('\u{258c}'));
    }

    #[test]
    fn a_selection_bar_runs_the_whole_height_of_a_tall_row() {
        // A two-row entry barred on only its first line reads as two entries,
        // one of them selected.
        let mut buf = buffer(20, 2);
        rows(
            &mut buf,
            Rect::new(0, 0, 20, 2),
            1,
            2,
            0,
            0,
            theme::yellow(),
        );
        assert!(row(&buf, 0).starts_with('\u{258c}'));
        assert!(row(&buf, 1).starts_with('\u{258c}'));
    }

    #[test]
    fn an_empty_list_lays_out_nothing() {
        let mut buf = buffer(20, 4);
        assert!(
            rows(
                &mut buf,
                Rect::new(0, 0, 20, 4),
                0,
                1,
                0,
                0,
                theme::yellow()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_zero_row_height_does_not_loop_for_ever() {
        let mut buf = buffer(20, 4);
        let slots = rows(
            &mut buf,
            Rect::new(0, 0, 20, 4),
            10,
            0,
            0,
            0,
            theme::yellow(),
        );
        assert_eq!(slots.len(), 4, "treated as one row each");
    }

    #[test]
    fn a_dialog_keeps_its_body_inside_its_frame() {
        let mut buf = buffer(40, 12);
        let b = Dialog::new("T")
            .size(Size::new(30, 8))
            .open(&mut buf, Rect::new(0, 0, 40, 12));
        assert!(b.inner.y > b.outer.y, "below the rule");
        assert!(b.inner.bottom() < b.outer.bottom(), "above the frame");
        assert!(b.inner.x > b.outer.x, "inside the left edge");
        assert!(b.inner.right() < b.outer.right());
    }

    #[test]
    fn a_footer_comes_out_of_the_body_rather_than_off_the_bottom() {
        let mut buf = buffer(40, 12);
        let plain = Dialog::new("T")
            .size(Size::new(30, 8))
            .open(&mut buf, Rect::new(0, 0, 40, 12));
        let mut buf = buffer(40, 12);
        let footed = Dialog::new("T")
            .size(Size::new(30, 8))
            .footer(2)
            .open(&mut buf, Rect::new(0, 0, 40, 12));
        assert_eq!(footed.outer, plain.outer, "the box is the same size");
        assert_eq!(footed.inner.height, plain.inner.height - 2);
        assert_eq!(footed.footer.height, 2);
        assert_eq!(footed.footer.y, footed.inner.bottom());
    }

    #[test]
    fn a_dialog_too_big_for_the_screen_is_cut_to_fit() {
        let mut buf = buffer(20, 6);
        let b = Dialog::new("T")
            .size(Size::new(200, 200))
            .open(&mut buf, Rect::new(0, 0, 20, 6));
        assert!(b.outer.width <= 20);
        assert!(b.outer.height <= 6);
    }

    #[test]
    fn a_search_body_puts_its_rows_under_the_query_not_over_it() {
        let mut buf = buffer(40, 12);
        let b = Dialog::new("/")
            .size(Size::new(30, 10))
            .open(&mut buf, Rect::new(0, 0, 40, 12));
        let slots = b
            .query(&mut buf, "abc", "> ", "", false)
            .rows(&mut buf, 3, 1, 0, 0);
        assert!(!slots.is_empty());
        assert!(
            slots[0].area.y > b.inner.y,
            "the first row sits below the query line"
        );
        assert!(
            row(&buf, b.inner.y).contains("abc"),
            "and the query is drawn"
        );
    }

    #[test]
    fn a_section_hands_back_the_body_under_its_rule() {
        let mut buf = buffer(20, 10);
        let body = Section::new("TITLE").open(&mut buf, Rect::new(0, 0, 20, 10));
        assert_eq!(body.y, 2, "heading and rule");
        assert_eq!(body.height, 8);
        assert!(row(&buf, 0).contains("TITLE"));
    }

    #[test]
    fn a_count_sits_opposite_the_title() {
        let mut buf = buffer(20, 4);
        Section::new("FILES")
            .count(12)
            .open(&mut buf, Rect::new(0, 0, 20, 4));
        let head = row(&buf, 0);
        assert!(head.starts_with(" FILES"), "{head:?}");
        assert!(head.trim_end().ends_with("12"), "{head:?}");
    }

    #[test]
    fn a_focused_section_says_so_in_its_title() {
        let mut lit = buffer(20, 4);
        Section::new("T")
            .focused(true)
            .open(&mut lit, Rect::new(0, 0, 20, 4));
        let mut dim = buffer(20, 4);
        Section::new("T")
            .focused(false)
            .open(&mut dim, Rect::new(0, 0, 20, 4));
        assert_ne!(
            lit.cell((1u16, 0u16)).map(|c| c.fg),
            dim.cell((1u16, 0u16)).map(|c| c.fg),
            "a focused pane has to be visible as one"
        );
    }
    #[test]
    fn the_scrim_only_dims_when_it_is_asked_for() {
        let base = Rect::new(0, 0, 20, 6);
        let mut lit = buffer(20, 6);
        fill(&mut lit, base, theme::green());
        Dialog::new("T").size(Size::new(6, 3)).open(&mut lit, base);

        let mut dimmed = buffer(20, 6);
        fill(&mut dimmed, base, theme::green());
        Dialog::new("T")
            .size(Size::new(6, 3))
            .scrim()
            .open(&mut dimmed, base);

        let corner = |b: &Buffer| b.cell((0u16, 0u16)).map(|c| c.bg);
        assert_ne!(
            corner(&lit),
            corner(&dimmed),
            "the scrim changed the ground"
        );
    }
    /// Every column the doc comment claims, checked against a drawn row.
    ///
    /// This is the test the two hand-written copies could not have: one lived
    /// in a modal and the other in a pane, and reaching either meant building
    /// an entire application first.
    #[test]
    fn an_agent_row_puts_each_field_in_its_column() {
        let mut buf = buffer(70, 2);
        agent_row(
            &mut buf,
            Rect::new(0, 0, 70, 2),
            &AgentRow {
                kind: "claude",
                icon: "✳",
                status: "working",
                title: "rewriting the reducer",
                detail: "/home/you/work/tuikit",
                trailing: "wK:p1",
                selected: false,
                mark: Some(theme::cyan()),
                ground: theme::bg(),
            },
        );
        let first = row(&buf, 0);
        assert!(
            first.starts_with("▌ ◐ ✳ claude"),
            "the mark is at the row's own left edge, then the columns: {first}"
        );
        assert!(first.contains("rewriting the reducer"), "{first}");
        assert!(first.trim_end().ends_with("working"), "{first}");

        let second = row(&buf, 1);
        assert!(
            second.starts_with("▌     /home/you/work/tuikit"),
            "{second}"
        );
        assert!(second.trim_end().ends_with("wK:p1"), "{second}");
    }

    /// The two fields diffline has no room for, left out rather than blanked.
    #[test]
    fn an_agent_row_without_a_title_or_a_pane_draws_neither() {
        let mut buf = buffer(70, 2);
        agent_row(
            &mut buf,
            Rect::new(0, 0, 70, 2),
            &AgentRow {
                kind: "codex",
                icon: "◆",
                status: "idle",
                title: "",
                detail: "/src/other",
                trailing: "",
                selected: false,
                mark: None,
                ground: theme::bg(),
            },
        );
        assert_eq!(
            row(&buf, 0),
            "  ○ ◆ codex".to_string() + &" ".repeat(53) + "idle"
        );
        assert_eq!(row(&buf, 1).trim_end(), "      /src/other");
    }

    /// A one-line slot is what a pane with an odd number of rows left hands
    /// out. The second line has to be dropped rather than drawn over whatever
    /// is below.
    #[test]
    fn an_agent_row_in_a_single_line_keeps_to_it() {
        let mut buf = buffer(70, 2);
        agent_row(
            &mut buf,
            Rect::new(0, 0, 70, 1),
            &AgentRow {
                kind: "claude",
                icon: "✳",
                status: "done",
                title: "",
                detail: "/home/you/work/tuikit",
                trailing: "wK:p1",
                selected: false,
                mark: Some(theme::cyan()),
                ground: theme::bg(),
            },
        );
        assert!(row(&buf, 0).contains("claude"));
        assert_eq!(row(&buf, 1), "", "nothing below the row it was given");
    }

    /// The status glyph and its colour are one decision, taken in one place.
    #[test]
    fn every_status_has_its_own_glyph() {
        let glyphs: Vec<&str> = ["working", "idle", "blocked", "done", "unknown"]
            .iter()
            .map(|status| agent_status(status).0)
            .collect();
        let mut seen = glyphs.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            glyphs.len(),
            "two states drawn alike: {glyphs:?}"
        );
    }
}
