//! A glyph for a file, and the language it belongs to.
//!
//! Nerd Font glyphs, and the same ones `nvim-web-devicons` picks, so a file
//! looks the same here as it does in the editor this program hands files to.
//! That familiarity is the whole point; an original set would be a set nobody
//! recognises.
//!
//! They can be turned off. Not every terminal has a Nerd Font, and a row of
//! replacement boxes is worse than no icons at all — so `file-icons = plain`
//! falls back to two ASCII-safe marks, and `none` to nothing.

/// How much decoration a row gets.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    /// Nerd Font glyphs, one per language.
    #[default]
    Nerd,
    /// A file and a folder mark, both in the range any terminal can draw.
    Plain,
    None,
}

impl Style {
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "plain" => Self::Plain,
            "none" => Self::None,
            _ => Self::Nerd,
        }
    }
}

/// The mark an agent is known by, for the ones that have one.
///
/// Only two of these are real. `π` is what pi puts in its own terminal title,
/// and `✳` is what Claude Code prints for itself — both are the agent's own
/// choice, not ours. Codex and opencode have no glyph of their own, so they
/// get a neutral one rather than an invented brand: a made-up icon is
/// decoration pretending to be information.
///
/// Deliberately plain BMP symbols rather than Nerd Font glyphs. A Nerd Font
/// icon lives in the private use area, where `unicode-width` has to guess it
/// is one column wide while the non-Mono font variants draw it across two —
/// which is how a column chart quietly goes crooked. Anyone whose font can do
/// better can say so in the config.
pub fn agent(kind: &str) -> &'static str {
    match kind {
        "claude" => "✳",
        "pi" => "π",
        "codex" => "◆",
        "opencode" => "◇",
        _ => "▪",
    }
}

/// The glyph for a directory, open or closed.
pub fn folder(style: Style, open: bool) -> &'static str {
    match (style, open) {
        (Style::None, _) => "",
        (Style::Plain, _) => "▪",
        (Style::Nerd, true) => "\u{f07c}",  // nf-fa-folder_open
        (Style::Nerd, false) => "\u{f07b}", // nf-fa-folder
    }
}

/// The glyph for a file, by what it is.
pub fn file(style: Style, path: &str) -> &'static str {
    match style {
        Style::None => "",
        Style::Plain => "·",
        Style::Nerd => nerd(path),
    }
}

/// The name of the language a path is in, matching what `theme::lang` colours.
///
/// Returned rather than the colour itself so the model stays out of the view's
/// business: the caller asks the theme what that language looks like.
pub fn language(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);

    // Whole-name matches first: a Makefile has no extension, and `.gitignore`
    // is a name rather than an extension called `gitignore`.
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => return "Makefile",
        "Dockerfile" | "Containerfile" => return "Dockerfile",
        "Cargo.lock" | "go.sum" | "package-lock.json" => return "Lock",
        _ => {}
    }

    match ext(name) {
        "rs" => "Rust",
        "go" => "Go",
        "py" | "pyi" => "Python",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        "lua" => "Lua",
        "zig" => "Zig",
        "c" | "h" => "C",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "C++",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "rb" => "Ruby",
        "ex" | "exs" => "Elixir",
        "swift" => "Swift",
        "dart" => "Dart",
        "cs" => "C#",
        "hs" => "Haskell",
        "ml" | "mli" => "OCaml",
        "scala" | "sbt" => "Scala",
        "nix" => "Nix",
        "sh" | "bash" | "zsh" | "fish" => "Shell",
        "vim" => "Vim script",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "scss" | "sass" => "SCSS",
        "vue" => "Vue",
        "svelte" => "Svelte",
        _ => "",
    }
}

fn ext(name: &str) -> &str {
    // `split_once` from the right, so `archive.tar.gz` is `gz` and a dotfile
    // like `.gitignore` has no extension rather than an extension of its name.
    match name.rfind('.') {
        Some(0) | None => "",
        Some(i) => &name[i + 1..],
    }
}

/// The glyph itself. Grouped by what a reader is actually looking for: source
/// files by language, everything else by what it does.
fn nerd(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);

    match name {
        "Makefile" | "makefile" | "GNUmakefile" => return "\u{e779}",
        "Dockerfile" | "Containerfile" | "docker-compose.yml" | "compose.yml" => {
            return "\u{f308}";
        }
        "Cargo.toml" | "Cargo.lock" => return "\u{e7a8}",
        "package.json" | "package-lock.json" => return "\u{e718}",
        "go.mod" | "go.sum" => return "\u{e627}",
        "LICENSE" | "LICENCE" | "COPYING" => return "\u{f718}",
        _ => {}
    }
    if name.starts_with(".git") {
        return "\u{f1d3}"; // nf-dev-git
    }

    match ext(name) {
        "rs" => "\u{e7a8}",
        "go" => "\u{e627}",
        "py" | "pyi" => "\u{e73c}",
        "ts" | "mts" | "cts" => "\u{e628}",
        "tsx" | "jsx" => "\u{e7ba}",
        "js" | "mjs" | "cjs" => "\u{e781}",
        "lua" => "\u{e620}",
        "zig" => "\u{e6a9}",
        "c" | "h" => "\u{e61e}",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "\u{e61d}",
        "java" => "\u{e738}",
        "kt" | "kts" => "\u{e634}",
        "rb" => "\u{e739}",
        "ex" | "exs" => "\u{e62d}",
        "swift" => "\u{e755}",
        "dart" => "\u{e798}",
        "cs" => "\u{f031b}",
        "hs" => "\u{e777}",
        "nix" => "\u{f313}",
        "sh" | "bash" | "zsh" | "fish" => "\u{f489}",
        "vim" => "\u{e62b}",
        "html" | "htm" => "\u{e736}",
        "css" => "\u{e749}",
        "scss" | "sass" => "\u{e74b}",
        "vue" => "\u{fd42}",
        "svelte" => "\u{e697}",
        "json" | "jsonc" => "\u{e60b}",
        "toml" => "\u{e6b2}",
        "yml" | "yaml" => "\u{e6a8}",
        "md" | "markdown" => "\u{e73e}",
        "sql" => "\u{e706}",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" => "\u{f03e}",
        "pdf" => "\u{f1c1}",
        "zip" | "gz" | "tar" | "xz" | "zst" | "7z" => "\u{f410}",
        "lock" => "\u{f023}",
        _ => "\u{f016}", // nf-fa-file_o, the one that means "a file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn an_extension_is_what_follows_the_last_dot() {
        assert_eq!(ext("main.rs"), "rs");
        assert_eq!(ext("archive.tar.gz"), "gz");
        assert_eq!(ext("Makefile"), "");
    }

    #[test]
    fn a_dotfile_has_no_extension() {
        // `.gitignore` is a name, not an extension called `gitignore`
        assert_eq!(ext(".gitignore"), "");
        assert_eq!(language(".gitignore"), "");
    }

    #[test]
    fn a_language_is_recognised_through_a_full_path() {
        assert_eq!(language("src/app/input.rs"), "Rust");
        assert_eq!(language("cmd/server/main.go"), "Go");
    }

    #[test]
    fn a_file_with_no_extension_can_still_be_known_by_name() {
        assert_eq!(language("Makefile"), "Makefile");
        assert_eq!(language("deploy/Dockerfile"), "Dockerfile");
    }

    #[test]
    fn an_unknown_extension_is_no_language_rather_than_a_wrong_one() {
        assert_eq!(language("notes.xyz"), "");
    }

    #[test]
    fn every_style_gives_a_folder_and_a_file_mark() {
        for style in [Style::Nerd, Style::Plain, Style::None] {
            let _ = folder(style, true);
            let _ = folder(style, false);
            let _ = file(style, "main.rs");
        }
        assert_ne!(folder(Style::Nerd, true), folder(Style::Nerd, false));
        assert_eq!(file(Style::None, "main.rs"), "");
    }

    #[test]
    fn the_plain_style_stays_where_any_terminal_can_draw() {
        // the reason it exists: a row of replacement boxes is worse than none
        for s in [folder(Style::Plain, true), file(Style::Plain, "x.rs")] {
            assert!(
                s.chars().all(|c| (c as u32) < 0x3000),
                "{s} is beyond what a plain terminal is asked for"
            );
        }
    }

    #[test]
    fn every_glyph_is_one_character() {
        // the column is fixed; two characters would push the row sideways
        for path in [
            "main.rs",
            "a.go",
            "b.py",
            "c.ts",
            "d.unknown",
            "Makefile",
            "Cargo.toml",
            ".gitignore",
        ] {
            let g = file(Style::Nerd, path);
            assert_eq!(g.chars().count(), 1, "{path} drew {g:?}");
            assert!(g.width() <= 2, "{path} drew something too wide");
        }
    }

    #[test]
    fn an_unknown_file_still_gets_a_mark() {
        assert!(!file(Style::Nerd, "whatever.qqq").is_empty());
    }

    #[test]
    fn the_style_is_read_from_a_word_and_defaults_to_glyphs() {
        assert_eq!(Style::parse("plain"), Style::Plain);
        assert_eq!(Style::parse("none"), Style::None);
        assert_eq!(Style::parse("nerd"), Style::Nerd);
        assert_eq!(Style::parse("  plain  "), Style::Plain);
        assert_eq!(Style::parse("nonsense"), Style::Nerd, "the default holds");
    }
}
