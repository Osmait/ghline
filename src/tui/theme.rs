//! Colours and glyphs.
//!
//! The design's palette is one of several. A theme is a whole `Palette`, and
//! switching one in is a single store, so the change lands on the very next
//! frame — there is nothing to rebuild or re-fetch.
//!
//! Call sites read through the accessors below rather than naming a constant,
//! which is what makes the swap possible at all.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Every colour the interface draws with, in the roles the design defines.
#[derive(Clone, Copy)]
pub struct Palette {
    // surfaces
    pub bg: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub tab_active_bg: Color,
    pub border: Color,
    pub border_soft: Color,
    pub sel: Color,
    pub sel_mark_idle: Color,
    pub err_bg: Color,
    pub diff_add_bg: Color,
    pub diff_del_bg: Color,
    pub diff_void_bg: Color,
    // text
    pub fg: Color,
    pub bright: Color,
    pub body: Color,
    pub step_fg: Color,
    pub log_fg: Color,
    pub dim: Color,
    pub dimmer: Color,
    pub dimmest: Color,
    pub gutter: Color,
    // accents
    pub cyan: Color,
    pub cyan_soft: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub purple: Color,
    pub orange: Color,
}

/// The palette of `GitHub TUI.dc.html`, which everything else was drawn to
/// match.
const DESIGN: Palette = Palette {
    bg: rgb(0x000b0e14),
    panel: rgb(0x0010141c),
    panel_alt: rgb(0x000d1017),
    tab_active_bg: rgb(0x00151b25),
    border: rgb(0x00232936),
    border_soft: rgb(0x001a1f29),
    sel: rgb(0x001b2330),
    sel_mark_idle: rgb(0x002c3444),
    err_bg: rgb(0x001f1418),
    diff_add_bg: rgb(0x000f2418),
    diff_del_bg: rgb(0x0026141a),
    diff_void_bg: rgb(0x000e1219),
    fg: rgb(0x00c5cdd9),
    bright: rgb(0x00e6e1cf),
    body: rgb(0x00a9b2bf),
    step_fg: rgb(0x009aa4b2),
    log_fg: rgb(0x00b8c0cc),
    dim: rgb(0x005c6773),
    dimmer: rgb(0x004d5566),
    dimmest: rgb(0x00565b66),
    gutter: rgb(0x00323947),
    cyan: rgb(0x0039bae6),
    cyan_soft: rgb(0x0073d0ff),
    green: rgb(0x007fd962),
    yellow: rgb(0x00ffb454),
    red: rgb(0x00f07178),
    purple: rgb(0x00d2a6ff),
    orange: rgb(0x00ff8f40),
};

/// Catppuccin Mocha.
///
/// `base` is the ground, because that is what a Catppuccin terminal paints
/// behind everything — anything else leaves a visible seam where the interface
/// meets the rest of the screen.
///
/// That inverts the design's own relationship, where panels sit a shade
/// *lighter* than the background. Catppuccin goes the other way: its editors
/// put the sidebar and the status bar on `mantle`, darker than the content. The
/// flavour's convention wins here, since the reason to pick it is that
/// everything else on screen already follows it.
const MOCHA: Palette = Palette {
    bg: rgb(0x001e1e2e),            // base, the terminal's own background
    panel: rgb(0x00181825),         // mantle, for headers and status bars
    panel_alt: rgb(0x0011111b),     // crust, for the sidebar and the trees
    tab_active_bg: rgb(0x001e1e2e), // the active tab rises back to base
    border: rgb(0x00313244),        // surface0
    border_soft: rgb(0x0026273a),
    sel: rgb(0x00313244),           // surface0
    sel_mark_idle: rgb(0x00585b70), // surface2
    err_bg: rgb(0x00302031),
    diff_add_bg: rgb(0x00203230),
    diff_del_bg: rgb(0x00352334),
    diff_void_bg: rgb(0x00191926),
    fg: rgb(0x00cdd6f4),      // text
    bright: rgb(0x00f5e0dc),  // rosewater
    body: rgb(0x00bac2de),    // subtext1
    step_fg: rgb(0x00a6adc8), // subtext0
    log_fg: rgb(0x00c0c8e0),
    dim: rgb(0x007f849c),    // overlay1
    dimmer: rgb(0x006c7086), // overlay0
    dimmest: rgb(0x005f6377),
    gutter: rgb(0x00585b70),    // surface2
    cyan: rgb(0x0089b4fa),      // blue
    cyan_soft: rgb(0x0089dceb), // sky
    green: rgb(0x00a6e3a1),
    yellow: rgb(0x00f9e2af),
    red: rgb(0x00f38ba8),
    purple: rgb(0x00cba6f7), // mauve
    orange: rgb(0x00fab387), // peach
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Design,
    Mocha,
    /// One the reader wrote, by index into `custom()`.
    Custom(usize),
}

impl Theme {
    /// The two built in, and whatever is in the themes directory.
    pub fn all() -> Vec<Self> {
        let mut v = vec![Self::Design, Self::Mocha];
        v.extend((0..custom().len()).map(Self::Custom));
        v
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Mocha => "catppuccin mocha",
            Self::Custom(i) => custom().get(i).map_or("?", |c| c.name),
        }
    }

    /// The name this theme is stored under. Kept apart from `name()` so that
    /// rewording the picker cannot invalidate everyone's saved config.
    pub fn key(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Mocha => "mocha",
            Self::Custom(i) => custom().get(i).map_or("?", |c| c.key),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().into_iter().find(|t| t.key() == key)
    }

    /// One line about where the palette comes from, for the picker.
    pub fn about(self) -> &'static str {
        match self {
            Self::Design => "the palette of GitHub TUI.dc.html",
            Self::Mocha => "catppuccin.com · the darkest of the four flavours",
            Self::Custom(_) => "yours, from the themes directory",
        }
    }

    fn palette(self) -> &'static Palette {
        match self {
            Self::Design => &DESIGN,
            Self::Mocha => &MOCHA,
            Self::Custom(i) => custom().get(i).map_or(&MOCHA, |c| &c.palette),
        }
    }
}

// --- themes from disk -------------------------------------------------------

/// A theme somebody wrote, read out of the themes directory.
pub struct Custom {
    pub name: &'static str,
    pub key: &'static str,
    pub palette: Palette,
}

/// The custom themes, read once.
///
/// Once and not on every frame: a palette lookup happens per cell, and this
/// is a directory listing. Nothing here can change while the program runs
/// except by the reader editing a file, and `r` is not a thing they can press
/// at a colour.
static CUSTOM: OnceLock<Vec<Custom>> = OnceLock::new();

pub fn custom() -> &'static [Custom] {
    CUSTOM.get_or_init(load_custom)
}

/// Where a theme file goes: `<config>/themes/<name>.theme`.
pub fn dir() -> Option<std::path::PathBuf> {
    crate::shared::settings::path_beside("themes")
}

fn load_custom() -> Vec<Custom> {
    let Some(dir) = dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        // No directory is the normal case, not a failure worth reporting.
        return Vec::new();
    };

    let mut out: Vec<Custom> = Vec::new();
    let mut paths: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "theme"))
        .collect();
    // Read in a stable order, or the picker would shuffle between runs.
    paths.sort();

    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Leaked because a `Theme` is `Copy` and hands out `&'static str`.
        // These are read once at startup and live as long as the process, so
        // the leak is the lifetime and not a mistake.
        let key: &'static str = Box::leak(stem.to_string().into_boxed_str());
        out.push(Custom {
            name: key,
            key,
            palette: parse_palette(&text),
        });
    }
    out
}

/// Reads a theme file over the top of Mocha.
///
/// Over the top rather than from nothing: a theme that names three colours
/// should give you those three and a working interface, not three colours and
/// twenty-six holes. Anything unnamed keeps the value Mocha gives it.
pub fn parse_palette(text: &str) -> Palette {
    let mut p = MOCHA;
    for line in text.lines() {
        // A `#` only starts a comment at the start of a line or after the
        // value. It cannot be stripped up front, because every value in the
        // file begins with one.
        if line.trim_start().starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        // `#00ff00   # green` — the value is the first word, the rest is
        // whatever the reader wrote to remind themselves.
        let Some(v) = v.split_whitespace().next() else {
            continue;
        };
        let Some(c) = parse_hex(v) else { continue };
        match k {
            "bg" => p.bg = c,
            "panel" => p.panel = c,
            "panel_alt" => p.panel_alt = c,
            "tab_active_bg" => p.tab_active_bg = c,
            "border" => p.border = c,
            "border_soft" => p.border_soft = c,
            "sel" => p.sel = c,
            "sel_mark_idle" => p.sel_mark_idle = c,
            "err_bg" => p.err_bg = c,
            "diff_add_bg" => p.diff_add_bg = c,
            "diff_del_bg" => p.diff_del_bg = c,
            "diff_void_bg" => p.diff_void_bg = c,
            "fg" => p.fg = c,
            "bright" => p.bright = c,
            "body" => p.body = c,
            "step_fg" => p.step_fg = c,
            "log_fg" => p.log_fg = c,
            "dim" => p.dim = c,
            "dimmer" => p.dimmer = c,
            "dimmest" => p.dimmest = c,
            "gutter" => p.gutter = c,
            "cyan" => p.cyan = c,
            "cyan_soft" => p.cyan_soft = c,
            "green" => p.green = c,
            "yellow" => p.yellow = c,
            "red" => p.red = c,
            "purple" => p.purple = c,
            "orange" => p.orange = c,
            _ => {}
        }
    }
    p
}

/// `#rrggbb`, `rrggbb`, or `#rgb`.
fn parse_hex(raw: &str) -> Option<Color> {
    let h = raw.trim().trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        // `#f0a` is `#ff00aa`, as it is everywhere else
        3 => {
            let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
            Some(Color::Rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
        }
        6 => Some(rgb(n)),
        _ => None,
    }
}

/// The current palette, written out as a theme file to start from.
///
/// Starting from a full file rather than an empty one is the difference
/// between choosing colours and guessing role names: every role is there,
/// commented, already holding a value that works.
///
/// The text and where it goes; putting it there is the worker's job.
pub fn template(name: &str) -> (std::path::PathBuf, String) {
    let path = dir()
        .unwrap_or_else(|| std::path::PathBuf::from("themes"))
        .join(format!("{name}.theme"));
    let p = *p();
    let mut out = String::new();
    out.push_str(&format!("# {name} — a theme for github-tui and diffline\n"));
    out.push_str("#\n");
    out.push_str("# Every role is listed. Delete any line and it keeps the\n");
    out.push_str("# value catppuccin mocha gives it, so a theme can be three\n");
    out.push_str("# colours or all of them.\n\n");

    let width = ROLES.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (role, about) in ROLES {
        let c = role_of(&p, role);
        out.push_str(&format!(
            "{role:<width$} = {}   # {about}\n",
            hex_of(c),
            width = width
        ));
    }
    (path, out)
}

fn role_of(p: &Palette, role: &str) -> Color {
    match role {
        "bg" => p.bg,
        "panel" => p.panel,
        "panel_alt" => p.panel_alt,
        "tab_active_bg" => p.tab_active_bg,
        "border" => p.border,
        "border_soft" => p.border_soft,
        "sel" => p.sel,
        "sel_mark_idle" => p.sel_mark_idle,
        "err_bg" => p.err_bg,
        "diff_add_bg" => p.diff_add_bg,
        "diff_del_bg" => p.diff_del_bg,
        "diff_void_bg" => p.diff_void_bg,
        "fg" => p.fg,
        "bright" => p.bright,
        "body" => p.body,
        "step_fg" => p.step_fg,
        "log_fg" => p.log_fg,
        "dim" => p.dim,
        "dimmer" => p.dimmer,
        "dimmest" => p.dimmest,
        "gutter" => p.gutter,
        "cyan" => p.cyan,
        "cyan_soft" => p.cyan_soft,
        "green" => p.green,
        "yellow" => p.yellow,
        "red" => p.red,
        "purple" => p.purple,
        _ => p.orange,
    }
}

fn hex_of(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => "#000000".into(),
    }
}

/// The role names a theme file may set, in the order they are written out.
pub const ROLES: &[(&str, &str)] = &[
    ("bg", "the terminal's own background"),
    ("panel", "headers and status bars"),
    ("panel_alt", "sidebars and trees"),
    ("tab_active_bg", "the tab in force"),
    ("border", "rules between panes"),
    ("border_soft", "rules inside a pane"),
    ("sel", "the selected row"),
    ("sel_mark_idle", "the cursor bar, unfocused"),
    ("err_bg", "behind an error"),
    ("diff_add_bg", "behind an added line"),
    ("diff_del_bg", "behind a deleted line"),
    ("diff_void_bg", "behind nothing at all, in split view"),
    ("fg", "ordinary text"),
    ("bright", "text that matters"),
    ("body", "prose"),
    ("step_fg", "step names"),
    ("log_fg", "log output"),
    ("dim", "text that matters less"),
    ("dimmer", "text that matters least"),
    ("dimmest", "barely there"),
    ("gutter", "line numbers"),
    ("cyan", "links, and TypeScript"),
    ("cyan_soft", "a second cyan"),
    ("green", "added, passing, approved"),
    ("yellow", "running, queued, the accent"),
    ("red", "deleted, failing, refused"),
    ("purple", "merged, closed, visual mode"),
    ("orange", "Rust, and warnings"),
];

/// The active theme. An atomic rather than a plain static because the service
/// thread shares this module, even though only the render reads it.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// The active theme.
///
/// Worked out from the index rather than by indexing `all()`, because `all()`
/// builds a `Vec` — and this is called by every colour lookup, which is once
/// per cell per frame. It was allocating thousands of times a frame to answer
/// a question with three possible shapes. `built_in_count` and the order in
/// `all()` are the two halves of the same fact, and a test below holds them
/// together.
pub fn current() -> Theme {
    let i = ACTIVE.load(Ordering::Relaxed);
    match i {
        0 => Theme::Design,
        1 => Theme::Mocha,
        n => {
            let k = n - BUILT_IN;
            if k < custom().len() {
                Theme::Custom(k)
            } else {
                // An index left over from a theme file that has since gone.
                Theme::Mocha
            }
        }
    }
}

/// How many themes ship, and so where the custom ones start.
const BUILT_IN: usize = 2;

pub fn set(theme: Theme) {
    let i = Theme::all().iter().position(|t| *t == theme).unwrap_or(0);
    ACTIVE.store(i, Ordering::Relaxed);
}

fn p() -> &'static Palette {
    current().palette()
}

// --- surfaces ---
pub fn bg() -> Color {
    p().bg
}
pub fn panel() -> Color {
    p().panel
}
pub fn panel_alt() -> Color {
    p().panel_alt
}
pub fn tab_active_bg() -> Color {
    p().tab_active_bg
}
pub fn border() -> Color {
    p().border
}
pub fn border_soft() -> Color {
    p().border_soft
}
pub fn sel() -> Color {
    p().sel
}
pub fn sel_mark_idle() -> Color {
    p().sel_mark_idle
}
pub fn err_bg() -> Color {
    p().err_bg
}
pub fn diff_add_bg() -> Color {
    p().diff_add_bg
}
pub fn diff_del_bg() -> Color {
    p().diff_del_bg
}
pub fn diff_void_bg() -> Color {
    p().diff_void_bg
}

// --- text ---
pub fn fg() -> Color {
    p().fg
}
pub fn bright() -> Color {
    p().bright
}
pub fn body() -> Color {
    p().body
}
pub fn step_fg() -> Color {
    p().step_fg
}
pub fn log_fg() -> Color {
    p().log_fg
}
pub fn dim() -> Color {
    p().dim
}
pub fn dimmer() -> Color {
    p().dimmer
}
pub fn dimmest() -> Color {
    p().dimmest
}
pub fn gutter() -> Color {
    p().gutter
}

// --- accents ---
pub fn cyan() -> Color {
    p().cyan
}
pub fn cyan_soft() -> Color {
    p().cyan_soft
}
pub fn green() -> Color {
    p().green
}
pub fn yellow() -> Color {
    p().yellow
}
pub fn red() -> Color {
    p().red
}
pub fn purple() -> Color {
    p().purple
}
pub fn orange() -> Color {
    p().orange
}

/// Per-language colour for the dot in the repository pane.
pub fn lang(name: &str) -> Color {
    match name {
        // the six the design defines
        "TypeScript" => cyan(),
        "Go" => cyan_soft(),
        "Rust" => orange(),
        "Python" => yellow(),
        "Elixir" => purple(),
        "Shell" => green(),
        // the rest, following the colours GitHub assigns them
        "JavaScript" => yellow(),
        "Java" | "Kotlin" => purple(),
        "C" | "C++" | "Zig" => cyan_soft(),
        "C#" => green(),
        "Ruby" | "Nix" => red(),
        "Lua" | "Swift" | "Dart" => orange(),
        "HTML" | "CSS" | "SCSS" | "Vue" | "Svelte" => green(),
        "Haskell" | "OCaml" | "Scala" => purple(),
        "Vim Script" | "Vim script" | "Makefile" | "Dockerfile" => dim(),
        _ => dimmer(),
    }
}

/// A label colour as it arrives from GitHub.
pub fn label(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

/// Visibility markers for the repo pane. The design leaves both empty (the
/// Nerd Font glyphs were lost on save), though the colour is still defined.
pub const PRIVATE_MARK: &str = "";
pub const PUBLIC_MARK: &str = "";

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn the_active_index_and_the_list_agree() {
        // `current()` works the theme out from the index instead of indexing
        // `all()`, because `all()` allocates and this is called once per cell
        // per frame. The two are now separate statements of one fact, so
        // this is what stops them drifting.
        let _g = LOCK.lock();
        for (i, t) in Theme::all().into_iter().enumerate() {
            ACTIVE.store(i, Ordering::Relaxed);
            assert_eq!(current(), t, "index {i}");
        }
        set(Theme::Design);
    }

    #[test]
    fn an_index_past_the_end_falls_back_rather_than_panicking() {
        // A config naming a theme file that has since been deleted.
        let _g = LOCK.lock();
        ACTIVE.store(999, Ordering::Relaxed);
        assert_eq!(current(), Theme::Mocha);
        set(Theme::Design);
    }

    fn hexof(c: Color) -> String {
        hex_of(c)
    }

    #[test]
    fn a_hash_starts_a_comment_but_a_colour_is_not_one() {
        // The first cut stripped everything after the first `#`, which is
        // every value in the file — the theme loaded, listed, and changed
        // nothing.
        let p = parse_palette("green = #00ff00   # what arrived\n# a whole line\nred = #ff0000\n");
        assert_eq!(hexof(p.green), "#00ff00");
        assert_eq!(hexof(p.red), "#ff0000");
    }

    #[test]
    fn a_theme_that_names_three_colours_gets_twenty_six_working_ones() {
        let p = parse_palette("green = #00ff00\n");
        assert_eq!(hexof(p.green), "#00ff00");
        assert_eq!(hexof(p.bg), hexof(MOCHA.bg), "the rest fall back to mocha");
        assert_eq!(hexof(p.yellow), hexof(MOCHA.yellow));
    }

    #[test]
    fn a_short_hex_is_the_long_one() {
        assert_eq!(hexof(parse_hex("#f0a").unwrap()), "#ff00aa");
        assert_eq!(hexof(parse_hex("0b0e14").unwrap()), "#0b0e14");
        assert!(parse_hex("nonsense").is_none());
        assert!(parse_hex("#12345").is_none(), "five digits is neither");
    }

    #[test]
    fn nonsense_is_skipped_rather_than_taking_the_theme_down() {
        let p = parse_palette("green = not-a-colour\nnosuchrole = #ffffff\n= \nred=#ff0000");
        assert_eq!(hexof(p.green), hexof(MOCHA.green), "left as it was");
        assert_eq!(hexof(p.red), "#ff0000", "and the good line still landed");
    }

    #[test]
    fn every_role_the_template_writes_is_one_the_parser_reads() {
        // The two lists are written by hand at opposite ends of the file. If
        // they drift, a theme file tells you about a colour it cannot set.
        for (role, _) in ROLES {
            let text = format!("{role} = #010203");
            let p = parse_palette(&text);
            assert_eq!(
                hexof(role_of(&p, role)),
                "#010203",
                "{role} is written into the template but not read back"
            );
        }
    }

    /// The active theme is process-wide, so the tests that move it take turns.
    /// Everything else asserts against the accessors and is theme-agnostic.
    pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn switching_theme_changes_what_the_accessors_return() {
        let _g = LOCK.lock();
        set(Theme::Design);
        let design_bg = bg();
        set(Theme::Mocha);
        assert_ne!(bg(), design_bg);
        assert_eq!(current(), Theme::Mocha);
        set(Theme::Design);
        assert_eq!(bg(), design_bg);
    }

    #[test]
    fn the_derived_colours_follow_the_theme() {
        // `lang` is the derived colour still here; the status mapping moved
        // to `ui`, where the vocabulary it maps actually lives.
        let _g = LOCK.lock();
        set(Theme::Design);
        let design = lang("Rust");
        set(Theme::Mocha);
        assert_ne!(lang("Rust"), design);
        assert_eq!(lang("Rust"), orange());
        set(Theme::Design);
    }

    #[test]
    fn every_theme_defines_every_role() {
        let _g = LOCK.lock();
        // a palette with a hole would draw an invisible pane, so the whole set
        // is walked rather than spot-checked
        for t in Theme::all() {
            set(t);
            let colours = [
                bg(),
                panel(),
                panel_alt(),
                tab_active_bg(),
                border(),
                border_soft(),
                sel(),
                sel_mark_idle(),
                err_bg(),
                diff_add_bg(),
                diff_del_bg(),
                diff_void_bg(),
                fg(),
                bright(),
                body(),
                step_fg(),
                log_fg(),
                dim(),
                dimmer(),
                dimmest(),
                gutter(),
                cyan(),
                cyan_soft(),
                green(),
                yellow(),
                red(),
                purple(),
                orange(),
            ];
            assert!(
                colours.iter().all(|c| matches!(c, Color::Rgb(..))),
                "{} leaves a role to the terminal's default",
                t.name()
            );
            // and the text must not be the same colour as the ground it sits on
            assert_ne!(fg(), bg(), "{}", t.name());
        }
        set(Theme::Design);
    }

    #[test]
    fn a_label_keeps_the_colour_github_sent() {
        assert_eq!(label((1, 2, 3)), Color::Rgb(1, 2, 3));
    }
}
