//! Invoking the `herdr` CLI and translating its JSON into the app's model.
//!
//! The same shape as `gh.rs`, for the same reason: a program that is already
//! there, speaks JSON, and blocks. Everything here runs on the service thread.
//!
//! One thing is worth knowing before reading further. herdr wraps its answers
//! in an envelope — `{"id":…,"result":…}` on success, `{"id":…,"error":…}` on
//! failure — and it prints the failure envelope on *stdout* with a zero exit
//! status. So a call that went wrong looks exactly like a call that went right
//! until the envelope is opened, which is why nothing here reads `stdout`
//! directly.

use std::process::Command;

use serde_json::Value;

pub use process_error::{Error, Result as Res};

use crate::{Agent, AgentStatus, Multiplexer};

/// herdr, the multiplexer this was written against.
///
/// A unit struct: there is nothing to configure and nothing to hold open — a
/// call is a process, and the state lives in herdr rather than here.
pub struct Herdr;

impl Multiplexer for Herdr {
    fn name(&self) -> &'static str {
        "herdr"
    }

    fn available(&self) -> bool {
        available()
    }

    fn agents(&self) -> Res<Vec<Agent>> {
        agents()
    }

    fn prompt(&self, pane: &str, text: &str) -> Res<()> {
        prompt(pane, text)
    }

    fn create_workspace(&self, cwd: &str, label: &str) -> Res<String> {
        create_workspace(cwd, label)
    }

    fn create_worktree(&self, repo_root: &str, branch: &str, label: &str) -> Res<String> {
        create_worktree(repo_root, branch, label)
    }

    fn close_workspace(&self, pane: &str) -> Res<()> {
        close_workspace(pane)
    }

    fn remove_worktree(&self, pane: &str) -> Res<()> {
        remove_worktree(pane)
    }

    fn start_agent(&self, pane: &str, kind: &str) -> Res<()> {
        start_agent(pane, kind)
    }

    // herdr watches its panes, so the status it reports is a real one — the
    // whole reason this is a capability rather than an assumption.
    fn detects_status(&self) -> bool {
        true
    }
}

/// The leading arguments identify the call in error messages.
/// How a failed call is named in an error. Includes the program, because
/// three of them share one error type and "`pr list` failed" does not say
/// which of them was run.
fn label(args: &[&str]) -> String {
    std::iter::once("herdr")
        .chain(
            args.iter()
                .take_while(|a| !a.starts_with('-'))
                .take(3)
                .copied(),
        )
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `herdr` and returns whatever its envelope carried in `result`.
fn call(args: &[&str]) -> Res<Value> {
    let out = Command::new("herdr")
        .args(args)
        .output()
        .map_err(|source| Error::Spawn {
            program: "herdr",
            install: None,
            source,
        })?;

    // A dead server or a bad flag fails the usual way, with a status and a
    // line on stderr rather than an envelope.
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(Error::Command {
            args: label(args),
            status: out.status.code(),
            stderr: if stderr.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).into_owned()
            } else {
                stderr
            },
        });
    }

    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    let v: Value = serde_json::from_str(&raw).map_err(|e| Error::Json {
        args: label(args),
        source: e,
    })?;
    envelope(&v, args)
}

/// Opens the envelope: `result` on success, `error.message` on failure.
fn envelope(v: &Value, args: &[&str]) -> Res<Value> {
    if let Some(err) = v.get("error") {
        let message = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("herdr refused the call");
        return Err(Error::Command {
            args: label(args),
            status: None,
            stderr: message.to_string(),
        });
    }
    // a command with nothing to report still succeeded
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Is there a herdr, with a server behind it?
///
/// Both halves matter: the binary can be installed with nothing running, and
/// every call would then fail one at a time instead of once.
pub fn available() -> bool {
    Command::new("herdr")
        .arg("status")
        .output()
        .map(|o| {
            o.status.success() && String::from_utf8_lossy(&o.stdout).contains("status: running")
        })
        .unwrap_or(false)
}

/// Every agent herdr is running, in the order it reports them.
pub fn agents() -> Res<Vec<Agent>> {
    if !available() {
        return Err(Error::Command {
            args: "herdr status".into(),
            status: None,
            stderr: "no herdr server running — start one with `herdr`".into(),
        });
    }
    let v = call(&["agent", "list"])?;
    Ok(v.get("agents")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| Agent {
            kind: s(a, "agent"),
            status: AgentStatus::parse(&s(a, "agent_status")),
            // `foreground_cwd` follows the process; `cwd` is where the pane
            // was opened. The first is the truer answer to "where is it
            // working", and falls back to the second when nothing is running.
            cwd: match s(a, "foreground_cwd") {
                x if x.is_empty() => s(a, "cwd"),
                x => x,
            },
            pane: s(a, "pane_id"),
            // most agents put a summary of the task in the title; the stripped
            // form has the spinner glyph taken off the front
            title: match s(a, "terminal_title_stripped") {
                x if x.is_empty() => s(a, "terminal_title"),
                x => x,
            },
            focused: a.get("focused").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect())
}

/// Sends `text` to the agent in `pane`, and does not wait for it.
///
/// Waiting would block the service thread for as long as the agent takes,
/// which can be many minutes; the Agents tab is how you watch it instead.
pub fn prompt(pane: &str, text: &str) -> Res<()> {
    call(&["agent", "prompt", pane, text]).map(|_| ())
}

/// Creates a worktree of `repo_root` on `branch` and returns the pane herdr
/// opened for it, ready for an agent to be started in.
///
/// `--no-focus` on purpose: dispatching from here should not yank the reader's
/// terminal over to the new workspace mid-sentence.
pub fn create_worktree(repo_root: &str, branch: &str, label: &str) -> Res<String> {
    let v = call(&[
        "worktree",
        "create",
        "--cwd",
        repo_root,
        "--branch",
        branch,
        "--label",
        label,
        "--no-focus",
    ])?;
    let pane = v
        .pointer("/root_pane/pane_id")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    if pane.is_empty() {
        return Err(Error::Field {
            args: "herdr worktree create".into(),
            field: "root_pane.pane_id",
        });
    }
    Ok(pane)
}

/// Opens a workspace on an existing checkout and returns its pane.
///
/// The counterpart to `create_worktree` for working where you already are:
/// same shape of answer, no branch, and nothing created on disk.
pub fn create_workspace(cwd: &str, label: &str) -> Res<String> {
    let v = call(&[
        "workspace",
        "create",
        "--cwd",
        cwd,
        "--label",
        label,
        "--no-focus",
    ])?;
    let pane = v
        .pointer("/root_pane/pane_id")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();
    if pane.is_empty() {
        return Err(Error::Field {
            args: "herdr workspace create".into(),
            field: "root_pane.pane_id",
        });
    }
    Ok(pane)
}

/// Undoes `create_workspace`. Unlike removing a worktree this touches no
/// files: the checkout was already there and stays exactly as it was.
pub fn close_workspace(pane: &str) -> Res<()> {
    let workspace = pane.split(':').next().unwrap_or(pane);
    call(&["workspace", "close", workspace]).map(|_| ())
}

/// Starts an interactive agent of `kind` in an existing pane.
pub fn start_agent(pane: &str, kind: &str) -> Res<()> {
    call(&["agent", "start", kind, "--kind", kind, "--pane", pane]).map(|_| ())
}

/// Undoes `create_worktree`, for when what came after it failed.
pub fn remove_worktree(pane: &str) -> Res<()> {
    // a pane id is `<workspace>:<pane>`, and removal takes the workspace
    let workspace = pane.split(':').next().unwrap_or(pane);
    call(&["worktree", "remove", "--workspace", workspace, "--force"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Res<Value> {
        envelope(&serde_json::from_str(raw).unwrap(), &["agent", "list"])
    }

    #[test]
    fn a_result_envelope_is_opened() {
        let v = parse(r#"{"id":"x","result":{"agents":[]}}"#).unwrap();
        assert!(v.get("agents").is_some());
    }

    #[test]
    fn an_error_envelope_becomes_an_error_despite_the_zero_exit() {
        // the case that would otherwise be silent
        let e =
            parse(r#"{"id":"x","error":{"code":"nope","message":"agent target pi not found"}}"#)
                .unwrap_err();
        assert!(
            e.to_string().contains("agent target pi not found"),
            "the message herdr gave should survive: {e}"
        );
    }

    #[test]
    fn an_error_with_no_message_still_reads_as_a_refusal() {
        let e = parse(r#"{"id":"x","error":{"code":"nope"}}"#).unwrap_err();
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn a_command_with_nothing_to_report_is_not_a_failure() {
        assert!(parse(r#"{"id":"x"}"#).is_ok());
    }

    #[test]
    fn a_pane_id_names_the_workspace_to_remove() {
        assert_eq!("wN:p1".split(':').next(), Some("wN"));
    }

    #[test]
    fn agent_states_map_to_what_they_mean_for_dispatch() {
        assert!(AgentStatus::parse("idle").is_free());
        assert!(AgentStatus::parse("done").is_free());
        assert!(!AgentStatus::parse("working").is_free());
        assert!(
            !AgentStatus::parse("blocked").is_free(),
            "an agent on a permission prompt would read the task as the answer"
        );
        assert!(
            !AgentStatus::parse("unknown").is_free(),
            "not knowing is not permission"
        );
    }

    #[test]
    fn an_unrecognised_state_does_not_panic() {
        assert_eq!(AgentStatus::parse("something-new"), AgentStatus::Unknown);
    }
}
