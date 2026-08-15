//! What a click can land on, in diffline.
//!
//! The geometry is `tui::hit`, shared with the other program; this is only
//! the list of things that exist here. The renderer is the one part that
//! knows how wide the tree ended up and which rows survived the scroll, so it
//! records what it drew rather than the input layer working it out again and
//! drifting the first time a pane changes.

use super::app::Pane;

/// What a click can land on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A pane of the interface proper.
    Pane(Pane),
    /// One of the scope tabs in the header.
    Scope(usize),
    /// A row of a modal. While one is up it owns the keyboard, and it owns
    /// the mouse on the same terms.
    Modal,
    /// The tab that shows the queue count while the queue is hidden.
    QueueTab,
}

pub type Region = crate::tui::hit::Region<Target>;
