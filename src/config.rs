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
