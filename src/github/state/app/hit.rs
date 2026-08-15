//! What a click can land on, in this program.
//!
//! The geometry — which row of which rectangle, through what scroll — is in
//! `tui::hit`, generic over the target, because it is the same arithmetic in
//! both programs. Only the list of things that can be hit is ours.

use super::Pane;

/// What a click can land on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// A pane of the interface proper.
    Pane(Pane),
    /// One tab of the tab bar, by index into `data::TABS`.
    Tab(usize),
    /// A row of a modal. These are not panes: while one is up it owns the
    /// keyboard, and it owns the mouse on the same terms.
    Finder,
    Themes,
    Accounts,
    Dispatch,
}

pub type Region = crate::tui::hit::Region<Target>;
