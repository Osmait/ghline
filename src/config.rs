//! The handful of settings that outlive a run.
//!
//! One file of `key = value` lines, because the alternative — a serialisation
//! dependency — would be larger than everything it stores. Unknown keys are
//! kept in memory and written back out, so a config written by a newer version
//! survives being loaded by an older one.
//!
//! Nothing here is allowed to stop the application. A config that cannot be
//! read is a config with no settings in it, which is exactly what a first run
//! has.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use crate::theme::Theme;

const DIR: &str = "github-tui";
const FILE: &str = "config";

/// `$XDG_CONFIG_HOME/github-tui/config`, falling back to `~/.config`.
///
/// `None` when neither variable is set — an environment with no home to write
/// to, such as a build sandbox, where not persisting is the correct outcome.
pub fn path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join(DIR).join(FILE))
}

/// Everything the file holds, by key.
pub type Settings = BTreeMap<String, String>;

/// Reads the file, or an empty set if there is nothing to read.
pub fn load() -> Settings {
    path().map(load_from).unwrap_or_default()
}

fn load_from(p: impl AsRef<std::path::Path>) -> Settings {
    std::fs::read_to_string(p)
        .map(|text| parse(&text))
        .unwrap_or_default()
}

fn parse(text: &str) -> Settings {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let (k, v) = (k.trim(), v.trim());
            if k.is_empty() {
                return None;
            }
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

fn render(settings: &Settings) -> String {
    let mut out = String::from("# github-tui. Written by the application; edit freely.\n");
    for (k, v) in settings {
        out.push_str(&format!("{k} = {v}\n"));
    }
    out
}

/// Writes `key = value`, leaving every other setting as it was on disk.
///
/// Read-modify-write rather than writing the one key: two settings changed in
/// one run must not erase each other, and a key this version does not know
/// about must not be dropped.
pub fn set(key: &str, value: &str) -> io::Result<()> {
    let p = path().ok_or_else(|| io::Error::other("no HOME or XDG_CONFIG_HOME to write to"))?;
    set_at(p, key, value)
}

fn set_at(p: impl AsRef<std::path::Path>, key: &str, value: &str) -> io::Result<()> {
    let p = p.as_ref();
    let mut settings = load_from(p);
    settings.insert(key.to_string(), value.to_string());

    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Written beside the target and renamed over it, so an interrupted write
    // leaves the previous config intact instead of a truncated one.
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, render(&settings))?;
    std::fs::rename(&tmp, p)
}

// ------------------------------------------------------------------- theme

const THEME: &str = "theme";

/// Applies the saved theme, if there is one. Called once at startup.
pub fn apply_theme() {
    if let Some(t) = load().get(THEME).and_then(|k| Theme::from_key(k)) {
        crate::theme::set(t);
    }
}

pub fn save_theme(theme: Theme) -> io::Result<()> {
    set(THEME, theme.key())
}

// ------------------------------------------------------------------ agents

/// What an agent is told, per kind of thing it is being handed.
///
/// A setting rather than a constant because what a coding agent needs in its
/// first message is a matter of taste and will change. The URL is in every
/// default on purpose: an agent that can read the thing itself asks fewer
/// questions than one working from a paraphrase.
///
/// A config file is one line per key, so `\n` in a value is those two
/// characters; they become real newlines on the way out.
/// The template stored under `key`, or `fallback` when there is none.
///
/// Takes a key and a default rather than the thing they belong to, so this
/// module — which both programs share — does not have to know what a GitHub
/// issue is.
pub fn template(key: &str, fallback: &str) -> String {
    load()
        .get(key)
        .map(|t| t.replace("\\n", "\n"))
        .unwrap_or_else(|| fallback.to_string())
}

/// Fills the template in. An unknown placeholder is left alone rather than
/// blanked, so a typo shows up as itself instead of vanishing.
pub fn render_prompt(
    template: &str,
    repo: &str,
    num: i64,
    title: &str,
    url: &str,
    context: &str,
) -> String {
    // `{context}` is substituted last, so a body that happens to contain
    // `{repo}` arrives as it was written rather than expanded.
    template
        .replace("{repo}", repo)
        .replace("{num}", &num.to_string())
        .replace("{title}", title)
        .replace("{url}", url)
        .replace("{context}", context)
}

const AGENTS: &str = "agents";

/// The agents offered for a fresh worktree.
///
/// A setting because herdr decides what it can start, not this program, and
/// the list grows. An unsupported name is not rejected here — herdr's own
/// refusal is a better message than a guess at one.
const DEFAULT_AGENTS: &str = "claude, codex, opencode, pi";

pub fn agent_kinds() -> Vec<String> {
    load()
        .get(AGENTS)
        .cloned()
        .unwrap_or_else(|| DEFAULT_AGENTS.to_string())
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .collect()
}

const ICONS: &str = "agent-icons";

/// The mark to draw for an agent, from config or from what it calls itself.
///
/// Configurable because the right answer depends on a font this program cannot
/// see. Someone running a Nerd Font Mono can put a proper brand glyph here;
/// the defaults stay in the range every terminal can fall back to.
pub fn agent_icon(kind: &str) -> String {
    load()
        .get(ICONS)
        .and_then(|spec| lookup_icon(spec, kind))
        .unwrap_or_else(|| crate::icons::agent(kind).to_string())
}

/// Reads `claude=✳, codex=⌬` and finds one entry.
///
/// An entry that is not a single character is ignored rather than drawn: this
/// goes into a fixed-width column, and a word there would push everything on
/// the row sideways.
fn lookup_icon(spec: &str, kind: &str) -> Option<String> {
    spec.split(',')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| k.trim() == kind)
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| v.chars().count() == 1)
}

/// How much decoration the file tree gets.
///
/// A setting because it depends on a font this program cannot see: a terminal
/// without a Nerd Font would draw a column of replacement boxes, which is
/// worse than no icons at all.
pub fn file_icons() -> crate::icons::Style {
    load()
        .get("file-icons")
        .map_or(crate::icons::Style::Nerd, |v| crate::icons::Style::parse(v))
}

/// Puts a typed instruction into a rendered message.
///
/// A template that names `{note}` decides where it goes. One that does not —
/// which is every default, and every config written before notes existed —
/// gets it in front, because a specific instruction is what the agent should
/// read first and the template is the context for it.
///
/// An empty note leaves the message exactly as it was, so nothing changes for
/// anyone who does not type one.
pub fn with_note(template: &str, note: &str) -> String {
    let note = note.trim();
    if template.contains("{note}") {
        return template.replace("{note}", note);
    }
    if note.is_empty() {
        return template.to_string();
    }
    format!("{note}\n\n{template}")
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
    fn a_missing_file_is_simply_no_settings() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let s = parse("# a note\n\n  \ntheme = mocha\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s["theme"], "mocha");
    }

    #[test]
    fn whitespace_around_the_equals_does_not_matter() {
        assert_eq!(parse("theme=mocha")["theme"], "mocha");
        assert_eq!(parse("  theme   =   mocha  ")["theme"], "mocha");
    }

    #[test]
    fn a_line_with_no_equals_is_ignored_rather_than_fatal() {
        let s = parse("nonsense\ntheme = design\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s["theme"], "design");
    }

    #[test]
    fn a_value_may_contain_an_equals_sign() {
        assert_eq!(parse("k = a=b")["k"], "a=b");
    }

    #[test]
    fn an_empty_value_is_kept_not_dropped() {
        // it round-trips, which matters more than what it means
        assert_eq!(parse("k =")["k"], "");
    }

    #[test]
    fn the_last_of_two_identical_keys_wins() {
        assert_eq!(parse("theme = design\ntheme = mocha")["theme"], "mocha");
    }

    #[test]
    fn what_is_rendered_parses_back_to_the_same_thing() {
        let mut s = Settings::new();
        s.insert("theme".into(), "mocha".into());
        s.insert("unknown-to-this-version".into(), "kept".into());
        assert_eq!(parse(&render(&s)), s);
    }

    #[test]
    fn every_theme_survives_a_round_trip_through_its_key() {
        for t in Theme::ALL {
            assert_eq!(Theme::from_key(t.key()), Some(t), "{}", t.name());
        }
    }

    #[test]
    fn the_default_prompt_carries_what_an_agent_needs() {
        let out = render_prompt(
            crate::subject::Subject::Issue.default_template(),
            "Osmait/sbql",
            14,
            "Fix the parser",
            "https://github.com/Osmait/sbql/issues/14",
            "It breaks on empty input.",
        );
        for expected in [
            "Osmait/sbql#14",
            "Fix the parser",
            "https://github.com/Osmait/sbql/issues/14",
            "It breaks on empty input.",
        ] {
            assert!(out.contains(expected), "{expected} missing from:\n{out}");
        }
        assert!(out.contains('\n'), "the escaped newlines became real ones");
    }

    #[test]
    fn a_placeholder_that_is_not_a_placeholder_survives() {
        // a typo should look like a typo, not like an empty string
        let out = render_prompt("{repo} {nope}", "a/b", 1, "t", "u", "ctx");
        assert_eq!(out, "a/b {nope}");
    }

    #[test]
    fn a_body_containing_braces_is_not_re_expanded() {
        // substitution runs once per placeholder, left to right, so a body
        // that happens to contain `{repo}` stays as it was written
        let out = render_prompt("{context}", "a/b", 1, "t", "u", "see {repo}");
        assert_eq!(out, "see {repo}");
    }

    #[test]
    fn the_default_agent_list_is_the_ones_on_this_machine() {
        let kinds = agent_kinds();
        for k in ["claude", "codex", "opencode", "pi"] {
            assert!(kinds.iter().any(|x| x == k), "{k} missing from {kinds:?}");
        }
    }

    #[test]
    fn an_agent_with_no_mark_of_its_own_gets_a_neutral_one() {
        // inventing a brand glyph would be decoration pretending to be
        // information; a plain mark is honest
        assert_eq!(crate::icons::agent("something-new"), "▪");
    }

    #[test]
    fn the_two_agents_that_have_a_real_mark_keep_it() {
        assert_eq!(crate::icons::agent("pi"), "π");
        assert_eq!(crate::icons::agent("claude"), "✳");
    }

    #[test]
    fn every_default_icon_is_one_column_wide() {
        // the column is fixed; a wide glyph would push the row sideways
        use unicode_width::UnicodeWidthStr;
        for kind in ["claude", "codex", "opencode", "pi", "anything"] {
            let icon = crate::icons::agent(kind);
            assert_eq!(icon.width(), 1, "{kind} draws {icon}");
        }
    }

    #[test]
    fn a_configured_icon_is_found_by_name() {
        assert_eq!(
            lookup_icon("claude=A, codex=B", "codex").as_deref(),
            Some("B")
        );
        assert_eq!(lookup_icon("claude = A", "claude").as_deref(), Some("A"));
    }

    #[test]
    fn a_name_that_is_not_configured_falls_through() {
        assert_eq!(lookup_icon("claude=A", "pi"), None);
        assert_eq!(lookup_icon("", "pi"), None);
        assert_eq!(lookup_icon("nonsense", "pi"), None);
    }

    #[test]
    fn an_icon_that_is_not_a_single_character_is_refused() {
        // it would push everything on the row sideways
        assert_eq!(lookup_icon("claude=hello", "claude"), None);
        assert_eq!(lookup_icon("claude=", "claude"), None);
    }

    #[test]
    fn a_configured_icon_may_be_a_wide_one() {
        // one character, two columns — `put` clears the cell it covers, so
        // this is the reader's call to make
        assert_eq!(lookup_icon("claude=漢", "claude").as_deref(), Some("漢"));
    }

    #[test]
    fn a_note_leads_the_message_when_the_template_does_not_place_it() {
        let out = with_note("Work on {repo}", "only the parser, ignore the tests");
        assert_eq!(out, "only the parser, ignore the tests\n\nWork on {repo}");
    }

    #[test]
    fn a_template_that_names_the_note_decides_where_it_goes() {
        let out = with_note("Context first.\n\n{note}", "then this");
        assert_eq!(out, "Context first.\n\nthen this");
    }

    #[test]
    fn no_note_leaves_the_message_exactly_as_it_was() {
        // nothing changes for anyone who never types one
        assert_eq!(with_note("Work on {repo}", ""), "Work on {repo}");
        assert_eq!(with_note("Work on {repo}", "   "), "Work on {repo}");
    }

    #[test]
    fn a_template_that_names_the_note_drops_the_placeholder_when_empty() {
        assert_eq!(with_note("a {note} b", ""), "a  b");
    }

    #[test]
    fn a_note_is_trimmed_but_not_otherwise_touched() {
        let out = with_note("T", "  spaces around, %d inside  ");
        assert!(out.starts_with("spaces around, %d inside\n"), "{out}");
    }

    #[test]
    fn an_unknown_theme_name_is_not_a_theme() {
        assert_eq!(Theme::from_key("solarized"), None);
    }

    // --- the disk, for real: the parser being right proves nothing about
    // whether a setting actually survives being written and read back ---

    /// A directory of our own under the system temp, removed on the way out.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("github-tui-test-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            Self(p)
        }

        /// A config path two levels down, so creating the parents is exercised.
        fn config(&self) -> PathBuf {
            self.0.join("nested").join(FILE)
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_setting_survives_being_written_and_read_back() {
        let tmp = Tmp::new("roundtrip");
        let p = tmp.config();

        assert!(load_from(&p).is_empty(), "nothing is there yet");
        set_at(&p, THEME, Theme::Mocha.key()).unwrap();

        assert_eq!(
            load_from(&p).get(THEME).and_then(|k| Theme::from_key(k)),
            Some(Theme::Mocha),
            "this is the restart"
        );
    }

    #[test]
    fn writing_one_setting_leaves_the_others_alone() {
        let tmp = Tmp::new("preserve");
        let p = tmp.config();

        set_at(&p, "from-a-newer-version", "keep me").unwrap();
        set_at(&p, THEME, "mocha").unwrap();
        set_at(&p, THEME, "design").unwrap();

        let s = load_from(&p);
        assert_eq!(s["from-a-newer-version"], "keep me");
        assert_eq!(s[THEME], "design", "the newer value wins");
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        let tmp = Tmp::new("clean");
        let p = tmp.config();
        set_at(&p, THEME, "mocha").unwrap();
        assert!(!p.with_extension("tmp").exists());
    }

    #[test]
    fn writing_somewhere_impossible_reports_it_rather_than_panicking() {
        let tmp = Tmp::new("denied");
        // a file where the config wants a directory
        std::fs::create_dir_all(&tmp.0).unwrap();
        let blocker = tmp.0.join("nested");
        std::fs::write(&blocker, "not a directory").unwrap();

        assert!(set_at(tmp.config(), THEME, "mocha").is_err());
    }
}
