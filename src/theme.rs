//! Palette and glyphs taken verbatim from the design (`GitHub TUI.dc.html`).

use crate::data::Status;
use ratatui::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// --- surfaces ---
pub const BG: Color = rgb(0x0b0e14);
pub const PANEL: Color = rgb(0x10141c);
pub const PANEL_ALT: Color = rgb(0x0d1017);
pub const TAB_ACTIVE_BG: Color = rgb(0x151b25);
pub const BORDER: Color = rgb(0x232936);
pub const BORDER_SOFT: Color = rgb(0x1a1f29);
pub const SEL: Color = rgb(0x1b2330);
pub const SEL_MARK_IDLE: Color = rgb(0x2c3444);
pub const ERR_BG: Color = rgb(0x1f1418);
// diff backgrounds
pub const DIFF_ADD_BG: Color = rgb(0x0f2418);
pub const DIFF_DEL_BG: Color = rgb(0x26141a);
/// Filler for the empty half of an unbalanced pair in the split diff.
pub const DIFF_VOID_BG: Color = rgb(0x0e1219);

// --- text ---
pub const FG: Color = rgb(0xc5cdd9);
pub const BRIGHT: Color = rgb(0xe6e1cf);
pub const BODY: Color = rgb(0xa9b2bf);
pub const STEP_FG: Color = rgb(0x9aa4b2);
pub const LOG_FG: Color = rgb(0xb8c0cc);
pub const DIM: Color = rgb(0x5c6773);
pub const DIMMER: Color = rgb(0x4d5566);
pub const DIMMEST: Color = rgb(0x565b66);
pub const GUTTER: Color = rgb(0x323947);

// --- accents ---
pub const CYAN: Color = rgb(0x39bae6);
pub const CYAN_SOFT: Color = rgb(0x73d0ff);
pub const GREEN: Color = rgb(0x7fd962);
pub const YELLOW: Color = rgb(0xffb454);
pub const RED: Color = rgb(0xf07178);
pub const PURPLE: Color = rgb(0xd2a6ff);
pub const ORANGE: Color = rgb(0xff8f40);

/// Per-language colour for the dot in the repository pane.
pub fn lang(name: &str) -> Color {
    match name {
        // the six the design defines
        "TypeScript" => CYAN,
        "Go" => CYAN_SOFT,
        "Rust" => ORANGE,
        "Python" => YELLOW,
        "Elixir" => PURPLE,
        "Shell" => GREEN,
        // the rest, following the colours GitHub assigns them
        "JavaScript" => YELLOW,
        "Java" | "Kotlin" => PURPLE,
        "C" | "C++" | "Zig" => CYAN_SOFT,
        "C#" => GREEN,
        "Ruby" | "Nix" => RED,
        "Lua" | "Swift" | "Dart" => ORANGE,
        "HTML" | "CSS" | "SCSS" | "Vue" | "Svelte" => GREEN,
        "Haskell" | "OCaml" | "Scala" => PURPLE,
        "Vim Script" | "Vim script" | "Makefile" | "Dockerfile" => DIM,
        _ => DIMMER,
    }
}

/// The design's `sc(status)`.
pub fn state_color(status: Status) -> Color {
    match status {
        Status::Success | Status::Open => GREEN,
        Status::Failure => RED,
        Status::Running => YELLOW,
        Status::Pending | Status::Skipped => DIMMER,
        Status::Cancelled | Status::Draft => DIM,
        Status::Closed | Status::Merged => PURPLE,
        Status::Unknown => FG,
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
pub fn review(state: crate::data::ReviewState) -> (Color, &'static str) {
    use crate::data::ReviewState as R;
    match state {
        R::Approved => (GREEN, "✓"),
        R::ChangesRequested => (RED, "✗"),
        R::Dismissed => (DIM, "⊘"),
        R::Commented => (YELLOW, "●"),
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
