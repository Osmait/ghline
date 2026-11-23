//! Comment drafting, queue rendering, and sending notes to agents.

use crate::app::{App, Modal, Pane};
use crate::keys::Action;
use crate::model::{Comment, State};
use crate::service::{Request, Write};

impl App {
    // --- comments ---

    /// `␣n`, or `enter` on a line: the note editor over whatever is
    /// selected. In the tree this is a note about the file itself; in the diff
    /// it is a note about the selected lines.
    pub fn open_comment(&mut self) {
        if self.pane == Pane::Tree {
            if self.path().is_empty() {
                self.flash("no file to ask about");
                return;
            }
            self.about_file = true;
        } else {
            if self.selected_anchors().is_empty() {
                self.flash("move to a code line first");
                return;
            }
            self.about_file = false;
        }
        self.modal = Some(Modal::Comment);
        self.draft.clear();
    }

    /// Saves the draft against the selection.
    pub fn save_comment(&mut self) {
        let body = self.draft.trim().to_string();
        let about_file = std::mem::take(&mut self.about_file);
        let anchors = if about_file {
            Vec::new()
        } else {
            self.selected_anchors()
        };
        let file = self.path().to_string();
        self.modal = None;
        self.draft.clear();

        if body.is_empty() || file.is_empty() || (!about_file && anchors.is_empty()) {
            return;
        }
        let n = anchors.len();
        let snippet = if about_file {
            // What the file is, rather than a line of it: enough for the
            // agent to know which one is meant without opening it.
            let f = self.file();
            f.map(|f| format!("{} · +{} −{}", f.status.label(), f.add, f.del))
                .unwrap_or_default()
        } else {
            self.diff_rows()
                .get(self.span().0)
                .map(|r| r.text.trim().chars().take(60).collect::<String>())
                .unwrap_or_default()
        };

        // A comment is an instruction handed to an agent, and an anchor
        // naming the wrong file is an instruction to change the wrong code.
        //
        // This cannot fire today: the anchors and the file are both taken
        // from `self.path()` a line apart, so they agree by construction.
        // What it guards is a later change to that construction — and a
        // later change is made in development, where `debug_assert` fires
        // and where the tests over this function run. Making it a hard
        // assertion would only add the case where such a change reaches
        // release untested by anything, in exchange for the risk of throwing
        // somebody out of the alternate screen with an unsent queue in it.
        debug_assert!(
            anchors.iter().all(|a| a.path == file && a.line > 0),
            "a note anchored somewhere other than the file it is about"
        );
        self.comments.push(Comment {
            anchors,
            file,
            snippet,
            body,
            state: State::Queued,
        });
        self.anchor = None;
        self.flash(if about_file {
            "queued · about the whole file".to_string()
        } else {
            format!("comment queued · {n} line{}", if n == 1 { "" } else { "s" })
        });
    }

    /// `␣x`: drops whatever comment covers the cursor.
    pub fn delete_comment(&mut self) {
        let path = self.path().to_string();
        let Some(a) = self.row().and_then(|r| r.anchor(&path)) else {
            return;
        };
        let before = self.comments.len();
        self.comments
            .retain(|c| c.state == State::Sending || !c.anchors.contains(&a));
        if self.comments.len() < before {
            self.queue_sel = self.queue_sel.min(self.comments.len().saturating_sub(1));
            self.flash("comment removed");
        }
    }

    /// `␣s`: hands the queue to the chosen agent.
    pub fn send_queue(&mut self) {
        if self.comments.is_empty() {
            self.flash("queue is empty");
            return;
        }
        // Nothing may be sent while a send is in flight: the second one would
        // arrive at an agent already reading the first, and the comments
        // would be marked sent by whichever answered last.
        if self.busy {
            self.flash("still sending");
            return;
        }
        let text = self.render_queue();

        if let Some(kind) = self.new_kind.clone() {
            for c in &mut self.comments {
                c.state = State::Sending;
            }
            self.busy = true;
            self.sending_to = Some(kind.clone());
            self.flash(format!("starting a {kind}…"));
            self.ask_spawn(kind, text);
            return;
        }

        let Some(agent) = self.agent().cloned() else {
            let pick = self
                .keys
                .key_label(Action::Agents)
                .unwrap_or_else(|| ":".into());
            self.flash(format!("no agent to send to — {pick} picks one"));
            return;
        };
        // Asked of the multiplexer, not of the status: a backend that cannot
        // see what its agents are doing must not have every send refused on
        // its behalf.
        if let Some(why) = self.refusal(&agent) {
            // Say what to do, not only what went wrong: with every agent busy
            // the way out is to start one, and that is two keys away.
            self.flash(format!("{}: {why} · ␣a to pick another", agent.kind));
            return;
        }

        for c in &mut self.comments {
            c.state = State::Sending;
        }
        self.busy = true;
        self.sending_to = Some(agent.kind.clone());
        self.flash(format!("sending to {}…", agent.kind));
        self.ask_send(agent.pane, text);
    }

    /// Hands a write to the worker. Nothing in here touches a disk itself.
    pub(super) fn ask_write(&mut self, what: Write) {
        if !self
            .service
            .as_ref()
            .is_some_and(|s| s.send(Request::Write(what)))
        {
            self.flash("the worker is gone — nothing was written");
        }
    }

    /// Starts an agent for this repository and hands it the queue.
    ///
    /// The label names where it came from, so the workspace is recognisable in
    /// herdr next to whatever else is running.
    fn ask_spawn(&mut self, kind: String, text: String) -> bool {
        let label = format!("review {}", self.path());
        let sent = self.service.as_ref().is_some_and(|s| {
            s.send(Request::Spawn {
                repo: self.repo.clone(),
                label,
                kind,
                text,
            })
        });
        if !sent {
            self.busy = false;
            for c in &mut self.comments {
                c.state = State::Queued;
            }
            self.flash("the worker is gone — nothing was sent");
        }
        sent
    }

    fn ask_send(&mut self, pane: String, text: String) -> bool {
        let sent = self
            .service
            .as_ref()
            .is_some_and(|s| s.send(Request::Send { pane, text }));
        if !sent {
            self.busy = false;
            for c in &mut self.comments {
                c.state = State::Queued;
            }
            self.flash("the worker thread is gone — restart diffline");
        }
        sent
    }

    /// The whole queue as one message.
    ///
    /// One message rather than one per comment: an agent handed twelve
    /// separate prompts answers twelve times and sees no shape. Grouped by
    /// file, in line order, because that is the order it will work in.
    pub fn render_queue(&self) -> String {
        let mut out = String::from("Review notes on the current diff.\n");
        out.push_str(&format!("Repository: {}\n", self.repo));
        out.push_str(&format!("Scope: {}\n\n", self.scope));

        let mut paths: Vec<&str> = self.comments.iter().map(Comment::path).collect();
        paths.sort_unstable();
        paths.dedup();

        for path in paths {
            out.push_str(&format!("--- {path}\n"));
            let mut here: Vec<&Comment> =
                self.comments.iter().filter(|c| c.path() == path).collect();
            here.sort_by_key(|c| c.anchors.iter().map(|a| a.line).min().unwrap_or(0));
            for c in here {
                out.push_str(&format!("\n  {}\n", c.where_label()));
                if !c.snippet.is_empty() {
                    out.push_str(&format!("    | {}\n", c.snippet));
                }
                out.push_str(&format!("    {}\n", c.body));
            }
            out.push('\n');
        }
        out
    }
}
