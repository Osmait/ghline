//! Golden frames for diffline, on the fixture in `view::snapshot`.
//!
//! The same bargain as `frames.rs`, and here it is load bearing rather than
//! nice to have: the finder, the agent picker, the status bar and the header
//! are about to become components shared with github-tui, and these are the
//! frames that say the move changed nothing anybody can see.
//!
//! Accept a change with `cargo insta review` (or `INSTA_UPDATE=always cargo
//! test --test diffline_frames`) once you have read it.

use std::io;

use github_tui::diffline::view::snapshot;

/// Wide enough for three panes: the tree, the diff and the queue.
const WIDE: (u16, u16) = (160, 44);

fn frame(keys: &str) -> io::Result<String> {
    let (w, h) = WIDE;
    snapshot::frame(keys, w, h)
}

#[test]
fn the_review_as_it_opens() -> io::Result<()> {
    insta::assert_snapshot!(frame("")?);
    Ok(())
}

/// The diff is the pane with the most to get wrong: two number columns, a
/// sign column, and a comment marker in the gutter.
#[test]
fn the_second_file_selected() -> io::Result<()> {
    insta::assert_snapshot!(frame("j")?);
    Ok(())
}

/// Old on the left, new on the right — the same rows laid out twice over.
#[test]
fn the_split_diff() -> io::Result<()> {
    insta::assert_snapshot!(frame(" v")?);
    Ok(())
}

// --- the four that are about to move into `tui` ---

#[test]
fn the_finder() -> io::Result<()> {
    insta::assert_snapshot!(frame("/")?);
    Ok(())
}

#[test]
fn the_finder_with_a_query() -> io::Result<()> {
    insta::assert_snapshot!(frame("/sid")?);
    Ok(())
}

#[test]
fn the_agent_picker() -> io::Result<()> {
    insta::assert_snapshot!(frame(" a")?);
    Ok(())
}

#[test]
fn the_help() -> io::Result<()> {
    insta::assert_snapshot!(frame(" ?")?);
    Ok(())
}

/// Below 60x10 there is a notice instead of an interface.
#[test]
fn too_small_to_draw_anything() -> io::Result<()> {
    insta::assert_snapshot!(snapshot::frame("", 40, 8)?);
    Ok(())
}
