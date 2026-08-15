//! What the keys mean, and where that can be changed.
//!
//! The keymap used to be the `match` inside `on_key`, which meant the key and
//! what it did were the same fact written once. Splitting them into a chord
//! and an action is what lets a file say `x = quit` — and it costs nothing at
//! read time, because a lookup happens once per keystroke and keystrokes
//! arrive at the speed of hands.

use std::collections::HashMap;

use crate::shared::key::{Key, Press};

use crate::diffline::app::Pending;
use crate::shared::nav::Dir;

/// Everything a key can be bound to.
///
/// Split into motions and commands because that is the division the modes are
/// built on — a motion also extends a selection in visual mode, a command
/// does not — but they share one namespace so a reader can bind either to
/// anything.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // --- motions ---
    LineDown,
    LineUp,
    Top,
    Bottom,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    ViewDown,
    ViewUp,
    HunkPrev,
    HunkNext,
    ChangePrev,
    ChangeNext,
    FilePrev,
    FileNext,
    ScopePrev,
    ScopeNext,
    CursorToMiddle,
    CursorToTop,
    CursorToBottom,
    ScrollLeft,
    ScrollRight,
    WordForward,
    WordEnd,
    WordBack,
    LineStart,
    FirstWord,
    LineEnd,
    PaneLeft,
    PaneRight,
    PaneNext,
    PanePrev,
    // --- search and modes ---
    Search,
    SearchNext,
    SearchPrev,
    Visual,
    OtherEnd,
    Cancel,
    // --- commands ---
    TreePane,
    QueuePane,
    CodePane,
    Note,
    DeleteNote,
    Agents,
    Send,
    Split,
    Blame,
    Deps,
    ContextMore,
    ContextLess,
    Refresh,
    Help,
    Themes,
    Commands,
    Quit,
    Redraw,
    Enter,
}

impl Action {
    /// The name a config file calls it. Kept apart from anything shown to a
    /// reader, so rewording the help cannot invalidate a keymap.
    pub fn name(self) -> &'static str {
        match self {
            Self::LineDown => "line-down",
            Self::LineUp => "line-up",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::ScreenTop => "screen-top",
            Self::ScreenMiddle => "screen-middle",
            Self::ScreenBottom => "screen-bottom",
            Self::HalfDown => "half-down",
            Self::HalfUp => "half-up",
            Self::PageDown => "page-down",
            Self::PageUp => "page-up",
            Self::ViewDown => "view-down",
            Self::ViewUp => "view-up",
            Self::HunkPrev => "hunk-prev",
            Self::HunkNext => "hunk-next",
            Self::ChangePrev => "change-prev",
            Self::ChangeNext => "change-next",
            Self::FilePrev => "file-prev",
            Self::FileNext => "file-next",
            Self::ScopePrev => "scope-prev",
            Self::ScopeNext => "scope-next",
            Self::CursorToMiddle => "cursor-to-middle",
            Self::CursorToTop => "cursor-to-top",
            Self::CursorToBottom => "cursor-to-bottom",
            Self::ScrollLeft => "scroll-left",
            Self::ScrollRight => "scroll-right",
            Self::WordForward => "word-forward",
            Self::WordEnd => "word-end",
            Self::WordBack => "word-back",
            Self::LineStart => "line-start",
            Self::FirstWord => "first-word",
            Self::LineEnd => "line-end",
            Self::PaneLeft => "pane-left",
            Self::PaneRight => "pane-right",
            Self::PaneNext => "pane-next",
            Self::PanePrev => "pane-prev",
            Self::Search => "search",
            Self::SearchNext => "search-next",
            Self::SearchPrev => "search-prev",
            Self::Visual => "visual",
            Self::OtherEnd => "other-end",
            Self::Cancel => "cancel",
            Self::TreePane => "tree-pane",
            Self::QueuePane => "queue-pane",
            Self::CodePane => "code-pane",
            Self::Note => "note",
            Self::DeleteNote => "delete-note",
            Self::Agents => "agents",
            Self::Send => "send",
            Self::Split => "split",
            Self::Blame => "blame",
            Self::Deps => "deps",
            Self::ContextMore => "context-more",
            Self::ContextLess => "context-less",
            Self::Refresh => "refresh",
            Self::Help => "help",
            Self::Themes => "themes",
            Self::Commands => "commands",
            Self::Quit => "quit",
            Self::Redraw => "redraw",
            Self::Enter => "enter",
        }
    }

    /// What it does, for the help. One line, lower case, no full stop.
    pub fn about(self) -> &'static str {
        match self {
            Self::LineDown => "line down",
            Self::LineUp => "line up",
            Self::Top => "first line",
            Self::Bottom => "last line",
            Self::ScreenTop => "top of the screen",
            Self::ScreenMiddle => "middle of the screen",
            Self::ScreenBottom => "bottom of the screen",
            Self::HalfDown => "half a screen down",
            Self::HalfUp => "half a screen up",
            Self::PageDown => "a screen down",
            Self::PageUp => "a screen up",
            Self::ViewDown => "scroll down, cursor stays",
            Self::ViewUp => "scroll up, cursor stays",
            Self::HunkPrev => "previous hunk",
            Self::HunkNext => "next hunk",
            Self::ChangePrev => "previous change",
            Self::ChangeNext => "next change",
            Self::FilePrev => "previous file",
            Self::FileNext => "next file",
            Self::ScopePrev => "previous scope",
            Self::ScopeNext => "next scope",
            Self::CursorToMiddle => "cursor line to the middle",
            Self::CursorToTop => "cursor line to the top",
            Self::CursorToBottom => "cursor line to the bottom",
            Self::ScrollLeft => "scroll the line left",
            Self::ScrollRight => "scroll the line right",
            Self::WordForward => "forward a word",
            Self::WordEnd => "to the end of a word",
            Self::WordBack => "back a word",
            Self::LineStart => "start of the line",
            Self::FirstWord => "first word of the line",
            Self::LineEnd => "end of the line",
            Self::PaneLeft => "pane left",
            Self::PaneRight => "pane right",
            Self::PaneNext => "next pane",
            Self::PanePrev => "previous pane",
            Self::Search => "search",
            Self::SearchNext => "next match",
            Self::SearchPrev => "previous match",
            Self::Visual => "visual line mode",
            Self::OtherEnd => "to the other end of the selection",
            Self::Cancel => "cancel the selection",
            Self::TreePane => "show or hide the file tree",
            Self::QueuePane => "show or hide the review queue",
            Self::CodePane => "back to the code",
            Self::Note => "note on the selection",
            Self::DeleteNote => "delete the note under the cursor",
            Self::Agents => "pick the target agent",
            Self::Send => "send the queue",
            Self::Split => "split or unified view",
            Self::Blame => "inline blame",
            Self::Deps => "blast radius",
            Self::ContextMore => "expand context",
            Self::ContextLess => "collapse context",
            Self::Refresh => "refresh",
            Self::Help => "this help",
            Self::Themes => "pick a theme",
            Self::Commands => "the command list",
            Self::Quit => "quit",
            Self::Redraw => "repaint the screen",
            Self::Enter => "into the pane / accept",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        ALL.iter().copied().find(|a| a.name() == name)
    }

    /// A motion moves and so also extends a selection; a command acts. The
    /// division is what makes visual mode work without a keymap of its own.
    pub fn is_motion(self) -> bool {
        matches!(
            self,
            Self::LineDown
                | Self::LineUp
                | Self::Top
                | Self::Bottom
                | Self::ScreenTop
                | Self::ScreenMiddle
                | Self::ScreenBottom
                | Self::HalfDown
                | Self::HalfUp
                | Self::PageDown
                | Self::PageUp
                | Self::HunkPrev
                | Self::HunkNext
                | Self::ChangePrev
                | Self::ChangeNext
                | Self::SearchNext
                | Self::SearchPrev
        )
    }
}

/// Every action, so that `from_name` and the help have one list to walk.
pub const ALL: &[Action] = &[
    Action::LineDown,
    Action::LineUp,
    Action::Top,
    Action::Bottom,
    Action::ScreenTop,
    Action::ScreenMiddle,
    Action::ScreenBottom,
    Action::HalfDown,
    Action::HalfUp,
    Action::PageDown,
    Action::PageUp,
    Action::ViewDown,
    Action::ViewUp,
    Action::HunkPrev,
    Action::HunkNext,
    Action::ChangePrev,
    Action::ChangeNext,
    Action::FilePrev,
    Action::FileNext,
    Action::ScopePrev,
    Action::ScopeNext,
    Action::CursorToMiddle,
    Action::CursorToTop,
    Action::CursorToBottom,
    Action::ScrollLeft,
    Action::ScrollRight,
    Action::WordForward,
    Action::WordEnd,
    Action::WordBack,
    Action::LineStart,
    Action::FirstWord,
    Action::LineEnd,
    Action::PaneLeft,
    Action::PaneRight,
    Action::PaneNext,
    Action::PanePrev,
    Action::Search,
    Action::SearchNext,
    Action::SearchPrev,
    Action::Visual,
    Action::OtherEnd,
    Action::Cancel,
    Action::TreePane,
    Action::QueuePane,
    Action::CodePane,
    Action::Note,
    Action::DeleteNote,
    Action::Agents,
    Action::Send,
    Action::Split,
    Action::Blame,
    Action::Deps,
    Action::ContextMore,
    Action::ContextLess,
    Action::Refresh,
    Action::Help,
    Action::Themes,
    Action::Commands,
    Action::Quit,
    Action::Redraw,
    Action::Enter,
];

/// One keystroke, in the state the keymap sees it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Chord {
    pub prefix: Pending,
    pub press: Press,
}

impl Chord {
    pub fn plain(key: Key) -> Self {
        Self {
            prefix: Pending::None,
            press: Press::new(key),
        }
    }

    /// How a config file would have to write this chord. Used by the help, so
    /// that what it prints is what you would type to change it.
    pub fn spec(self) -> String {
        let key = match self.press.key {
            Key::Char(' ') => "<space>".to_string(),
            Key::Char(c) => c.to_string(),
            Key::Esc => "<esc>".into(),
            Key::Enter => "<cr>".into(),
            Key::Tab => "<tab>".into(),
            Key::BackTab => "<s-tab>".into(),
            Key::Backspace => "<bs>".into(),
            Key::Delete => "<del>".into(),
            Key::Up => "<up>".into(),
            Key::Down => "<down>".into(),
            Key::Left => "<left>".into(),
            Key::Right => "<right>".into(),
            Key::Home => "<home>".into(),
            Key::End => "<end>".into(),
            Key::PageUp => "<pageup>".into(),
            Key::PageDown => "<pagedown>".into(),
            Key::Other => "?".into(),
        };
        let key = if self.press.ctrl {
            format!("<C-{}>", key.trim_matches(|c| c == '<' || c == '>'))
        } else {
            key
        };
        match self.prefix {
            Pending::None => key,
            Pending::Leader => format!("<leader>{key}"),
            Pending::G => format!("g{key}"),
            Pending::Z => format!("z{key}"),
            Pending::Bracket(Dir::Prev) => format!("[{key}"),
            Pending::Bracket(Dir::Next) => format!("]{key}"),
        }
    }
}

/// Reads a chord out of a config file.
///
/// `j`, `<C-d>`, `<leader>n`, `gg`, `]c`, `<esc>`. Returns `None` for anything
/// it does not recognise rather than guessing, so a typo is a line that did
/// nothing instead of a key that does the wrong thing.
pub fn parse_chord(spec: &str) -> Option<Chord> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }

    // `<leader>` first: what follows it is itself a chord, minus the prefix.
    if let Some(rest) = spec
        .strip_prefix("<leader>")
        .or_else(|| spec.strip_prefix("<Leader>"))
    {
        let mut c = parse_chord(rest)?;
        c.prefix = Pending::Leader;
        return Some(c);
    }

    // A named key, or a control chord.
    if spec.starts_with('<') && spec.ends_with('>') {
        let inner = &spec[1..spec.len() - 1];
        if let Some(c) = inner
            .strip_prefix("C-")
            .or_else(|| inner.strip_prefix("c-"))
        {
            let mut ch = parse_chord(c)?;
            ch.press.ctrl = true;
            return Some(ch);
        }
        let key = match inner.to_ascii_lowercase().as_str() {
            "esc" => Key::Esc,
            "cr" | "enter" | "return" => Key::Enter,
            "tab" => Key::Tab,
            "s-tab" => Key::BackTab,
            "bs" | "backspace" => Key::Backspace,
            "del" | "delete" => Key::Delete,
            "space" => Key::Char(' '),
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            _ => return None,
        };
        return Some(Chord::plain(key));
    }

    let mut chars = spec.chars();
    let first = chars.next()?;
    let rest: String = chars.collect();

    // Two keys where the first opens an alphabet: `gg`, `zt`, `]c`.
    if !rest.is_empty() {
        let prefix = match first {
            'g' => Pending::G,
            'z' => Pending::Z,
            '[' => Pending::Bracket(Dir::Prev),
            ']' => Pending::Bracket(Dir::Next),
            _ => return None,
        };
        let mut c = parse_chord(&rest)?;
        c.prefix = prefix;
        return Some(c);
    }

    Some(Chord::plain(Key::Char(first)))
}

/// The keymap as it ships, written the way a config file would write it.
///
/// One table rather than a `match`, so that the help, the config reader and
/// the dispatcher all read the same thing and cannot disagree about it.
pub const DEFAULTS: &[(&str, Action)] = &[
    // motions
    ("j", Action::LineDown),
    ("<down>", Action::LineDown),
    ("k", Action::LineUp),
    ("<up>", Action::LineUp),
    ("gg", Action::Top),
    ("gj", Action::LineDown),
    ("gk", Action::LineUp),
    ("G", Action::Bottom),
    ("H", Action::ScreenTop),
    ("M", Action::ScreenMiddle),
    ("L", Action::ScreenBottom),
    ("<C-d>", Action::HalfDown),
    ("<C-u>", Action::HalfUp),
    ("<C-f>", Action::PageDown),
    ("<C-b>", Action::PageUp),
    ("<C-e>", Action::ViewDown),
    ("<C-y>", Action::ViewUp),
    ("<pagedown>", Action::PageDown),
    ("<pageup>", Action::PageUp),
    ("{", Action::HunkPrev),
    ("}", Action::HunkNext),
    ("[c", Action::ChangePrev),
    ("]c", Action::ChangeNext),
    ("[f", Action::FilePrev),
    ("]f", Action::FileNext),
    ("[s", Action::ScopePrev),
    ("]s", Action::ScopeNext),
    ("zz", Action::CursorToMiddle),
    ("zt", Action::CursorToTop),
    ("zb", Action::CursorToBottom),
    ("h", Action::ScrollLeft),
    ("<left>", Action::ScrollLeft),
    ("l", Action::ScrollRight),
    ("<right>", Action::ScrollRight),
    ("w", Action::WordForward),
    ("e", Action::WordEnd),
    ("b", Action::WordBack),
    ("0", Action::LineStart),
    ("^", Action::FirstWord),
    ("$", Action::LineEnd),
    ("<tab>", Action::PaneNext),
    ("<s-tab>", Action::PanePrev),
    // search and modes
    ("/", Action::Search),
    ("n", Action::SearchNext),
    ("N", Action::SearchPrev),
    ("v", Action::Visual),
    ("V", Action::Visual),
    ("o", Action::OtherEnd),
    ("<esc>", Action::Cancel),
    (":", Action::Commands),
    ("<cr>", Action::Enter),
    ("<C-l>", Action::Redraw),
    // commands, behind the leader
    ("<leader>e", Action::TreePane),
    ("<leader>c", Action::QueuePane),
    ("<leader>d", Action::CodePane),
    ("<leader>n", Action::Note),
    ("<leader>x", Action::DeleteNote),
    ("<leader>a", Action::Agents),
    ("<leader>s", Action::Send),
    ("<leader>v", Action::Split),
    ("<leader>b", Action::Blame),
    ("<leader>g", Action::Deps),
    ("<leader>+", Action::ContextMore),
    ("<leader>=", Action::ContextMore),
    ("<leader>-", Action::ContextLess),
    ("<leader>r", Action::Refresh),
    ("<leader>t", Action::Themes),
    ("<leader>?", Action::Help),
    ("<leader>q", Action::Quit),
];

/// A keymap: the defaults, with whatever the reader's file says over the top.
pub struct Map {
    pub binds: HashMap<Chord, Action>,
    /// Lines the file got wrong, kept so the program can say so rather than
    /// leaving someone to wonder why their key does nothing.
    pub problems: Vec<String>,
}

impl Default for Map {
    fn default() -> Self {
        Self::new()
    }
}

impl Map {
    /// The shipped keymap, with no file read.
    pub fn new() -> Self {
        let mut binds = HashMap::new();
        for (spec, action) in DEFAULTS {
            if let Some(c) = parse_chord(spec) {
                binds.insert(c, *action);
            }
        }
        Self {
            binds,
            problems: Vec::new(),
        }
    }

    /// The shipped keymap with `text` applied over it.
    pub fn with(text: &str) -> Self {
        let mut map = Self::new();
        map.apply(text);
        map
    }

    /// `chord = action`, one per line, `#` starts a comment.
    ///
    /// `action` of `none` unbinds, which is the only way to get a key back
    /// that the defaults have taken.
    pub fn apply(&mut self, text: &str) {
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or(line).trim();
            if line.is_empty() {
                continue;
            }
            let Some((spec, name)) = line.split_once('=') else {
                self.problems.push(format!("line {}: no `=`", n + 1));
                continue;
            };
            let (spec, name) = (spec.trim(), name.trim());
            let Some(chord) = parse_chord(spec) else {
                self.problems
                    .push(format!("line {}: {spec} is not a key", n + 1));
                continue;
            };
            if name == "none" {
                self.binds.remove(&chord);
                continue;
            }
            let Some(action) = Action::from_name(name) else {
                self.problems
                    .push(format!("line {}: no action called {name}", n + 1));
                continue;
            };
            self.binds.insert(chord, action);
        }
    }

    pub fn get(&self, chord: Chord) -> Option<Action> {
        self.binds.get(&chord).copied()
    }

    /// True when this key opens an alphabet rather than doing something —
    /// which is to say something is bound behind it. Computed rather than
    /// fixed, so unbinding every `]x` stops `]` swallowing a keystroke.
    pub fn is_prefix(&self, key: Key) -> bool {
        let want = match key {
            Key::Char(' ') => Pending::Leader,
            Key::Char('g') => Pending::G,
            Key::Char('z') => Pending::Z,
            Key::Char('[') => Pending::Bracket(Dir::Prev),
            Key::Char(']') => Pending::Bracket(Dir::Next),
            _ => return false,
        };
        self.binds.keys().any(|c| c.prefix == want)
    }

    /// Every binding, sorted for the help.
    pub fn listing(&self) -> Vec<(String, Action)> {
        let mut v: Vec<(String, Action)> = self.binds.iter().map(|(c, a)| (c.spec(), *a)).collect();
        v.sort_by(|a, b| a.1.name().cmp(b.1.name()).then(a.0.cmp(&b.0)));
        v
    }
}

/// `<config>/keys`, beside the config and the themes directory.
pub fn path() -> Option<std::path::PathBuf> {
    Some(crate::shared::config::path()?.with_file_name("keys"))
}

/// The reader's keymap, or the shipped one if they have not written a file.
pub fn load() -> Map {
    match path().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => Map::with(&text),
        None => Map::new(),
    }
}

/// The shipped keymap, written out as a file to start from.
///
/// The text, not the writing: what belongs in a keymap template is a fact
/// about the keymap, and putting bytes on a disk is not something the state
/// layer does. The caller hands both to the worker.
pub fn template() -> String {
    let mut out = String::from("# diffline keys\n#\n");
    out.push_str("# Every binding as it ships. Change the key, or write\n");
    out.push_str("# `<key> = none` to take one away.\n#\n");
    out.push_str("# Keys: a letter, `<C-d>`, `<leader>x`, `gg`, `]c`, `<esc>`,\n");
    out.push_str("# `<cr>`, `<tab>`, `<s-tab>`, `<space>`, the arrows.\n\n");

    let width = DEFAULTS.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    for (spec, action) in DEFAULTS {
        out.push_str(&format!(
            "{spec:<width$} = {:<18} # {}\n",
            action.name(),
            action.about(),
            width = width
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_binding_parses() {
        // A default that does not parse is a key that silently does nothing,
        // and nothing else would catch it.
        for (spec, action) in DEFAULTS {
            assert!(
                parse_chord(spec).is_some(),
                "{spec} ({}) is not a key anything can read",
                action.name()
            );
        }
    }

    #[test]
    fn every_action_has_a_name_that_reads_back() {
        for a in ALL {
            assert_eq!(Action::from_name(a.name()), Some(*a));
        }
    }

    #[test]
    fn no_two_actions_share_a_name() {
        let mut names: Vec<&str> = ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two actions answer to one name");
    }

    #[test]
    fn a_spec_survives_the_round_trip() {
        // The help prints `spec()`, and what it prints has to be what you
        // would type to change it.
        for spec in [
            "j",
            "G",
            "<C-d>",
            "<leader>n",
            "gg",
            "]c",
            "[f",
            "zt",
            "<esc>",
            "<tab>",
            "<s-tab>",
            "<cr>",
            "<space>",
            "0",
            "$",
        ] {
            let c = parse_chord(spec).unwrap_or_else(|| panic!("{spec} did not parse"));
            assert_eq!(c.spec(), spec, "{spec} did not survive");
        }
    }

    #[test]
    fn a_file_overrides_and_unbinds() {
        let m = Map::with("x = quit\nj = none\n");
        assert_eq!(m.get(parse_chord("x").unwrap()), Some(Action::Quit));
        assert_eq!(m.get(parse_chord("j").unwrap()), None, "j was taken away");
        assert_eq!(
            m.get(parse_chord("k").unwrap()),
            Some(Action::LineUp),
            "and the rest stands"
        );
        assert!(m.problems.is_empty());
    }

    #[test]
    fn a_bad_line_is_reported_rather_than_guessed_at() {
        let m = Map::with("j = fly\n<nope> = quit\nk\n# a comment\n");
        assert_eq!(m.problems.len(), 3, "{:?}", m.problems);
        assert_eq!(
            m.get(parse_chord("j").unwrap()),
            Some(Action::LineDown),
            "a line that made no sense must not take the key with it"
        );
    }

    #[test]
    fn a_prefix_is_a_prefix_only_while_something_lives_behind_it() {
        let m = Map::new();
        assert!(m.is_prefix(Key::Char(']')));
        let m = Map::with("]c = none\n]f = none\n]s = none\n");
        assert!(
            !m.is_prefix(Key::Char(']')),
            "with nothing behind it, ] must not swallow the next key"
        );
    }

    #[test]
    fn a_comment_does_not_take_the_binding_with_it() {
        let m = Map::with("x = quit  # because I keep hitting it\n");
        assert_eq!(m.get(parse_chord("x").unwrap()), Some(Action::Quit));
    }
}
