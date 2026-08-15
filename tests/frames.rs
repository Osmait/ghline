//! Golden frames: the whole screen, compared character for character.
//!
//! The unit tests next to each view assert what that view is *about* — that a
//! count appears in the tab, that a long line stops at the pane's edge. That
//! is the right thing for them to say, and it is why they survive a redesign
//! of everything around the thing they name. It also means the layout can move
//! underneath them: a pane one column narrower, a column of a table lost, a
//! header that runs into the status on the right, and every one of those tests
//! still passes.
//!
//! These take the other half. They say nothing about intent and everything
//! about the pixels: any change to any of these frames fails, and the diff is
//! the screen before and after. Accept the new frame with `cargo insta review`
//! (or `INSTA_UPDATE=always cargo test --test frames`) once you have looked at
//! it — the looking is the test.
//!
//! An integration test rather than a unit one, so it goes through the same
//! public surface anything else depending on this crate would use.
//!
//! Determinism: `snapshot::demo` reads nobody's config and runs the design's
//! fixture, so there is no network, no clock and no `$HOME` in any of this.

use std::io;

use github_tui::github::snapshot;

/// The size the design was drawn at.
const WIDE: (u16, u16) = (160, 44);

/// Every test returns `io::Result` and reaches the frame through `?`. The
/// obvious `unwrap` is a clippy error here and correctly so: `clippy.toml`
/// allows unwrapping in tests, but that only reaches `#[cfg(test)]` modules,
/// and an integration test is its own crate with no such marking on it.
fn frame(keys: &str) -> io::Result<String> {
    let (w, h) = WIDE;
    snapshot::frame(keys, w, h, 0)
}

#[test]
fn pull_requests_is_what_it_opens_on() -> io::Result<()> {
    insta::assert_snapshot!(frame("")?);
    Ok(())
}

#[test]
fn the_issues_tab() -> io::Result<()> {
    insta::assert_snapshot!(frame("1")?);
    Ok(())
}

#[test]
fn the_actions_tab() -> io::Result<()> {
    insta::assert_snapshot!(frame("3")?);
    Ok(())
}

#[test]
fn the_files_tab() -> io::Result<()> {
    insta::assert_snapshot!(frame("4")?);
    Ok(())
}

#[test]
fn the_agents_tab() -> io::Result<()> {
    insta::assert_snapshot!(frame("5")?);
    Ok(())
}

/// The diff, which is the widest thing drawn and the one with the most to get
/// wrong: two columns of gutter, a split, and a scrollbar.
#[test]
fn the_diff_of_a_pull_request() -> io::Result<()> {
    insta::assert_snapshot!(frame("<enter>d")?);
    Ok(())
}

/// Modals are drawn over the panes they cover, and what shows around the edges
/// is part of the frame.
#[test]
fn the_help_modal() -> io::Result<()> {
    insta::assert_snapshot!(frame("?")?);
    Ok(())
}

#[test]
fn the_finder() -> io::Result<()> {
    insta::assert_snapshot!(frame("p")?);
    Ok(())
}

/// Below 90 columns the repository pane is meant to be gone. This is the
/// frame that says whether it is.
#[test]
fn narrow_enough_to_lose_the_repository_pane() -> io::Result<()> {
    insta::assert_snapshot!(snapshot::frame("", 88, 24, 0)?);
    Ok(())
}

/// And below 40x8 there is a notice instead of an interface. A terminal this
/// size is the one place the layout has no room to be wrong in, so it is the
/// one that would break silently.
#[test]
fn too_small_to_draw_anything() -> io::Result<()> {
    insta::assert_snapshot!(snapshot::frame("", 30, 6, 0)?);
    Ok(())
}
