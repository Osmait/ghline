//! Where settings come from.
//!
//! `config` read the file on every call, and nothing above it knew that. Two
//! views ask for an agent's icon inside the loop that draws the rows, so the
//! picker was reading the config file once per agent per frame — sixty times
//! a second each while anything animated. Nobody wrote that; it is what
//! "reach for the filesystem wherever you happen to need a value" adds up to.
//!
//! Behind a trait, the reading has somewhere to live and somewhere to be
//! cached, and a caller that wants a value asks the store rather than the
//! disk. The store is chosen once and never changes underneath.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Somewhere settings live.
pub trait Store: Sync + Send + 'static {
    /// The value for `key`, if it has one.
    fn get(&self, key: &str) -> Option<String>;

    /// Remembers `value` under `key`.
    fn set(&self, key: &str, value: &str) -> io::Result<()>;

    /// Where it keeps them, when that is a place. `None` for a store with no
    /// file behind it, which is what a test uses.
    fn path(&self) -> Option<PathBuf> {
        None
    }
}

/// The settings file: `$XDG_CONFIG_HOME/github-tui/config`, or `~/.config`.
///
/// Read once and kept. Nothing else writes that file while the program runs
/// except this, so re-reading it would only cost what it used to cost.
pub struct Files;

/// The parsed file, read the first time anything asks.
static CACHE: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn cache() -> &'static RwLock<HashMap<String, String>> {
    CACHE.get_or_init(|| RwLock::new(read_file()))
}

fn read_file() -> HashMap<String, String> {
    let Some(p) = config_path() else {
        return HashMap::new();
    };
    std::fs::read_to_string(p)
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// `key = value`, one per line, `#` starts a comment.
pub fn parse(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            Some((k.trim().to_string(), v.trim().to_string()))
        })
        .collect()
}

/// `None` when neither variable is set — an environment with no home to write
/// to is one where settings are not remembered, which is a fact and not a
/// failure.
pub fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("github-tui").join("config"))
}

impl Store for Files {
    fn get(&self, key: &str) -> Option<String> {
        cache().read().ok()?.get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) -> io::Result<()> {
        let p = config_path()
            .ok_or_else(|| io::Error::other("no HOME or XDG_CONFIG_HOME to write to"))?;
        // Read-modify-write rather than writing the one key: two settings
        // changed in one session must not lose each other.
        let mut all = read_file();
        all.insert(key.to_string(), value.to_string());
        write_all(&p, &all)?;
        if let Ok(mut c) = cache().write() {
            c.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }

    fn path(&self) -> Option<PathBuf> {
        config_path()
    }
}

/// Writes the whole file, beside the target and renamed over it, so an
/// interrupted write cannot leave half a config where a config was.
fn write_all(p: &std::path::Path, all: &HashMap<String, String>) -> io::Result<()> {
    use std::io::Write;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut keys: Vec<&String> = all.keys().collect();
    keys.sort();
    let mut out = String::from("# github-tui. Written by the application; edit freely.\n");
    for k in keys {
        if let Some(v) = all.get(k) {
            out.push_str(&format!("{k} = {v}\n"));
        }
    }
    let tmp = p.with_extension("tmp");
    std::fs::File::create(&tmp)?.write_all(out.as_bytes())?;
    std::fs::rename(&tmp, p)
}

/// A store that remembers nothing beyond itself, for tests and for the
/// headless render modes, which must draw the same frame on any machine.
#[derive(Default)]
pub struct Memory {
    values: RwLock<HashMap<String, String>>,
}

impl Memory {
    pub fn with(pairs: &[(&str, &str)]) -> Self {
        let m = Self::default();
        for (k, v) in pairs {
            let _ = m.set(k, v);
        }
        m
    }
}

impl Store for Memory {
    fn get(&self, key: &str) -> Option<String> {
        self.values.read().ok()?.get(key).cloned()
    }

    fn set(&self, key: &str, value: &str) -> io::Result<()> {
        if let Ok(mut v) = self.values.write() {
            v.insert(key.to_string(), value.to_string());
        }
        Ok(())
    }
}

/// Reads a file that sits beside the settings — the keymap, a theme.
///
/// Here because "a file under the config directory" is one responsibility
/// and this module has it. The alternative was every module that owns such a
/// file also owning a `std::fs` call, which is how the state layer ended up
/// reading from disk.
pub fn read_beside(name: &str) -> Option<String> {
    let p = current().path()?.with_file_name(name);
    std::fs::read_to_string(p).ok()
}

/// Where a file beside the settings would go.
pub fn path_beside(name: &str) -> Option<PathBuf> {
    Some(current().path()?.with_file_name(name))
}

static CHOSEN: OnceLock<Box<dyn Store>> = OnceLock::new();

/// The store in use. The file, unless something said otherwise first.
pub fn current() -> &'static dyn Store {
    CHOSEN.get_or_init(|| Box::new(Files)).as_ref()
}

/// Uses `store` instead of the file.
///
/// Only takes if nothing has asked for the store yet, which is why the
/// binaries do it before anything else. Returns whether it took, so a caller
/// that cares is told rather than left assuming.
pub fn use_store(store: Box<dyn Store>) -> bool {
    CHOSEN.set(store).is_ok()
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
    fn a_memory_store_remembers_what_it_was_told() {
        let m = Memory::with(&[("theme", "mocha")]);
        assert_eq!(m.get("theme").as_deref(), Some("mocha"));
        assert_eq!(m.get("nothing"), None);
        m.set("theme", "design").unwrap();
        assert_eq!(m.get("theme").as_deref(), Some("design"));
    }

    #[test]
    fn a_memory_store_has_no_file_behind_it() {
        // Which is the point: a snapshot must draw the same frame on any
        // machine, and a store that found somebody's config would not.
        assert_eq!(Memory::default().path(), None);
    }

    #[test]
    fn parsing_skips_comments_and_blank_lines() {
        let all = parse("# a note\n\ntheme = mocha\n  agents = claude, pi  \n");
        assert_eq!(all.get("theme").map(String::as_str), Some("mocha"));
        assert_eq!(all.get("agents").map(String::as_str), Some("claude, pi"));
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn a_value_may_contain_the_separator() {
        // `agent-icons = claude=✳` splits once, not on every `=`.
        let all = parse("agent-icons = claude=A, pi=B\n");
        assert_eq!(
            all.get("agent-icons").map(String::as_str),
            Some("claude=A, pi=B")
        );
    }

    #[test]
    fn a_line_with_no_separator_is_skipped_rather_than_guessed_at() {
        assert!(parse("nonsense\n").is_empty());
    }

    // --- the disk, for real: the parser being right proves nothing about
    // whether a setting survives being written and read back ---

    /// A directory of our own under the system temp, removed on the way out.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(tag: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("github-tui-test-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            Self(p)
        }

        /// Two levels down, so creating the parents is exercised.
        fn config(&self) -> PathBuf {
            self.0.join("nested").join("config")
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(p: &std::path::Path, pairs: &[(&str, &str)]) -> io::Result<()> {
        let all: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        write_all(p, &all)
    }

    #[test]
    fn a_setting_survives_being_written_and_read_back() {
        let tmp = Tmp::new("roundtrip");
        let p = tmp.config();
        write(&p, &[("theme", "mocha")]).expect("written");
        let back = parse(&std::fs::read_to_string(&p).expect("read"));
        assert_eq!(back.get("theme").map(String::as_str), Some("mocha"));
    }

    #[test]
    fn a_key_this_version_does_not_know_is_not_dropped() {
        // A config written by a newer build has to survive an older one.
        let tmp = Tmp::new("unknown");
        let p = tmp.config();
        write(
            &p,
            &[("from-a-newer-version", "keep me"), ("theme", "mocha")],
        )
        .expect("written");
        let back = parse(&std::fs::read_to_string(&p).expect("read"));
        assert_eq!(
            back.get("from-a-newer-version").map(String::as_str),
            Some("keep me")
        );
    }

    #[test]
    fn no_temporary_file_is_left_behind() {
        // It is written beside the target and renamed over it, so an
        // interrupted write cannot leave half a config where a config was —
        // but a successful one must not leave the halfway house either.
        let tmp = Tmp::new("leftover");
        let p = tmp.config();
        write(&p, &[("theme", "mocha")]).expect("written");
        assert!(!p.with_extension("tmp").exists());
    }

    #[test]
    fn writing_where_there_is_no_home_is_an_error_rather_than_a_panic() {
        let p = std::path::Path::new("/proc/nonexistent/config");
        assert!(write(p, &[("theme", "mocha")]).is_err());
    }
}
