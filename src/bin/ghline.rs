//! The process entry point for `ghline`.
//!
//! Process setup lives beside this file so the entry point says only which
//! runtime owns the binary.
#![allow(clippy::print_stdout, reason = "help and --version are stdout's job")]

#[path = "ghline/program.rs"]
mod program;
#[path = "ghline/runtime.rs"]
mod runtime;

fn main() -> std::io::Result<()> {
    runtime::run()
}
