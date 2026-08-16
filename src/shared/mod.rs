//! The parts neither program owns.
//!
//! A module here is one both `diffline` and `github` read, and the rule that
//! keeps it honest is one arrow: nothing in this directory may name anything
//! in either of theirs. That has been broken three times and cost the same
//! each time — `herdr` reaching into github-tui's `data` for the agent types,
//! `theme` reaching into it for a check status, `config` for the shape of a
//! prompt — so the boundary is a directory now rather than a comment.

// The same reasoning as the boundary above, one step further: code with two
// callers is read by people who did not write it, and a field whose meaning
// is guessed at from its name is how `Span::from` gets sliced as characters
// once. Every item here carries a `///` today, and the lint is what keeps
// the next one from not. Scoped to this directory and `tui` rather than set
// crate-wide — the two programs are read by whoever is changing them, which
// is a different job.
#![warn(missing_docs)]

pub mod ago;
pub mod clones;
pub mod config;
pub mod error;
pub mod fuzzy;
pub mod herdr;
pub mod icons;
pub mod key;
pub mod log;
pub mod mux;
pub mod nav;
pub mod settings;
pub mod syntax;
pub mod text;
pub mod worker;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    #[test]
    fn nothing_shared_names_either_program() {
        // The rule the directory exists for, and it has been broken three
        // times: `herdr` reached into github-tui's `data` for the agent
        // types, `theme` for a check status, `config` for a prompt. Each was
        // found by reading rather than by anything failing.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shared");
        let mut guilty = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("the shared directory is there") {
            let path = entry.expect("readable").path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // this file, which has to name them to look for them
            if path.file_name().is_some_and(|n| n == "mod.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains("crate::diffline") || line.contains("crate::github") {
                    guilty.push(format!(
                        "{}:{}: {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        n + 1,
                        line.trim()
                    ));
                }
            }
        }
        assert!(
            guilty.is_empty(),
            "shared code must not name a program:\n  {}",
            guilty.join("\n  ")
        );
    }
}
