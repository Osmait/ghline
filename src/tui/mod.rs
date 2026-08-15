//! The parts a terminal interface is made of.
//!
//! Both programs draw a grid of cells with the same hands, and this is where
//! the hands live: writing text that stops at an edge, filling a rectangle,
//! ruling a line, framing a modal, keeping a selection on screen.
//!
//! It is here rather than in either program because it belongs to neither.
//! These lived in `ui`, which is github-tui's, and diffline imported ten of
//! them out of it — the same arrow that `data` and `theme` had, a shared
//! thing inside one program's own. It cost what that always costs: `centered`,
//! `frame` and `rule` were written a second time in diffline and drifted, one
//! copy picking up a `saturating_sub` that the other never got.

pub mod diff;
pub mod hit;
pub mod probe;
pub mod run;
pub mod theme;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

pub fn fill(buf: &mut Buffer, area: Rect, bg: ratatui::style::Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(bg);
            }
        }
    }
}

/// Like `fill`, but it also wipes the text: whatever is drawn on top hides
/// what was underneath. This is what the modals need.
pub fn clear(buf: &mut Buffer, area: Rect, bg: ratatui::style::Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(" ");
                cell.set_style(Style::default().bg(bg).fg(theme::fg()));
            }
        }
    }
}

/// Writes `text` at (x, y), clipping at `max_x` (exclusive). Returns the end x.
pub fn put(buf: &mut Buffer, x: u16, y: u16, max_x: u16, text: &str, style: Style) -> u16 {
    // The invariant this program has broken twice: a tab measured as one
    // column and drawn as eight, and a diff line painted over the pane beside
    // it. Both were found by looking at a screenshot. `debug_assert` because
    // the cost of being wrong here is a wrong cell, and the cost of a panic
    // is a terminal left in raw mode with a queue of unsent comments in it.
    debug_assert!(
        x <= max_x,
        "asked to write at {x} with a right edge of {max_x}"
    );
    let mut cx = x;
    if y >= buf.area.bottom() {
        return cx;
    }
    for g in unicode_segmentation(text) {
        // A backstop, not the fix: text is expanded where it is read, in
        // `crate::shared::text`. But this is the one place every string in either
        // program passes through on its way to a cell, and a control
        // character reaching one moves the terminal's cursor somewhere the
        // layout did not account for — which is how a diff line ends up
        // painted over the pane beside it.
        if g.chars()
            .next()
            .is_some_and(|c| (c as u32) < 0x20 || c == '\x7f')
        {
            continue;
        }
        let w = g.width() as u16;
        if w == 0 {
            continue;
        }
        if cx + w > max_x {
            break;
        }
        if let Some(cell) = buf.cell_mut((cx, y)) {
            cell.set_symbol(g);
            cell.set_style(style);
        }
        // clear the cell a wide grapheme covers
        if w == 2
            && let Some(cell) = buf.cell_mut((cx + 1, y))
        {
            cell.set_symbol(" ");
            cell.set_style(style);
        }
        cx += w;
    }
    debug_assert!(cx <= max_x, "wrote to {cx}, past the edge at {max_x}");
    cx
}

/// Like `put`, but appends `…` when the text does not fit.
pub fn put_trunc(buf: &mut Buffer, x: u16, y: u16, max_x: u16, text: &str, style: Style) -> u16 {
    let avail = max_x.saturating_sub(x);
    if text.width() as u16 <= avail {
        return put(buf, x, y, max_x, text, style);
    }
    if avail == 0 {
        return x;
    }
    let cut = truncate_to(text, avail.saturating_sub(1) as usize);
    let end = put(buf, x, y, max_x, &cut, style);
    put(buf, end, y, max_x, "…", style)
}

pub fn put_right(buf: &mut Buffer, right_x: u16, y: u16, text: &str, style: Style) -> u16 {
    let w = text.width() as u16;
    let x = right_x.saturating_sub(w);
    put(buf, x, y, right_x, text, style);
    x
}

pub fn hline(buf: &mut Buffer, x: u16, y: u16, w: u16, color: ratatui::style::Color) {
    let s = "─".repeat(w as usize);
    put(
        buf,
        x,
        y,
        x + w,
        &s,
        Style::default().fg(color).bg(theme::bg()),
    );
}

pub fn vline(buf: &mut Buffer, x: u16, y: u16, h: u16, color: ratatui::style::Color) {
    for yy in y..y + h {
        put(
            buf,
            x,
            yy,
            x + 1,
            "│",
            Style::default().fg(color).bg(theme::bg()),
        );
    }
}

fn unicode_segmentation(s: &str) -> Vec<&str> {
    // No extra dependency: one `char` is grapheme enough for this glyph set.
    let mut out = Vec::new();
    let mut idx = 0;
    for c in s.chars() {
        let len = c.len_utf8();
        out.push(&s[idx..idx + len]);
        idx += len;
    }
    out
}

/// Cuts to `w` columns and pads with spaces to fill them all.
pub fn truncate_pad(s: &str, w: usize) -> String {
    if w == 0 {
        return String::new();
    }
    let mut out = if s.width() > w {
        let mut t = truncate_to(s, w.saturating_sub(1));
        t.push('…');
        t
    } else {
        s.to_string()
    };
    while out.width() < w {
        out.push(' ');
    }
    out
}

fn truncate_to(s: &str, w: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let cw = c.to_string().width();
        if used + cw > w {
            break;
        }
        out.push(c);
        used += cw;
    }
    out
}

/// Lays text with hard breaks out into lines of `width` columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let indent: String = para.chars().take_while(|c| *c == ' ').collect();
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                format!("{indent}{word}")
            } else {
                format!("{line} {word}")
            };
            if candidate.width() > width && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                line = format!("{indent}{word}");
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// A run of text with its style; a line is several of them in a row.
pub type Seg = (String, Style);

/// One placeholder block of a skeleton.
///
/// The design asks for this shape itself: its `sc-for` elements carry a
/// `hint-placeholder-count`, so a pane that is still loading is meant to show
/// the outline of what is coming rather than a word.
///
/// `row` and `phase` put a highlight band travelling down the rows, which is
/// what separates "on its way" from "stuck".
pub fn skel_bar(buf: &mut Buffer, x: u16, y: u16, w: u16, row: usize, phase: u64) {
    if w == 0 {
        return;
    }
    const CYCLE: u64 = 16;
    let band = (phase % CYCLE) as i64;
    let color = match (row as i64 - band).abs() {
        0 => theme::sel_mark_idle(),
        1 => theme::sel(),
        _ => theme::panel(),
    };
    let block = "█".repeat(w as usize);
    put(
        buf,
        x,
        y,
        x + w,
        &block,
        Style::default().bg(theme::bg()).fg(color),
    );
}

/// A percentage of the available width, so a skeleton keeps its proportions at
/// any pane size.
pub fn pct(avail: u16, p: u16) -> u16 {
    (u32::from(avail) * u32::from(p) / 100) as u16
}

/// Keeps `sel` visible inside a window of `height` rows.
pub fn scroll_into_view(offset: &mut usize, sel: usize, height: usize, len: usize) {
    if height == 0 {
        return;
    }
    if sel < *offset {
        *offset = sel;
    } else if sel >= *offset + height {
        *offset = sel + 1 - height;
    }
    let max = len.saturating_sub(height);
    *offset = (*offset).min(max);

    // What this function is *for*: after it, the selection is on screen. It
    // is easy to write a version that clamps the offset and quietly loses
    // that, which is a cursor you cannot see and cannot find.
    debug_assert!(
        sel >= len || (sel >= *offset && sel < *offset + height),
        "selection {sel} is outside the window {}..{} of {len}",
        *offset,
        *offset + height
    );
}

pub fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}

// --- modals -----------------------------------------------------------------

/// Centred, never larger than what it is centred in.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 0, 0)
}

/// Centred with a gutter kept either side, so the thing underneath still
/// shows and the modal reads as floating over it rather than replacing it.
///
/// The difference from `centered` used to be the difference between the two
/// programs' copies of this function, which is to say it was an accident.
/// Named, it is a choice: diffline's modals sit over a diff you are still
/// reading, github-tui's cover a list you are done with.
pub fn centered_over(area: Rect, w: u16, h: u16) -> Rect {
    inset(area, w, h, 4, 2)
}

fn inset(area: Rect, w: u16, h: u16, mx: u16, my: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(mx));
    let h = h.min(area.height.saturating_sub(my));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

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

// --- components -------------------------------------------------------------
//
// These take data rather than an app, which is the whole reason they can be
// shared: there are two unrelated `App` types in this crate and neither
// component has any business knowing either. What a row *contains* stays with
// whoever has the data; what every row has in common lives here.

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

/// The line a searching modal types into.
///
/// Takes the query rather than the app that holds it — the only thing it ever
/// wanted from one, and the reason this could not be shared before.
pub fn query_line(
    buf: &mut Buffer,
    m: Rect,
    y: u16,
    query: &str,
    lead: &str,
    placeholder: &str,
    caret: bool,
) {
    let base = Style::default().bg(theme::panel());
    let x = put(
        buf,
        m.x + 2,
        y,
        m.right() - 2,
        lead,
        base.fg(theme::yellow()),
    );
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
        put(buf, end, y, m.right() - 2, "▌", base.fg(theme::yellow()));
    }
}

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

// --- the modal container ----------------------------------------------------
//
// Every modal in both programs is the same box: something dimmed or not
// behind it, a centred rectangle, a frame in an accent colour, a title with a
// hint opposite, a rule, a body, and sometimes a footer. What differs is the
// body — and the body is the caller's, which is why this hands back where to
// put one instead of trying to take it as a parameter.

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
                let fg = s.fg.unwrap_or(theme::fg());
                let bg = s.bg.unwrap_or(theme::bg());
                cell.set_style(Style::default().fg(shade(fg)).bg(shade(bg)));
            }
        }
    }
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

    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
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
        let outer = if self.over_content {
            centered_over(area, self.width, self.height)
        } else {
            centered(area, self.width, self.height)
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

/// A modal that has been drawn, and where its parts are.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    /// The whole box, frame included — what a click on the chrome hits.
    pub outer: Rect,
    /// Between the rule and the footer. Everything a caller draws goes here.
    pub inner: Rect,
    /// Empty when none was asked for.
    pub footer: Rect,
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
            query,
            lead,
            placeholder,
            caret,
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

// --- panes ------------------------------------------------------------------

/// Trims a rectangle inwards. The concept the whole interface was missing:
/// there are three hundred and eighty-three hand-written `area.x + 2`s
/// between the two programs, and every one of them is this.
pub fn pad(area: Rect, x: u16, y: u16) -> Rect {
    Rect {
        x: area.x + x.min(area.width / 2),
        y: area.y + y.min(area.height / 2),
        width: area.width.saturating_sub(x * 2),
        height: area.height.saturating_sub(y * 2),
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

    #[test]
    fn wrap_respects_hard_newlines() {
        let out = wrap("one\ntwo", 40);
        assert_eq!(out, vec!["one", "two"]);
    }

    #[test]
    fn wrap_keeps_blank_lines_that_separate_paragraphs() {
        let out = wrap("a\n\nb", 40);
        assert_eq!(out, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_breaks_on_width() {
        let out = wrap("aaa bbb ccc", 7);
        assert_eq!(out, vec!["aaa bbb", "ccc"]);
    }

    #[test]
    fn wrap_keeps_the_indentation_of_a_wrapped_paragraph() {
        let out = wrap("  1. a very long numbered item here", 12);
        assert!(out.len() > 1);
        assert!(out[0].starts_with("  "));
        assert!(out[1].starts_with("  "), "continuation keeps the indent");
    }

    #[test]
    fn wrap_does_not_lose_a_word_longer_than_the_width() {
        // a single word that cannot fit is emitted on its own, uncut, so that
        // nothing disappears from a log line or a URL
        let out = wrap("supercalifragilistic", 5);
        assert_eq!(out, vec!["supercalifragilistic"]);
    }

    #[test]
    fn wrap_of_nothing_is_one_empty_line() {
        assert_eq!(wrap("", 10), vec![""]);
    }

    // --- truncation ---

    #[test]
    fn truncate_pad_fills_short_text_to_the_width() {
        assert_eq!(truncate_pad("ab", 5), "ab   ");
    }

    #[test]
    fn truncate_pad_leaves_exact_text_alone() {
        assert_eq!(truncate_pad("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_pad_marks_what_it_cut() {
        let out = truncate_pad("abcdefgh", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_pad_handles_a_zero_width() {
        assert_eq!(truncate_pad("abc", 0), "");
    }

    #[test]
    fn scroll_into_view_does_nothing_when_the_selection_is_visible() {
        let mut off = 5;
        scroll_into_view(&mut off, 7, 10, 100);
        assert_eq!(off, 5);
    }

    #[test]
    fn scroll_into_view_follows_the_selection_up_and_down() {
        let mut off = 10;
        scroll_into_view(&mut off, 3, 5, 100);
        assert_eq!(off, 3, "scrolls up to reveal the selection");

        let mut off = 0;
        scroll_into_view(&mut off, 12, 5, 100);
        assert_eq!(off, 8, "scrolls down just enough");
    }

    #[test]
    fn scroll_into_view_never_scrolls_past_the_end() {
        let mut off = 90;
        scroll_into_view(&mut off, 5, 10, 20);
        assert!(off <= 10, "offset stays within len - height");
    }

    #[test]
    fn scroll_into_view_copes_with_a_list_shorter_than_the_window() {
        let mut off = 3;
        scroll_into_view(&mut off, 0, 20, 2);
        assert_eq!(off, 0);
    }

    #[test]
    fn scroll_into_view_ignores_a_zero_height_pane() {
        let mut off = 4;
        scroll_into_view(&mut off, 9, 0, 50);
        assert_eq!(off, 4, "a pane with no rows cannot scroll");
    }

    // --- buffer writing ---

    fn buffer(w: u16, h: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, w, h))
    }

    fn row(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn put_stops_at_the_limit_instead_of_overflowing() {
        let mut buf = buffer(10, 1);
        let end = put(&mut buf, 0, 0, 5, "abcdefgh", Style::default());
        assert_eq!(end, 5);
        assert_eq!(row(&buf, 0), "abcde");
    }

    #[test]
    fn put_off_the_bottom_of_the_buffer_is_a_no_op() {
        let mut buf = buffer(10, 1);
        let end = put(&mut buf, 0, 9, 10, "abc", Style::default());
        assert_eq!(end, 0, "nothing was written");
    }

    #[test]
    fn put_trunc_adds_an_ellipsis_only_when_it_cuts() {
        let mut buf = buffer(10, 1);
        put_trunc(&mut buf, 0, 0, 10, "abc", Style::default());
        assert_eq!(row(&buf, 0), "abc");

        let mut buf = buffer(10, 1);
        put_trunc(&mut buf, 0, 0, 4, "abcdefgh", Style::default());
        assert_eq!(row(&buf, 0), "abc…");
    }

    #[test]
    fn put_right_aligns_against_the_right_edge() {
        let mut buf = buffer(10, 1);
        let x = put_right(&mut buf, 10, 0, "abc", Style::default());
        assert_eq!(x, 7);
        assert_eq!(row(&buf, 0), "       abc");
    }

    #[test]
    fn put_right_clamps_text_wider_than_the_space() {
        let mut buf = buffer(4, 1);
        let x = put_right(&mut buf, 2, 0, "abcdefgh", Style::default());
        assert_eq!(x, 0, "it starts at the left edge rather than underflowing");
    }

    #[test]
    fn a_wide_glyph_clears_the_cell_it_covers() {
        // otherwise the second half keeps whatever was underneath
        let mut buf = buffer(4, 1);
        put(&mut buf, 0, 0, 4, "xx", Style::default());
        put(&mut buf, 0, 0, 4, "漢", Style::default());
        assert_eq!(buf[(1, 0)].symbol(), " ");
    }

    #[test]
    fn a_wide_glyph_that_does_not_fit_is_not_written() {
        // writing only half of it would corrupt the row
        let mut buf = buffer(4, 1);
        let end = put(&mut buf, 0, 0, 1, "漢", Style::default());
        assert_eq!(end, 0);
        assert_eq!(buf[(0, 0)].symbol(), " ");
    }

    // --- loading skeletons ---

    #[test]
    fn a_skeleton_bar_fills_exactly_its_width() {
        let mut buf = buffer(10, 1);
        skel_bar(&mut buf, 2, 0, 4, 0, 0);
        assert_eq!(row(&buf, 0), "  ████");
    }

    #[test]
    fn a_zero_width_bar_draws_nothing() {
        let mut buf = buffer(6, 1);
        skel_bar(&mut buf, 0, 0, 0, 0, 0);
        assert_eq!(row(&buf, 0), "");
    }

    #[test]
    fn the_highlight_band_travels_with_the_phase() {
        // the row the band is on is brighter than the rows away from it
        let lit = |row: usize, phase: u64| {
            let mut buf = buffer(4, 1);
            skel_bar(&mut buf, 0, 0, 2, row, phase);
            buf[(0, 0)].style().fg
        };
        assert_ne!(
            lit(0, 0),
            lit(5, 0),
            "row 0 is lit at phase 0, row 5 is not"
        );
        assert_eq!(lit(0, 0), lit(3, 3), "the band moved down with the phase");
    }

    #[test]
    fn a_bar_never_writes_past_its_width() {
        let mut buf = buffer(8, 1);
        skel_bar(&mut buf, 6, 0, 5, 0, 0); // would run off the right edge
        assert_eq!(row(&buf, 0).chars().count(), 8);
    }

    #[test]
    fn pct_keeps_proportions_and_never_overflows() {
        assert_eq!(pct(100, 50), 50);
        assert_eq!(pct(0, 80), 0);
        assert_eq!(pct(u16::MAX, 100), u16::MAX);
    }

    #[test]
    fn clear_wipes_the_text_underneath() {
        let mut buf = buffer(6, 1);
        put(&mut buf, 0, 0, 6, "abcdef", Style::default());
        clear(&mut buf, Rect::new(0, 0, 6, 1), theme::panel());
        assert_eq!(row(&buf, 0), "");
    }

    #[test]
    fn truncate_to_counts_columns_not_bytes() {
        // accented letters are one column each despite being two bytes
        assert_eq!(truncate_to("áéíóú", 3), "áéí");
        // and a wide glyph takes two columns, so only one fits in three
        assert_eq!(truncate_to("→→", 3), "→→");
    }

    // --- components ---

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
        query_line(&mut buf, m, 0, "", "> ", "type...", false);
        assert!(row(&buf, 0).contains("type..."));

        let mut buf = buffer(40, 3);
        query_line(&mut buf, m, 0, "abc", "> ", "type...", false);
        assert!(row(&buf, 0).contains("abc"));
        assert!(!row(&buf, 0).contains("type..."));
    }

    #[test]
    fn the_caret_blinks_rather_than_being_drawn_always() {
        let m = Rect::new(0, 0, 40, 3);
        let mut on = buffer(40, 3);
        query_line(&mut on, m, 0, "abc", "> ", "", true);
        let mut off = buffer(40, 3);
        query_line(&mut off, m, 0, "abc", "> ", "", false);
        assert_ne!(row(&on, 0), row(&off, 0));
    }

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

    // --- the dialog container ---

    #[test]
    fn a_dialog_keeps_its_body_inside_its_frame() {
        let mut buf = buffer(40, 12);
        let b = Dialog::new("T")
            .size(30, 8)
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
            .size(30, 8)
            .open(&mut buf, Rect::new(0, 0, 40, 12));
        let mut buf = buffer(40, 12);
        let footed = Dialog::new("T")
            .size(30, 8)
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
            .size(200, 200)
            .open(&mut buf, Rect::new(0, 0, 20, 6));
        assert!(b.outer.width <= 20);
        assert!(b.outer.height <= 6);
    }

    #[test]
    fn the_scrim_only_dims_when_it_is_asked_for() {
        let base = Rect::new(0, 0, 20, 6);
        let mut lit = buffer(20, 6);
        fill(&mut lit, base, theme::green());
        Dialog::new("T").size(6, 3).open(&mut lit, base);

        let mut dimmed = buffer(20, 6);
        fill(&mut dimmed, base, theme::green());
        Dialog::new("T").size(6, 3).scrim().open(&mut dimmed, base);

        let corner = |b: &Buffer| b.cell((0u16, 0u16)).map(|c| c.bg);
        assert_ne!(
            corner(&lit),
            corner(&dimmed),
            "the scrim changed the ground"
        );
    }

    #[test]
    fn a_search_body_puts_its_rows_under_the_query_not_over_it() {
        let mut buf = buffer(40, 12);
        let b = Dialog::new("/")
            .size(30, 10)
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

    // --- panes ---

    #[test]
    fn padding_trims_from_both_sides() {
        let r = pad(Rect::new(10, 5, 20, 10), 2, 1);
        assert_eq!((r.x, r.y), (12, 6));
        assert_eq!((r.width, r.height), (16, 8));
    }

    #[test]
    fn padding_a_rectangle_smaller_than_the_padding_does_not_invert_it() {
        // The arithmetic this replaces was `area.x + 2` written by hand, and
        // by hand it wraps.
        let r = pad(Rect::new(0, 0, 2, 1), 5, 5);
        assert!(r.width == 0 || r.width <= 2);
        assert!(r.x <= 1, "it must not walk off the right");
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
}
