//! The process entry point for `diffline`.
//!
//! Process setup lives beside this file so the entry point says only which
//! runtime owns the binary.
#![allow(clippy::print_stdout, reason = "help and --svg are stdout's job")]

#[path = "diffline/program.rs"]
mod program;
#[path = "diffline/runtime.rs"]
mod runtime;

fn main() -> std::io::Result<()> {
    runtime::run()
}
