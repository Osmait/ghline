//! Colouring source code, approximately.
//!
//! A lexer rather than a parser, and the difference is a deliberate trade. A
//! real grammar — tree-sitter — would need a C toolchain in a project that has
//! none, and one grammar crate per language, which means perfect colour for
//! the languages someone remembered to bundle and none at all for the rest.
//!
//! This pane is read-only. Comments, strings, numbers and keywords are what a
//! reader's eye uses to find its place, and those a lexer finds. What it does
//! not find is structure: a type is only coloured when it looks like one, and a
//! macro or a regex will occasionally be read as something it is not. That is
//! the honest cost, and for scrolling through a file it is a cheap one.
//!
//! Multi-line constructs mean a line cannot be coloured on its own, so a whole
//! file is lexed at once when it arrives and the spans are kept.

/// What a run of characters is, as far as colour is concerned.
///
/// There is no `Normal`: a stretch with no span over it is ordinary text, and
/// a variant for that would only be a second way to say nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A comment, opening marker included. Wins over everything inside it,
    /// which is why a keyword in prose is not coloured as one.
    Comment,
    /// A string, quotes included. Named short so it does not read as a
    /// mention of `String`, which is a `Type`.
    Str,
    /// A numeric literal, taken as one run with its separators and suffix:
    /// `1_000u64` is a single span, not three.
    Number,
    /// A whole word from the language's list. Whole only — `iffy` is not
    /// `if`, which is the one bug this pass had and has a test for.
    Keyword,
    /// A word that looks like a type: it starts with a capital.
    Type,
}

/// A run of one kind within a line, as byte offsets into that line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Span {
    /// Byte offset where the run starts.
    ///
    /// Bytes rather than characters because the caller slices the line with
    /// these; on a line with a `漢` in it the two counts differ.
    pub from: usize,
    /// Byte offset one past the run's last byte, so `line[from..to]` is it.
    pub to: usize,
    /// What the run is.
    pub kind: Kind,
}

/// What a language is made of, as far as this needs to know.
pub struct Lang {
    /// Everything from here to the end of the line.
    pub line_comment: &'static [&'static str],
    /// Opening and closing of a comment that can span lines.
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Quote characters that open a string.
    pub quotes: &'static [char],
    /// Whether a backslash escapes inside a string. Not universal: in TOML and
    /// in a shell's single quotes it does not.
    pub escapes: bool,
    /// The words coloured as keywords, matched whole rather than as prefixes.
    ///
    /// A slice searched linearly, not a set: these lists are a few dozen
    /// short strings, and a hash per word would cost more than the scan it
    /// replaces. Being sorted is for whoever edits the list, not for lookup.
    pub keywords: &'static [&'static str],
}

const C_LIKE: Option<(&str, &str)> = Some(("/*", "*/"));

const RUST: Lang = Lang {
    line_comment: &["//"],
    block_comment: C_LIKE,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ],
};

const GO: Lang = Lang {
    line_comment: &["//"],
    block_comment: C_LIKE,
    quotes: &['"', '`', '\''],
    escapes: true,
    keywords: &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "type",
        "var",
        "nil",
        "true",
        "false",
    ],
};

const PYTHON: Lang = Lang {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "False",
        "try", "while", "with", "yield",
    ],
};

const JS: Lang = Lang {
    line_comment: &["//"],
    block_comment: C_LIKE,
    quotes: &['"', '\'', '`'],
    escapes: true,
    keywords: &[
        "as",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "from",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "of",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "type",
        "typeof",
        "undefined",
        "var",
        "void",
        "while",
        "yield",
    ],
};

const LUA: Lang = Lang {
    line_comment: &["--"],
    block_comment: Some(("--[[", "]]")),
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if",
        "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ],
};

const ZIG: Lang = Lang {
    line_comment: &["//"],
    block_comment: None,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "align",
        "and",
        "anytype",
        "asm",
        "break",
        "catch",
        "comptime",
        "const",
        "continue",
        "defer",
        "else",
        "enum",
        "error",
        "export",
        "extern",
        "false",
        "fn",
        "for",
        "if",
        "inline",
        "null",
        "or",
        "orelse",
        "pub",
        "return",
        "struct",
        "switch",
        "test",
        "true",
        "try",
        "undefined",
        "union",
        "unreachable",
        "usingnamespace",
        "var",
        "while",
    ],
};

const C: Lang = Lang {
    line_comment: &["//"],
    block_comment: C_LIKE,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "auto",
        "break",
        "case",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "false",
        "float",
        "for",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "namespace",
        "new",
        "nullptr",
        "public",
        "private",
        "protected",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "template",
        "this",
        "true",
        "typedef",
        "union",
        "unsigned",
        "using",
        "virtual",
        "void",
        "while",
    ],
};

const JAVA: Lang = Lang {
    line_comment: &["//"],
    block_comment: C_LIKE,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "abstract",
        "boolean",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extends",
        "false",
        "final",
        "finally",
        "float",
        "for",
        "if",
        "implements",
        "import",
        "instanceof",
        "int",
        "interface",
        "long",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "throws",
        "true",
        "try",
        "void",
        "while",
    ],
};

const SHELL: Lang = Lang {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[
        "case", "do", "done", "elif", "else", "esac", "exit", "export", "fi", "for", "function",
        "if", "in", "local", "return", "set", "then", "until", "while",
    ],
};

/// Data formats. No keywords worth the name, but strings and comments are most
/// of what is on the screen anyway.
const TOML: Lang = Lang {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    escapes: false,
    keywords: &["true", "false"],
};

const YAML: Lang = Lang {
    line_comment: &["#"],
    block_comment: None,
    quotes: &['"', '\''],
    escapes: true,
    keywords: &["true", "false", "null", "yes", "no"],
};

const JSON: Lang = Lang {
    line_comment: &[],
    block_comment: None,
    quotes: &['"'],
    escapes: true,
    keywords: &["true", "false", "null"],
};

/// The language of a path, when it is one this knows.
pub fn of_path(path: &str) -> Option<&'static Lang> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "Makefile" | "makefile" | "GNUmakefile" | "Dockerfile" => return Some(&SHELL),
        _ => {}
    }
    let ext = match name.rfind('.') {
        Some(0) | None => return None,
        Some(i) => &name[i + 1..],
    };
    Some(match ext {
        "rs" => &RUST,
        "go" => &GO,
        "py" | "pyi" => &PYTHON,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => &JS,
        "lua" => &LUA,
        "zig" => &ZIG,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" => &C,
        "java" | "kt" | "kts" | "scala" => &JAVA,
        "sh" | "bash" | "zsh" | "fish" => &SHELL,
        "toml" => &TOML,
        "yml" | "yaml" => &YAML,
        "json" | "jsonc" => &JSON,
        _ => return None,
    })
}

/// Carried between lines, because a comment or a string can outlive one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum State {
    #[default]
    Code,
    Block,
    /// Inside a string opened with this quote on an earlier line.
    Str(char),
}

/// Colours a whole file, one vector of spans per line.
///
/// Whole-file because of `State`: a line inside a block comment cannot be
/// recognised without knowing how it was reached, and this pane can start
/// drawing anywhere.
pub fn highlight(lang: &Lang, text: &str) -> Vec<Vec<Span>> {
    let mut state = State::Code;
    text.lines().map(|l| line(lang, l, &mut state)).collect()
}

/// Everything that is not `Normal` on one line.
fn line(lang: &Lang, src: &str, state: &mut State) -> Vec<Span> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;

    while i < b.len() {
        match *state {
            State::Block => {
                let close = lang.block_comment.map_or("*/", |(_, c)| c);
                match find(src, close, i) {
                    Some(end) => {
                        push(&mut out, i, end + close.len(), Kind::Comment);
                        i = end + close.len();
                        *state = State::Code;
                    }
                    None => {
                        push(&mut out, i, b.len(), Kind::Comment);
                        return out;
                    }
                }
            }
            State::Str(q) => {
                let end = string_end(src, i, q, lang.escapes);
                push(&mut out, i, end.0, Kind::Str);
                i = end.0;
                if end.1 {
                    *state = State::Code;
                } else {
                    return out;
                }
            }
            State::Code => {
                // a line comment wins over everything left on the line
                if let Some(marker) = lang.line_comment.iter().find(|m| src[i..].starts_with(**m)) {
                    // `--[[` in Lua opens a block, and starts with `--`
                    let is_block = lang
                        .block_comment
                        .is_some_and(|(o, _)| src[i..].starts_with(o) && o.starts_with(marker));
                    if !is_block {
                        push(&mut out, i, b.len(), Kind::Comment);
                        return out;
                    }
                }
                if let Some((open, _)) = lang.block_comment
                    && src[i..].starts_with(open)
                {
                    *state = State::Block;
                    i += open.len();
                    push(&mut out, i - open.len(), i, Kind::Comment);
                    // the marker is re-covered by the Block arm's span; merging
                    // happens at the end
                    out.pop();
                    i -= open.len();
                    continue;
                }

                let c = src[i..].chars().next().unwrap_or(' ');
                if lang.quotes.contains(&c) {
                    let start = i;
                    i += c.len_utf8();
                    let end = string_end(src, i, c, lang.escapes);
                    push(&mut out, start, end.0, Kind::Str);
                    i = end.0;
                    if !end.1 {
                        *state = State::Str(c);
                        return out;
                    }
                    continue;
                }
                if c.is_ascii_digit() {
                    let start = i;
                    while i < b.len()
                        && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_')
                    {
                        i += 1;
                    }
                    push(&mut out, start, i, Kind::Number);
                    continue;
                }
                if c.is_alphabetic() || c == '_' {
                    let start = i;
                    // By character, not by byte. `is_alphabetic` is true of
                    // every letter there is, and this used to walk forwards
                    // only over the ASCII ones — so a word starting with `é`,
                    // `ñ` or `漢` entered the loop and then advanced by
                    // nothing, for ever, with the whole program inside it.
                    // Reading a file is what a terminal cannot be interrupted
                    // out of.
                    while let Some(ch) = src[i..].chars().next() {
                        if !ch.is_alphanumeric() && ch != '_' {
                            break;
                        }
                        i += ch.len_utf8();
                    }
                    let word = &src[start..i];
                    if lang.keywords.contains(&word) {
                        push(&mut out, start, i, Kind::Keyword);
                    } else if word.starts_with(char::is_uppercase) {
                        push(&mut out, start, i, Kind::Type);
                    }
                    continue;
                }
                i += c.len_utf8();
            }
        }
    }
    out
}

fn push(out: &mut Vec<Span>, from: usize, to: usize, kind: Kind) {
    if to > from {
        out.push(Span { from, to, kind });
    }
}

fn find(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay.get(from..)
        .and_then(|s| s.find(needle))
        .map(|i| i + from)
}

/// Where a string ends, and whether it was closed on this line.
///
/// `from` is the first byte after the opening quote.
fn string_end(src: &str, from: usize, quote: char, escapes: bool) -> (usize, bool) {
    let b = src.as_bytes();
    let mut i = from;
    while i < b.len() {
        let c = src[i..].chars().next().unwrap_or(' ');
        if escapes && c == '\\' {
            i += c.len_utf8();
            i += src[i..].chars().next().map_or(0, char::len_utf8);
            continue;
        }
        i += c.len_utf8();
        if c == quote {
            return (i, true);
        }
    }
    (b.len(), false)
}

/// Splits a line into pieces of at most `width` columns, as byte ranges.
///
/// Hard rather than on word boundaries, and not only because keeping the byte
/// offsets is what lets a span survive the wrap: code wrapped on spaces reads
/// worse than code wrapped on the column, since the indentation stops meaning
/// anything.
pub fn wrap_ranges(line: &str, width: usize) -> Vec<(usize, usize)> {
    use unicode_width::UnicodeWidthChar;

    if line.is_empty() || width == 0 {
        return vec![(0, line.len())];
    }
    let mut out = Vec::new();
    let (mut start, mut used) = (0usize, 0usize);

    for (i, c) in line.char_indices() {
        let w = c.width().unwrap_or(0);
        if used + w > width && i > start {
            out.push((start, i));
            start = i;
            used = 0;
        }
        used += w;
    }
    out.push((start, line.len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kinds found on a line, with the text each covers.
    fn spans<'a>(lang: &Lang, src: &'a str) -> Vec<(Kind, &'a str)> {
        let mut state = State::Code;
        line(lang, src, &mut state)
            .into_iter()
            .map(|s| (s.kind, &src[s.from..s.to]))
            .collect()
    }

    fn kinds(lang: &Lang, src: &str) -> Vec<Kind> {
        spans(lang, src).into_iter().map(|(k, _)| k).collect()
    }

    // --- which language ---

    #[test]
    fn a_language_is_found_through_a_path() {
        assert!(of_path("src/app/input.rs").is_some());
        assert!(of_path("cmd/main.go").is_some());
    }

    #[test]
    fn a_file_with_no_extension_can_still_be_known() {
        assert!(of_path("Makefile").is_some());
    }

    #[test]
    fn an_unknown_language_is_none_rather_than_a_guess() {
        assert!(of_path("notes.xyz").is_none());
        assert!(of_path("README").is_none());
    }

    // --- the pieces ---

    #[test]
    fn a_line_comment_takes_the_rest_of_the_line() {
        assert_eq!(
            spans(&RUST, "let x = 1; // and this"),
            vec![
                (Kind::Keyword, "let"),
                (Kind::Number, "1"),
                (Kind::Comment, "// and this"),
            ]
        );
    }

    #[test]
    fn a_comment_marker_inside_a_string_is_not_a_comment() {
        // the classic: the string is found first, so the `//` never opens one
        let got = spans(&RUST, r#"let url = "https://example.com";"#);
        assert_eq!(
            got,
            vec![
                (Kind::Keyword, "let"),
                (Kind::Str, r#""https://example.com""#),
            ]
        );
    }

    #[test]
    fn a_quote_inside_a_comment_does_not_open_a_string() {
        let mut state = State::Code;
        line(&RUST, "// it's fine", &mut state);
        assert_eq!(state, State::Code, "the line ended where it started");
    }

    #[test]
    fn an_escaped_quote_does_not_close_a_string() {
        let got = spans(&RUST, r#"let s = "a \" b";"#);
        assert_eq!(got[1], (Kind::Str, r#""a \" b""#));
    }

    #[test]
    fn a_string_that_does_not_close_carries_on_to_the_next_line() {
        let mut state = State::Code;
        let first = line(&PYTHON, "s = 'unterminated", &mut state);
        assert_eq!(state, State::Str('\''), "the state came with it");
        assert_eq!(first.last().unwrap().kind, Kind::Str);

        let second = line(&PYTHON, "still a string' then code", &mut state);
        assert_eq!(second[0].kind, Kind::Str);
        assert_eq!(state, State::Code, "and it closed on the second line");
    }

    #[test]
    fn a_block_comment_spans_lines_and_then_stops() {
        let out = highlight(&RUST, "/* one\ntwo */ let x = 1;");
        assert_eq!(out[0][0].kind, Kind::Comment);
        assert_eq!(out[1][0].kind, Kind::Comment);
        assert!(
            out[1].iter().any(|s| s.kind == Kind::Keyword),
            "code after the close is code again"
        );
    }

    #[test]
    fn a_block_comment_that_never_closes_colours_to_the_end() {
        let out = highlight(&RUST, "/* one\ntwo\nthree");
        for (n, spans) in out.iter().enumerate() {
            assert_eq!(spans[0].kind, Kind::Comment, "line {n}");
        }
    }

    #[test]
    fn keywords_are_whole_words_only() {
        // `format` contains `for`, and is not a keyword
        let got = spans(&RUST, "format");
        assert!(!got.iter().any(|(k, _)| *k == Kind::Keyword), "{got:?}");
    }

    #[test]
    fn a_capitalised_word_reads_as_a_type() {
        assert_eq!(spans(&RUST, "Vec"), vec![(Kind::Type, "Vec")]);
        assert!(spans(&RUST, "vec").is_empty(), "lowercase is ordinary");
    }

    /// The bug this is guarding against was not a wrong colour. Entering the
    /// word loop took one test — `is_alphabetic`, true of every letter there
    /// is — and leaving it took another, `is_ascii_alphanumeric`, which is
    /// false of most of them. A line with `año` or `漢字` on it walked
    /// forwards by nothing, for ever, inside the draw. There is no key that
    /// interrupts that: the program stops answering and the terminal keeps
    /// its screen.
    ///
    /// It survived this long because every fixture in this repository, and
    /// most code anywhere, is ASCII outside its strings and comments — and a
    /// string and a comment are both read by something else.
    #[test]
    fn a_word_that_is_not_ascii_is_still_a_word() {
        assert_eq!(spans(&RUST, "año"), vec![]);
        assert_eq!(spans(&RUST, "Año"), vec![(Kind::Type, "Año")]);
        assert_eq!(spans(&RUST, "Ünicode"), vec![(Kind::Type, "Ünicode")]);
        // and the word is taken whole, rather than one letter at a time
        assert_eq!(spans(&RUST, "let café"), vec![(Kind::Keyword, "let")]);
    }

    #[test]
    fn a_number_is_taken_whole_suffix_and_all() {
        assert_eq!(spans(&RUST, "1_000u64"), vec![(Kind::Number, "1_000u64")]);
        assert_eq!(spans(&RUST, "0.5"), vec![(Kind::Number, "0.5")]);
    }

    #[test]
    fn a_digit_inside_a_name_is_not_a_number() {
        assert!(
            !kinds(&RUST, "utf8_len").contains(&Kind::Number),
            "the name is one word"
        );
    }

    #[test]
    fn lua_tells_its_block_comment_from_its_line_comment() {
        // `--[[` starts with `--`, so the order these are tested in matters
        let out = highlight(&LUA, "--[[ one\ntwo ]] local x = 1");
        assert_eq!(out[0][0].kind, Kind::Comment);
        assert!(out[1].iter().any(|s| s.kind == Kind::Keyword));

        let plain = spans(&LUA, "-- just a comment");
        assert_eq!(plain, vec![(Kind::Comment, "-- just a comment")]);
    }

    #[test]
    fn a_language_with_no_block_comment_does_not_invent_one() {
        let out = highlight(&PYTHON, "x = 1 /* not a comment */");
        assert!(!out[0].iter().any(|s| s.kind == Kind::Comment));
    }

    #[test]
    fn toml_does_not_treat_a_backslash_as_an_escape() {
        // a Windows path in a TOML string ends where the quote is
        let got = spans(&TOML, r#"path = "C:\" "#);
        assert_eq!(got[0], (Kind::Str, r#""C:\""#));
    }

    #[test]
    fn every_span_is_a_valid_slice_of_its_line() {
        // the guard against a byte offset landing inside a character
        let src = "let s = \"日本語のコメント\"; // 説明";
        let mut state = State::Code;
        for s in line(&RUST, src, &mut state) {
            assert!(src.is_char_boundary(s.from), "{s:?}");
            assert!(src.is_char_boundary(s.to), "{s:?}");
        }
    }

    #[test]
    fn spans_never_overlap_and_run_forwards() {
        let src = r#"pub fn f() -> Vec<u8> { let x = "a"; /* c */ 42 }"#;
        let mut state = State::Code;
        let mut last = 0;
        for s in line(&RUST, src, &mut state) {
            assert!(s.from >= last, "overlapping at {s:?}");
            assert!(s.to > s.from);
            last = s.to;
        }
    }

    // --- wrapping, which colour has to survive ---

    #[test]
    fn a_short_line_wraps_to_itself() {
        assert_eq!(wrap_ranges("hello", 20), vec![(0, 5)]);
    }

    #[test]
    fn a_long_line_is_cut_at_the_column() {
        assert_eq!(wrap_ranges("abcdefgh", 3), vec![(0, 3), (3, 6), (6, 8)]);
    }

    #[test]
    fn the_ranges_cover_the_line_exactly_once() {
        let line = "let x = compute(a, b) + other(c);";
        let mut at = 0;
        for (from, to) in wrap_ranges(line, 7) {
            assert_eq!(from, at, "a gap or an overlap");
            at = to;
        }
        assert_eq!(at, line.len(), "and it reaches the end");
    }

    #[test]
    fn a_wrap_never_lands_inside_a_character() {
        let line = "日本語のテキストがここにある";
        for (from, to) in wrap_ranges(line, 5) {
            assert!(line.is_char_boundary(from));
            assert!(line.is_char_boundary(to));
        }
    }

    #[test]
    fn a_wide_character_is_not_split_across_two_rows() {
        // two columns each, three to a row of five
        let ranges = wrap_ranges("漢漢漢漢", 5);
        assert_eq!(ranges.len(), 2);
        assert_eq!(&"漢漢漢漢"[ranges[0].0..ranges[0].1], "漢漢");
    }

    #[test]
    fn an_empty_line_is_one_empty_range() {
        assert_eq!(wrap_ranges("", 10), vec![(0, 0)]);
    }

    #[test]
    fn a_zero_width_does_not_loop_forever() {
        assert_eq!(wrap_ranges("abc", 0), vec![(0, 3)]);
    }

    #[test]
    fn lexing_a_large_file_is_not_something_the_reader_waits_for() {
        // half a megabyte is the ceiling this pane will ever fetch
        let unit = "pub fn f(x: i32) -> Vec<u8> { /* c */ let s = \"text\"; 42 }\n";
        let big = unit.repeat(512 * 1024 / unit.len());
        let t = std::time::Instant::now();
        let out = highlight(&RUST, &big);
        let ms = t.elapsed().as_millis();

        assert!(!out.is_empty());
        // Generous on purpose: this is a guard against a pathological
        // regression, not a benchmark. It measured 54ms unoptimised, and it
        // runs on the service thread rather than between frames.
        assert!(
            ms < 400,
            "{} KB took {ms}ms, which is no longer a lexer",
            big.len() / 1024
        );
    }

    #[test]
    fn an_empty_file_is_no_lines_rather_than_a_panic() {
        assert!(highlight(&RUST, "").is_empty());
        assert!(line(&RUST, "", &mut State::Code).is_empty());
    }
}
