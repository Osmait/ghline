//! Bits of chrome more than one pane draws.

use ratatui::buffer::Buffer;
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::shared::theme;
use crate::tui::put;

/// Right-aligns a run of pieces that are not all the same colour.
///
/// `put_right` takes one style for the whole string, which is why the counts
/// were a flat grey: `+120 −80` is two facts and they are not the same fact.
/// Returns the left edge, as `put_right` does.
pub(super) fn put_right_parts(
    buf: &mut Buffer,
    right_x: u16,
    y: u16,
    parts: &[(&str, Style)],
) -> u16 {
    let total: u16 = parts.iter().map(|(t, _)| t.width() as u16).sum();
    let mut x = right_x.saturating_sub(total);
    let left = x;
    for (text, style) in parts {
        x = put(buf, x, y, right_x, text, *style);
    }
    left
}

/// Green for what arrived and red for what went, unless it is zero — a file
/// that added nothing should not be shouting green about it.
pub(super) fn count_style(base: Style, n: u32, added: bool) -> Style {
    if n == 0 {
        return base.fg(theme::dimmer());
    }
    base.fg(if added { theme::green() } else { theme::red() })
}

/// Width of the key column in the help. Named because two places depend on it
/// agreeing.
pub(super) const KEY_W: u16 = 14;
