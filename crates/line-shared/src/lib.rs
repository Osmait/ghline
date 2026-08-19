//! The application parts neither `ghline` nor `diffline` owns.
//!
//! A module here is one both programs read, and the rule that keeps it honest
//! is one arrow: this crate cannot depend on either application crate. That
//! has been broken three times and cost the same each time — agent dispatch
//! reaching into ghline's data, themes reaching into it for a check status,
//! and configuration reaching into it for a prompt — so Cargo owns the
//! boundary now rather than a comment.

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
pub use agent_mux::herdr;
pub use fuzzy_match as fuzzy;
pub use process_error as error;
pub mod icons;
pub use tui_kit::key;
pub mod log;
pub mod mux;
pub mod nav;
pub mod settings;
pub use source_text::{syntax, text};
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
        // Cargo enforces this for compiled imports. The source check also
        // catches a future path smuggled in behind conditional compilation.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut guilty = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("the shared directory is there") {
            let path = entry.expect("readable").path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            // this file, which has to name them to look for them
            if path.file_name().is_some_and(|n| n == "lib.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains("diffline_app") || line.contains("ghline_app") {
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
