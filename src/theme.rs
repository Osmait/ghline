//! Colours and glyphs.
//!
//! The design's palette is one of several. A theme is a whole `Palette`, and
//! switching one in is a single store, so the change lands on the very next
//! frame — there is nothing to rebuild or re-fetch.
//!
//! Call sites read through the accessors below rather than naming a constant,
//! which is what makes the swap possible at all.

use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::Color;

use crate::data::{ReviewState, Status};

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Every colour the interface draws with, in the roles the design defines.
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
}

impl Theme {
    pub const ALL: [Self; 2] = [Self::Design, Self::Mocha];

    pub fn name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Mocha => "catppuccin mocha",
        }
    }

    /// The name this theme is stored under. Kept apart from `name()` so that
    /// rewording the picker cannot invalidate everyone's saved config.
    pub fn key(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Mocha => "mocha",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    /// One line about where the palette comes from, for the picker.
    pub fn about(self) -> &'static str {
        match self {
            Self::Design => "the palette of GitHub TUI.dc.html",
            Self::Mocha => "catppuccin.com · the darkest of the four flavours",
        }
    }

    fn palette(self) -> &'static Palette {
        match self {
            Self::Design => &DESIGN,
            Self::Mocha => &MOCHA,
        }
    }
}

/// The active theme. An atomic rather than a plain static because the service
/// thread shares this module, even though only the render reads it.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn current() -> Theme {
    Theme::ALL[ACTIVE.load(Ordering::Relaxed).min(Theme::ALL.len() - 1)]
}

pub fn set(theme: Theme) {
    let i = Theme::ALL.iter().position(|t| *t == theme).unwrap_or(0);
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

/// The design's `sc(status)`.
pub fn state_color(status: Status) -> Color {
    match status {
        Status::Success | Status::Open => green(),
        Status::Failure => red(),
        Status::Running => yellow(),
        Status::Pending | Status::Skipped => dimmer(),
        Status::Cancelled | Status::Draft => dim(),
        Status::Closed | Status::Merged => purple(),
        Status::Unknown => fg(),
    }
}

/// The design's `si(status)`.
pub fn state_icon(status: Status) -> &'static str {
    match status {
        Status::Success => "✓",
        Status::Failure => "✗",
        Status::Running => "◐",
        Status::Pending => "○",
        Status::Skipped => "⊘",
        Status::Cancelled => "⊗",
        _ => "•",
    }
}

/// Colour and glyph for a review state. This is the view's decision, which is
/// why it lives here and not in the model.
pub fn review(state: ReviewState) -> (Color, &'static str) {
    match state {
        ReviewState::Approved => (green(), "✓"),
        ReviewState::ChangesRequested => (red(), "✗"),
        ReviewState::Dismissed => (dim(), "⊘"),
        ReviewState::Commented => (yellow(), "●"),
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
        let _g = LOCK.lock();
        set(Theme::Design);
        let design = state_color(Status::Success);
        set(Theme::Mocha);
        assert_ne!(state_color(Status::Success), design);
        assert_eq!(state_color(Status::Success), green());
        set(Theme::Design);
    }

    #[test]
    fn every_theme_defines_every_role() {
        let _g = LOCK.lock();
        // a palette with a hole would draw an invisible pane, so the whole set
        // is walked rather than spot-checked
        for t in Theme::ALL {
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
