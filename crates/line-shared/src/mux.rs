//! The application-owned choice of an agent multiplexer.
//!
//! Agent discovery and dispatch live in `agent-mux`; only selecting a backend
//! from this application's settings remains here.

pub use agent_mux::{Agent, AgentStatus, Multiplexer, all, select};

/// The configured multiplexer, chosen once for the process.
pub fn current() -> &'static dyn Multiplexer {
    use std::sync::OnceLock;

    static CHOSEN: OnceLock<&'static dyn Multiplexer> = OnceLock::new();
    *CHOSEN.get_or_init(|| select(&crate::config::multiplexer()))
}
