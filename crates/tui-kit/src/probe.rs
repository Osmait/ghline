//! Reading a drawn frame back, for tests.
//!
//! A view test draws into a `TestBackend` and then asks what is on the screen.
//! Asking meant the same twelve lines — walk the buffer, take each cell's
//! symbol, join the row — written out again in every file that has view tests:
//! four copies of one loop, differing in nothing.
//!
//! Public rather than `#[cfg(test)]`, because two things outside the unit
//! tests need it: the golden frames in `tests/`, which are a separate crate
//! and cannot see a test-only item, and `ghline_app::snapshot::frame`. The
//! `--snapshot` SVG paths do not — they want the colours too, and turning a
//! buffer into SVG is a different job that happens to start at the same
//! buffer.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// A blank buffer of exactly this size, for a component test.
///
/// A component below the `Frame` level takes a `Buffer` and a `Rect`, so its
/// tests never build a terminal at all. Four of the level modules wanted this
/// same two-line constructor.
pub fn buffer(w: u16, h: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, w, h))
}

/// One row of a buffer, trailing blanks trimmed.
///
/// Trimmed, unlike `rows` above, because a component test is usually asking
/// what a row *says*: the padding to the right of it is the pane's business
/// and not the component's.
pub fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Every row of the drawn frame, one string each, with trailing blanks kept.
///
/// The blanks are kept because a test about layout is often about them — a
/// pane that ends early and a pane that fills its width differ by nothing
/// else.
pub fn rows(term: &Terminal<TestBackend>) -> Vec<String> {
    let buf = term.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
                .collect()
        })
        .collect()
}

/// The whole frame as one string, rows separated by newlines.
///
/// What most tests want: `assert!(screen(&term).contains(…))`, and the same
/// string is what they print when they fail, so a failure shows the screen it
/// was looking at.
pub fn screen(term: &Terminal<TestBackend>) -> String {
    rows(term).join("\n")
}
