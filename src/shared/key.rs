//! What was pressed, in this program's own words.
//!
//! The reducers used to take `crossterm::event::KeyEvent` directly, which
//! made the state layer's vocabulary the terminal library's. It worked, and
//! it meant three things that should not follow from each other: a keymap
//! could not be read without linking a terminal library, a test had to build
//! an event with `KeyEventKind` and `KeyEventState` fields it did not care
//! about, and any other source of input — a script, a remote, a replayed
//! recording — had to pretend to be a terminal to say "the reader pressed j".
//!
//! Translating at the edge costs one `match` per keystroke, which is a rate
//! measured in hands.

/// A key, as this program thinks of one.
///
/// Deliberately smaller than the terminal's list: there is no `Insert` here
/// because nothing is bound to it, and `Other` is what everything unmapped
/// becomes rather than a variant per key nobody presses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    /// Anything this program has no name for.
    Other,
}

/// A key with whatever was held down with it.
///
/// `shift` is not carried: a terminal reports a shifted letter as the capital
/// and a shifted digit as the symbol, so asking for it separately would give
/// two ways to spell the same press and one of them would be wrong.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Press {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
}

impl Press {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    pub fn ctrl(key: Key) -> Self {
        Self {
            key,
            ctrl: true,
            alt: false,
        }
    }

    /// The character this press produces, if it is one and nothing was held.
    ///
    /// What a modal typing into a query wants: `a` is a letter, `^a` is a
    /// command, and a modal that took both would put a control code in the
    /// search box.
    pub fn typed(self) -> Option<char> {
        match self.key {
            Key::Char(c) if !self.ctrl && !self.alt => Some(c),
            _ => None,
        }
    }
}

/// Which button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// What the mouse did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Down(Button),
    Up(Button),
    Drag(Button),
    ScrollUp,
    ScrollDown,
    /// The pointer moved with nothing pressed. Nothing here follows a
    /// pointer, so this exists to be ignored — but it is named rather than
    /// dropped silently, because "we chose not to" and "we forgot" look the
    /// same in code that has neither.
    Moved,
}

/// Where it happened, and what.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mouse {
    pub col: u16,
    pub row: u16,
    pub what: Motion,
}

/// Turns a written key sequence — `"jj<enter>k"` — into presses.
///
/// Here rather than with either program's snapshot code because it is about
/// keys and nothing else, and both programs write their headless renders in
/// this notation. It used to live in `github::snapshot`, which meant
/// diffline's binary reached across into github-tui to press a key.
///
/// Anything in angle brackets that is not a name below is taken literally,
/// character by character: an unknown `<foo>` is `<`, `f`, `o`, `o`, `>`.
pub fn parse_keys(spec: &str) -> Vec<Press> {
    let mut out = Vec::new();
    let mut rest = spec;
    while !rest.is_empty() {
        if let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
            let name = &rest[1..end + 1];
            let code = match name {
                "enter" => Some(Key::Enter),
                "esc" => Some(Key::Esc),
                "tab" => Some(Key::Tab),
                "bs" => Some(Key::Backspace),
                "down" => Some(Key::Down),
                "up" => Some(Key::Up),
                "left" => Some(Key::Left),
                "right" => Some(Key::Right),
                _ => None,
            };
            if let Some(c) = code {
                out.push(Press::new(c));
                rest = &rest[end + 2..];
                continue;
            }
        }
        let Some(c) = rest.chars().next() else { break };
        out.push(Press::new(Key::Char(c)));
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_letter_is_typed_and_a_control_chord_is_not() {
        assert_eq!(Press::new(Key::Char('a')).typed(), Some('a'));
        assert_eq!(Press::ctrl(Key::Char('a')).typed(), None);
        assert_eq!(Press::new(Key::Enter).typed(), None);
    }

    #[test]
    fn alt_is_not_typing_either() {
        let alt = Press {
            key: Key::Char('x'),
            ctrl: false,
            alt: true,
        };
        assert_eq!(alt.typed(), None, "a meta chord is a command, not a letter");
    }

    #[test]
    fn a_press_is_worth_comparing_and_hashing() {
        // The keymap looks chords up in a map, so this is not decoration.
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Press::new(Key::Char('j')));
        assert!(set.contains(&Press::new(Key::Char('j'))));
        assert!(!set.contains(&Press::ctrl(Key::Char('j'))));
    }
}
