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
///
/// Each variant below is annotated with how it is written in the `<name>`
/// notation `parse_keys` reads and `Press::spell` writes — the form a keymap
/// file, a `--log` recording and a `--snapshot` script are all in. Several of
/// the abbreviations are not guessable from the variant name, which is the
/// only reason these lines are here.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    /// A character, already shifted by the terminal: `A` arrives as `A`, not
    /// as shift plus `a`. Spells as itself, except `<` and `>`, which have to
    /// be named so a chord over one does not eat its own bracket.
    Char(char),
    /// `<enter>`.
    Enter,
    /// `<esc>`.
    Esc,
    /// `<tab>`.
    Tab,
    /// `<btab>` — shift-tab, which terminals report as a key of its own
    /// rather than as tab with a modifier.
    BackTab,
    /// `<bs>`.
    Backspace,
    /// `<del>` — forward delete, not backspace.
    Delete,
    /// `<up>`.
    Up,
    /// `<down>`.
    Down,
    /// `<left>`.
    Left,
    /// `<right>`.
    Right,
    /// `<home>`.
    Home,
    /// `<end>`.
    End,
    /// `<pgup>`.
    PageUp,
    /// `<pgdn>`.
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
    /// Which key, with the modifiers lifted out into the two flags below.
    pub key: Key,
    /// Control was held. A control chord is a command and never text, which
    /// is the rule `typed` exists to enforce.
    pub ctrl: bool,
    /// Alt — Meta on some keyboards — was held. Nothing is bound to an alt
    /// chord today; it is carried because the terminal reports it, and a
    /// modifier silently dropped would make `<a-x>` mean plain `x`.
    pub alt: bool,
}

impl Press {
    /// A press with nothing held down.
    ///
    /// The common case by a wide margin, so it is the short name: most of a
    /// keymap is bare letters.
    pub fn new(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    /// A press with control held down.
    ///
    /// There is no `alt` twin, because nothing is bound to an alt chord — the
    /// struct literal is fine for the handful of tests that build one.
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

    /// This press written the way `parse_keys` reads it.
    ///
    /// The inverse, so that a session can be written down and played back:
    /// `--log` records what was pressed in this notation, and what it records
    /// is what `--snapshot` takes. A bug report becomes a frame.
    ///
    /// `Key::Other` is the one press that cannot survive the trip, because it
    /// is already the name for "we have no name for this" — it spells as
    /// `<?>`, which reads back as three characters, and a replay that reached
    /// one would be replaying a key nothing is bound to anyway.
    pub fn spell(self) -> String {
        let name = match self.key {
            // The two the notation is built out of. `<` because it opens a
            // name, and `>` because it closes one — `<a->` reads as four
            // literal characters, since the `>` meant as the key is taken as
            // the bracket first. Both are named instead.
            Key::Char('<') => "<lt>".to_string(),
            Key::Char('>') => "<gt>".into(),
            Key::Char(c) => c.to_string(),
            Key::Enter => "<enter>".into(),
            Key::Esc => "<esc>".into(),
            Key::Tab => "<tab>".into(),
            Key::BackTab => "<btab>".into(),
            Key::Backspace => "<bs>".into(),
            Key::Delete => "<del>".into(),
            Key::Up => "<up>".into(),
            Key::Down => "<down>".into(),
            Key::Left => "<left>".into(),
            Key::Right => "<right>".into(),
            Key::Home => "<home>".into(),
            Key::End => "<end>".into(),
            Key::PageUp => "<pgup>".into(),
            Key::PageDown => "<pgdn>".into(),
            Key::Other => "<?>".into(),
        };
        match (self.ctrl, self.alt) {
            (false, false) => name,
            // The modifier wraps whatever the key spelled as, including the
            // angle brackets: `<c-<up>>` would be unreadable, so a named key
            // loses its own brackets inside a chord.
            (c, a) => {
                let inner = name.trim_start_matches('<').trim_end_matches('>');
                let mods = match (c, a) {
                    (true, true) => "c-a-",
                    (true, false) => "c-",
                    _ => "a-",
                };
                format!("<{mods}{inner}>")
            }
        }
    }
}

/// Which button.
///
/// All three are translated at the terminal edge, but only `Left` is bound:
/// the others reach `--log` and nothing else. They are named rather than
/// folded into `Left` so a recorded session says which button was pressed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    /// The primary button — every click either program acts on.
    Left,
    /// Unbound. There is no context menu to open.
    Right,
    /// Unbound. On many terminals this is the X11 paste, which the terminal
    /// turns into typed characters before this layer sees anything.
    Middle,
}

/// What the mouse did.
///
/// Three of these are acted on — a left press and the two wheel directions.
/// The rest arrive, are recorded by `--log`, and are ignored, which is the
/// same reason `Moved` is spelt out below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    /// A button went down. Both programs act on the press rather than the
    /// release, so this is what selects.
    Down(Button),
    /// A button came back up.
    Up(Button),
    /// Moved with a button held. Nothing here supports a drag — no selection,
    /// no resize handle — so this is `Up`'s neighbour in being reported only.
    Drag(Button),
    /// One wheel notch away from the reader. Each program decides how many
    /// rows a notch is worth; the event carries no magnitude.
    ScrollUp,
    /// One wheel notch towards the reader.
    ScrollDown,
    /// The pointer moved with nothing pressed. Nothing here follows a
    /// pointer, so this exists to be ignored — but it is named rather than
    /// dropped silently, because "we chose not to" and "we forgot" look the
    /// same in code that has neither.
    Moved,
}

/// Where it happened, and what.
///
/// The position is in terminal cells, counted from zero at the top left of
/// the whole screen — never from the corner of a pane. Turning that into
/// "which row of which list" is the hit-testing each program does for itself,
/// and doing it here would need this module to know what a pane is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mouse {
    /// Cells from the left edge of the terminal.
    pub col: u16,
    /// Cells from the top edge of the terminal.
    pub row: u16,
    /// What the mouse did there.
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
///
/// A leading `c-` or `a-` inside the brackets is the modifier: `<c-a>` is
/// control-a and `<c-up>` is control-up. `<lt>` and `<gt>` are the two
/// characters this notation is made of, spelt out so that a chord over one of
/// them does not eat its own bracket. Together with `Press::spell` this reads
/// back everything it writes.
pub fn parse_keys(spec: &str) -> Vec<Press> {
    let mut out = Vec::new();
    let mut rest = spec;
    while !rest.is_empty() {
        if let Some(end) = rest.strip_prefix('<').and_then(|r| r.find('>')) {
            let name = &rest[1..end + 1];
            // Both, in either order, before the key itself.
            let (ctrl, name) = match name.strip_prefix("c-") {
                Some(n) => (true, n),
                None => (false, name),
            };
            let (alt, name) = match name.strip_prefix("a-") {
                Some(n) => (true, n),
                None => (false, name),
            };
            let code = match name {
                "enter" => Some(Key::Enter),
                "esc" => Some(Key::Esc),
                "tab" => Some(Key::Tab),
                "btab" => Some(Key::BackTab),
                "bs" => Some(Key::Backspace),
                "del" => Some(Key::Delete),
                "down" => Some(Key::Down),
                "up" => Some(Key::Up),
                "left" => Some(Key::Left),
                "right" => Some(Key::Right),
                "home" => Some(Key::Home),
                "end" => Some(Key::End),
                "pgup" => Some(Key::PageUp),
                "pgdn" => Some(Key::PageDown),
                "lt" => Some(Key::Char('<')),
                "gt" => Some(Key::Char('>')),
                // A chord over an ordinary letter: `<c-a>`. Only when a
                // modifier was given, so a bare `<a>` stays the three
                // characters it has always been.
                _ if (ctrl || alt) && name.chars().count() == 1 => {
                    name.chars().next().map(Key::Char)
                }
                _ => None,
            };
            if let Some(c) = code {
                out.push(Press { key: c, ctrl, alt });
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
    fn what_is_spelt_is_what_is_read_back() {
        // The point of the pair: `--log` writes with one and `--snapshot`
        // reads with the other, so a session recorded is a session replayed.
        let every = [
            Press::new(Key::Char('j')),
            Press::new(Key::Char('<')),
            Press::new(Key::Char('漢')),
            Press::new(Key::Enter),
            Press::new(Key::PageDown),
            Press::new(Key::BackTab),
            Press::ctrl(Key::Char('c')),
            Press::ctrl(Key::Up),
            Press {
                key: Key::Char('x'),
                ctrl: false,
                alt: true,
            },
            Press {
                key: Key::Delete,
                ctrl: true,
                alt: true,
            },
        ];
        for p in every {
            assert_eq!(
                parse_keys(&p.spell()),
                vec![p],
                "{} did not survive",
                p.spell()
            );
        }
        // and a whole session at once, which is how it is actually used
        let session: String = every.iter().map(|p| p.spell()).collect();
        assert_eq!(parse_keys(&session), every.to_vec());
    }

    #[test]
    fn an_unknown_name_is_still_read_literally() {
        // The old behaviour, which the modifiers must not have taken away:
        // `<foo>` is five characters, and so is a bare `<a>`.
        assert_eq!(parse_keys("<foo>").len(), 5);
        assert_eq!(parse_keys("<a>").len(), 3, "a bare letter is not a chord");
        assert_eq!(parse_keys("<c-a>"), vec![Press::ctrl(Key::Char('a'))]);
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
