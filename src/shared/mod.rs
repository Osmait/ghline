//! The parts neither program owns.
//!
//! A module here is one both `diffline` and `github` read, and the rule that
//! keeps it honest is one arrow: nothing in this directory may name anything
//! in either of theirs. That has been broken three times and cost the same
//! each time — `herdr` reaching into github-tui's `data` for the agent types,
//! `theme` reaching into it for a check status, `config` for the shape of a
//! prompt — so the boundary is a directory now rather than a comment.

pub mod ago;
pub mod clones;
pub mod config;
pub mod error;
pub mod fuzzy;
pub mod herdr;
pub mod icons;
pub mod key;
pub mod mux;
pub mod nav;
pub mod settings;
pub mod syntax;
pub mod text;

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
