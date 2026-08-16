//! The process entry point for `github-tui`.
//!
//! Process setup lives beside this file so the entry point says only which
//! runtime owns the binary.
#![allow(clippy::print_stdout, reason = "help and --version are stdout's job")]

#[path = "github-tui/program.rs"]
mod program;
#[path = "github-tui/runtime.rs"]
mod runtime;

fn main() -> std::io::Result<()> {
    runtime::run()
}
