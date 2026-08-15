//! The multiplexer: where agents run, and how to reach them.
//!
//! One coding agent per pane, somewhere. `herdr` is what this was written
//! against, but nothing above this layer knows that: the program asks a
//! `Multiplexer` for the agents and hands it prompts, and a pane id is an
//! opaque string it never reads.
//!
//! Adding a backend is implementing the six primitives below. The choreography
//! that matters — make a workspace, start an agent in it, give it the prompt,
//! and undo the workspace if either of the last two fails — is written once
//! here as a provided method, so a new backend cannot get the undo subtly
//! wrong by rewriting it.
//!
//! ## Backends that cannot see
//!
//! `AgentStatus` is the one thing not every multiplexer can answer. herdr
//! watches the pane and can say an agent is blocked on a permission prompt;
//! tmux can tell you a pane exists and what its title is, and no more. So a
//! backend declares whether it knows, rather than returning `Unknown` and
//! letting each caller guess what that means — the guess that matters is
//! `is_free`, and an unknown status read as "busy" would refuse every send.

use crate::shared::error::Result as Res;

/// What a coding agent is doing, as the multiplexer reports it.
///
/// Kept apart from `Status` even though `working` and `running` rhyme: an
/// agent waiting for you to answer a permission prompt has no equivalent in
/// the issue/PR/CI vocabulary, and conflating the two would lose exactly the
/// state you most want to see.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AgentStatus {
    /// Busy on a task.
    Working,
    /// Waiting for something to do.
    Idle,
    /// Stopped on a question — a permission prompt, a choice.
    Blocked,
    /// Finished what it was asked.
    Done,
    #[default]
    Unknown,
}

impl AgentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Idle => "idle",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "working" => Self::Working,
            "idle" => Self::Idle,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            _ => Self::Unknown,
        }
    }

    /// Can it be given something new to do?
    ///
    /// Typing into an agent mid-task is how you lose its context, and one
    /// stopped on a permission prompt would read the prompt as the answer.
    pub fn is_free(self) -> bool {
        matches!(self, Self::Idle | Self::Done)
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A coding agent running somewhere on this machine.
#[derive(Clone)]
pub struct Agent {
    /// `claude`, `codex`, `pi` — what is running, not what it is called.
    pub kind: String,
    pub status: AgentStatus,
    /// Where it is working. The only clue to which repository it is in.
    pub cwd: String,
    /// Whatever the multiplexer calls the place it is running — `wK:p1` for
    /// herdr, `%17` for tmux. Opaque here: this is the dispatch target and
    /// nothing else, and nothing above this layer should read it.
    pub pane: String,
    /// The terminal title, which for most agents is a summary of the task.
    pub title: String,
    /// True for the pane the reader is looking at — which, when this program
    /// is run from inside the multiplexer, is this program.
    pub focused: bool,
}

impl Agent {
    /// The last segment of `cwd`, which is the repository often enough to be
    /// worth showing and never worth trusting.
    pub fn where_short(&self) -> &str {
        self.cwd.rsplit('/').next().unwrap_or(&self.cwd)
    }
}

// --- the seam ---------------------------------------------------------------

/// Somewhere agents run, that this program can reach.
///
/// `Sync` because the worker thread is what calls it, and `'static` because
/// there is one for the life of the process — a multiplexer is a fact about
/// the machine, not something a screen owns.
pub trait Multiplexer: Sync + 'static {
    /// What to call it, for a message that has to name it.
    fn name(&self) -> &'static str;

    /// Is it here at all? Asked before anything is offered, so that a machine
    /// without one says so instead of failing at the moment of dispatch.
    fn available(&self) -> bool;

    /// Every agent it can see.
    fn agents(&self) -> Res<Vec<Agent>>;

    /// Types `text` into the agent in `pane`.
    fn prompt(&self, pane: &str, text: &str) -> Res<()>;

    /// Opens a place to work in `cwd` and returns its pane.
    fn create_workspace(&self, cwd: &str, label: &str) -> Res<String>;

    /// Opens a place to work on a *new branch* of `repo_root`, checked out
    /// somewhere of its own, and returns its pane.
    ///
    /// Separate from `create_workspace` because undoing it is different: this
    /// one made a checkout that has to be removed, that one only opened a
    /// window onto a checkout that was already there.
    fn create_worktree(&self, repo_root: &str, branch: &str, label: &str) -> Res<String>;

    /// Undoes `create_workspace`. Touches no files.
    fn close_workspace(&self, pane: &str) -> Res<()>;

    /// Undoes `create_worktree`, including the checkout it made.
    fn remove_worktree(&self, pane: &str) -> Res<()>;

    /// Starts an interactive agent of `kind` in an existing pane.
    ///
    /// `kind` is a name — `claude`, `codex` — and how to launch it is the
    /// backend's problem, not this program's. That is the whole reason there
    /// is no abstraction over agents up here: there is nothing to abstract.
    fn start_agent(&self, pane: &str, kind: &str) -> Res<()>;

    // --- provided ---

    /// Whether `agents()` fills in a real status.
    ///
    /// A backend that watches its panes says yes. One that can only list them
    /// says no, and everything that reads a status treats `Unknown` as "no
    /// answer" rather than as "busy".
    fn detects_status(&self) -> bool {
        true
    }

    /// Can this agent be given something new to do?
    ///
    /// The one place the answer is decided. Typing into an agent mid-task is
    /// how you lose its context, and one stopped on a permission prompt would
    /// read the prompt as the answer — so a known-busy agent is refused. But a
    /// backend that cannot tell must not have every send refused on its
    /// behalf: not knowing is not the same as knowing it is busy.
    fn is_free(&self, agent: &Agent) -> bool {
        if !self.detects_status() {
            return true;
        }
        agent.status.is_free()
    }

    /// A workspace, an agent in it, and the prompt — undoing the workspace if
    /// either of the last two fails.
    ///
    /// Provided rather than required: every backend needs exactly this
    /// sequence, and leaving a half-built workspace behind is worse than the
    /// failure that caused it — the next dispatch would offer a branch that
    /// already exists, and the reader would have a workspace they did not ask
    /// for and did not see appear.
    fn dispatch(
        &self,
        cwd: &str,
        branch: Option<&str>,
        label: &str,
        kind: &str,
        text: &str,
    ) -> Res<()> {
        let pane = match branch {
            Some(b) => self.create_worktree(cwd, b, label)?,
            None => self.create_workspace(cwd, label)?,
        };
        let undo = |pane: &str| {
            let _ = if branch.is_some() {
                self.remove_worktree(pane)
            } else {
                self.close_workspace(pane)
            };
        };

        if let Err(e) = self.start_agent(&pane, kind) {
            undo(&pane);
            return Err(e);
        }
        if let Err(e) = self.prompt(&pane, text) {
            undo(&pane);
            return Err(e);
        }
        Ok(())
    }
}

/// The backends that exist, in the order the picker would show them.
pub fn all() -> &'static [&'static dyn Multiplexer] {
    &[&crate::shared::herdr::Herdr]
}

/// The one in use.
///
/// Chosen once from `multiplexer = <name>` in the config, falling back to the
/// first that is actually on this machine — a name for something that is not
/// installed is worse than no answer, because everything it is asked returns
/// a spawn failure instead of an empty list.
pub fn current() -> &'static dyn Multiplexer {
    use std::sync::OnceLock;
    static CHOSEN: OnceLock<&'static dyn Multiplexer> = OnceLock::new();
    *CHOSEN.get_or_init(|| {
        let want = crate::shared::config::multiplexer();
        all()
            .iter()
            .find(|m| m.name() == want)
            .or_else(|| all().iter().find(|m| m.available()))
            .copied()
            .unwrap_or(all()[0])
    })
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
    use std::sync::Mutex;

    /// A backend that does nothing but write down what it was asked.
    ///
    /// The point of it is that it exists: everything below is what a real
    /// second multiplexer would have to write, and nothing else. If adding one
    /// ever needs more than these methods, this stops compiling.
    #[derive(Default)]
    struct Fake {
        /// What can only list panes, not watch them — tmux's position.
        blind: bool,
        fail_start: bool,
        fail_prompt: bool,
        log: Mutex<Vec<String>>,
    }

    impl Fake {
        fn note(&self, what: &str) {
            if let Ok(mut l) = self.log.lock() {
                l.push(what.to_string());
            }
        }
        fn log(&self) -> Vec<String> {
            self.log.lock().map(|l| l.clone()).unwrap_or_default()
        }
        fn boom() -> crate::shared::error::Error {
            crate::shared::error::Error::Spawn {
                program: "fake",
                source: std::io::Error::from(std::io::ErrorKind::Other),
            }
        }
    }

    impl Multiplexer for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        fn available(&self) -> bool {
            true
        }
        fn agents(&self) -> Res<Vec<Agent>> {
            Ok(Vec::new())
        }
        fn prompt(&self, pane: &str, _text: &str) -> Res<()> {
            self.note(&format!("prompt {pane}"));
            if self.fail_prompt {
                return Err(Self::boom());
            }
            Ok(())
        }
        fn create_workspace(&self, _cwd: &str, _label: &str) -> Res<String> {
            self.note("create_workspace");
            Ok("w1:p1".into())
        }
        fn create_worktree(&self, _root: &str, _branch: &str, _label: &str) -> Res<String> {
            self.note("create_worktree");
            Ok("w2:p1".into())
        }
        fn close_workspace(&self, pane: &str) -> Res<()> {
            self.note(&format!("close_workspace {pane}"));
            Ok(())
        }
        fn remove_worktree(&self, pane: &str) -> Res<()> {
            self.note(&format!("remove_worktree {pane}"));
            Ok(())
        }
        fn start_agent(&self, pane: &str, _kind: &str) -> Res<()> {
            self.note(&format!("start_agent {pane}"));
            if self.fail_start {
                return Err(Self::boom());
            }
            Ok(())
        }
        fn detects_status(&self) -> bool {
            !self.blind
        }
    }

    fn agent(status: AgentStatus) -> Agent {
        Agent {
            kind: "claude".into(),
            status,
            cwd: "/tmp/r".into(),
            pane: "w1:p1".into(),
            title: String::new(),
            focused: false,
        }
    }

    #[test]
    fn a_backend_gets_the_whole_dispatch_from_six_primitives() {
        let m = Fake::default();
        m.dispatch("/tmp/r", None, "label", "claude", "do it")
            .unwrap();
        assert_eq!(
            m.log(),
            ["create_workspace", "start_agent w1:p1", "prompt w1:p1"],
            "the order is the trait's, not the backend's"
        );
    }

    #[test]
    fn a_workspace_is_undone_when_what_follows_it_fails() {
        // The reason `dispatch` is provided rather than required: a backend
        // that rewrote this could leave a half-built workspace behind, and the
        // next dispatch would offer a branch that already exists.
        let m = Fake {
            fail_start: true,
            ..Fake::default()
        };
        assert!(m.dispatch("/tmp/r", None, "l", "claude", "x").is_err());
        assert_eq!(
            m.log().last().map(String::as_str),
            Some("close_workspace w1:p1")
        );

        let m = Fake {
            fail_prompt: true,
            ..Fake::default()
        };
        assert!(m.dispatch("/tmp/r", None, "l", "claude", "x").is_err());
        assert_eq!(
            m.log().last().map(String::as_str),
            Some("close_workspace w1:p1")
        );
    }

    #[test]
    fn a_worktree_is_removed_rather_than_merely_closed() {
        // Undoing them is not the same: one made a checkout, the other only
        // opened a window onto one that was already there.
        let m = Fake {
            fail_start: true,
            ..Fake::default()
        };
        assert!(m.dispatch("/tmp/r", Some("b"), "l", "claude", "x").is_err());
        assert_eq!(
            m.log().last().map(String::as_str),
            Some("remove_worktree w2:p1")
        );
    }

    #[test]
    fn a_backend_that_cannot_see_does_not_refuse_every_send() {
        // The leak this trait exists to close. `Unknown` is not `busy`: a
        // multiplexer that can only list panes would otherwise have every
        // dispatch refused on the grounds that it could not prove the agent
        // was idle.
        let seeing = Fake::default();
        assert!(!seeing.is_free(&agent(AgentStatus::Unknown)));
        assert!(!seeing.is_free(&agent(AgentStatus::Working)));
        assert!(seeing.is_free(&agent(AgentStatus::Idle)));

        let blind = Fake {
            blind: true,
            ..Fake::default()
        };
        assert!(blind.is_free(&agent(AgentStatus::Unknown)));
    }

    #[test]
    fn a_backend_that_can_see_still_refuses_a_busy_agent() {
        // And the guarantee is not lost in the process: typing into an agent
        // mid-task is how you lose its context.
        let m = Fake::default();
        assert!(!m.is_free(&agent(AgentStatus::Blocked)));
        assert!(!m.is_free(&agent(AgentStatus::Working)));
    }

    #[test]
    fn the_one_in_use_is_the_one_that_is_here() {
        let m = current();
        assert!(all().iter().any(|b| b.name() == m.name()));
    }
}
