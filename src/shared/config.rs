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

use super::settings::current as store;

/// Where the settings file lives, for the things filed beside it.
pub fn path() -> Option<std::path::PathBuf> {
    store().path()
}

use std::io;

use crate::tui::theme::Theme;

// ------------------------------------------------------------------- theme

const THEME: &str = "theme";

/// Applies the saved theme, if there is one. Called once at startup.
pub fn apply_theme() {
    if let Some(t) = store().get(THEME).and_then(|k| Theme::from_key(&k)) {
        crate::tui::theme::set(t);
    }
}

/// Writes the theme down for the next run.
///
/// The failure is returned rather than swallowed, and the caller shows it: a
/// theme that could not be saved is still applied now, so forgetting silently
/// would look like the setting had not been taken.
pub fn save_theme(theme: Theme) -> io::Result<()> {
    store().set(THEME, theme.key())
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
    store()
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

const MUX: &str = "multiplexer";

/// Which multiplexer to talk to. Unset means "whichever is here".
pub fn multiplexer() -> String {
    store().get(MUX).unwrap_or_default()
}

const AGENTS: &str = "agents";

/// The agents offered for a fresh worktree.
///
/// A setting because herdr decides what it can start, not this program, and
/// the list grows. An unsupported name is not rejected here — herdr's own
/// refusal is a better message than a guess at one.
const DEFAULT_AGENTS: &str = "claude, codex, opencode, pi";

/// The agent kinds to offer, in the order the setting lists them.
///
/// Order is the setting's, because the first one is what the picker lands on.
/// Empty entries are dropped, so a trailing comma is a trailing comma rather
/// than a nameless agent in the list.
pub fn agent_kinds() -> Vec<String> {
    store()
        .get(AGENTS)
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
    store()
        .get(ICONS)
        .and_then(|spec| lookup_icon(&spec, kind))
        .unwrap_or_else(|| crate::shared::icons::agent(kind).to_string())
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
pub fn file_icons() -> crate::shared::icons::Style {
    store()
        .get("file-icons")
        .map_or(crate::shared::icons::Style::Nerd, |v| {
            crate::shared::icons::Style::parse(&v)
        })
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
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_simply_no_settings() {
        assert!(crate::shared::settings::parse("").is_empty());
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let s = crate::shared::settings::parse("# a note\n\n  \ntheme = mocha\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s["theme"], "mocha");
    }

    #[test]
    fn whitespace_around_the_equals_does_not_matter() {
        assert_eq!(
            crate::shared::settings::parse("theme=mocha")["theme"],
            "mocha"
        );
        assert_eq!(
            crate::shared::settings::parse("  theme   =   mocha  ")["theme"],
            "mocha"
        );
    }

    #[test]
    fn a_line_with_no_equals_is_ignored_rather_than_fatal() {
        let s = crate::shared::settings::parse("nonsense\ntheme = design\n");
        assert_eq!(s.len(), 1);
        assert_eq!(s["theme"], "design");
    }

    #[test]
    fn a_value_may_contain_an_equals_sign() {
        assert_eq!(crate::shared::settings::parse("k = a=b")["k"], "a=b");
    }

    #[test]
    fn an_empty_value_is_kept_not_dropped() {
        // it round-trips, which matters more than what it means
        assert_eq!(crate::shared::settings::parse("k =")["k"], "");
    }

    #[test]
    fn the_last_of_two_identical_keys_wins() {
        assert_eq!(
            crate::shared::settings::parse("theme = design\ntheme = mocha")["theme"],
            "mocha"
        );
    }

    #[test]
    fn every_theme_survives_a_round_trip_through_its_key() {
        for t in Theme::all() {
            assert_eq!(Theme::from_key(t.key()), Some(t), "{}", t.name());
        }
    }

    #[test]
    fn the_default_prompt_carries_what_an_agent_needs() {
        // A template written out here rather than borrowed from the other
        // program's `Subject`: this module is shared, and what it does with a
        // template is not a fact about issues.
        let out = render_prompt(
            "Work on {repo}#{num}: {title}\n{url}\n\n{context}",
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
        assert_eq!(crate::shared::icons::agent("something-new"), "▪");
    }

    #[test]
    fn the_two_agents_that_have_a_real_mark_keep_it() {
        assert_eq!(crate::shared::icons::agent("pi"), "π");
        assert_eq!(crate::shared::icons::agent("claude"), "✳");
    }

    #[test]
    fn every_default_icon_is_one_column_wide() {
        // the column is fixed; a wide glyph would push the row sideways
        use unicode_width::UnicodeWidthStr;
        for kind in ["claude", "codex", "opencode", "pi", "anything"] {
            let icon = crate::shared::icons::agent(kind);
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
}
