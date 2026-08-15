//! The worker thread.
//!
//! `git diff` on a large repository takes long enough to drop a frame, and
//! `herdr agent list` spawns a process; neither belongs between two renders.
//! The same shape the GitHub browser uses next door: requests in, responses
//! out, and the interface polls.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use super::model::{ChangedFile, Row, Scope};
use crate::error::{Error, Result as Res};
use crate::mux::Agent;

pub enum Request {
    /// Which files a scope touches.
    Files { repo: String, scope: Scope },
    /// One file's diff, at a context width.
    Diff {
        repo: String,
        scope: Scope,
        path: String,
        context: u32,
    },
    /// Who last touched each line.
    Blame { repo: String, path: String },
    /// The coding agents on this machine.
    Agents,
    /// Hand the queue to one of them.
    Send { pane: String, text: String },
    /// Write a file: a config, a theme template, a keymap template.
    ///
    /// Small and local, but the render loop is the render loop — the only
    /// reason to have this thread is that nothing which touches a disk or a
    /// process should happen between a keystroke and the frame it draws.
    Write(Write),
    /// Start one that is not running yet, in this repository, and hand it the
    /// queue. Everything the reader wants to say is already written by the
    /// time they decide who should hear it.
    Spawn {
        repo: String,
        label: String,
        kind: String,
        text: String,
    },
}

pub enum Response {
    Files {
        scope: Scope,
        result: Res<Vec<ChangedFile>>,
    },
    Diff {
        path: String,
        context: u32,
        /// The rows, and the colour spans that go with them.
        result: Res<(Vec<Row>, Vec<Vec<crate::syntax::Span>>)>,
    },
    Blame {
        path: String,
        result: Res<Vec<String>>,
    },
    Agents(Res<Vec<Agent>>),
    Sent(Res<()>),
    /// What the write did, in words, for the status bar.
    Wrote(std::io::Result<String>),
}

/// The writes the interface can ask for.
#[derive(Debug)]
pub enum Write {
    Theme(crate::theme::Theme),
    ThemeTemplate(String),
    KeymapTemplate,
}

/// The worker thread is not coming back.
#[derive(Clone, Copy, Debug)]
pub struct Gone;

pub struct Service {
    tx: Sender<Request>,
    rx: Receiver<Response>,
}

impl Service {
    pub fn spawn() -> Self {
        let (tx, req_rx) = channel::<Request>();
        let (res_tx, rx) = channel::<Response>();

        thread::spawn(move || {
            while let Ok(req) = req_rx.recv() {
                if res_tx.send(handle(req)).is_err() {
                    break; // the interface is gone
                }
            }
        });
        Self { tx, rx }
    }

    /// Hands a request to the worker, reporting whether it got there.
    ///
    /// The answer matters: a request dropped because the thread is gone would
    /// otherwise leave whatever asked for it marked `Loading` forever, with a
    /// skeleton animating over data that is never coming.
    pub fn send(&self, req: Request) -> bool {
        self.tx.send(req).is_ok()
    }

    /// The next answer, if one has arrived.
    ///
    /// `Disconnected` is not the same as `Empty` and must not be flattened
    /// into it: the send side already refuses to leave a request in the air,
    /// but a worker that dies *after* taking one would otherwise leave that
    /// one loading for ever — `poll` returning `None` looks exactly like an
    /// answer that has not come yet.
    /// A service whose worker is already gone, for testing what happens then.
    #[cfg(test)]
    pub fn dead() -> Self {
        let (tx, _) = channel::<Request>();
        let (_, rx) = channel::<Response>();
        Self { tx, rx }
    }

    pub fn poll(&self) -> Result<Option<Response>, Gone> {
        match self.rx.try_recv() {
            Ok(r) => Ok(Some(r)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(Gone),
        }
    }
}

fn handle(req: Request) -> Response {
    // Chosen per request rather than held: the repository is in the request,
    // and which backend owns it is a fact about that repository.
    let vcs = |repo: &str| super::vcs::of(repo).unwrap_or(&super::git::Git);

    match req {
        Request::Files { repo, scope } => Response::Files {
            result: vcs(&repo).changed_files(&repo, &scope),
            scope,
        },

        Request::Diff {
            repo,
            scope,
            path,
            context,
        } => {
            // Lexed here rather than when the answer lands: a large file takes
            // long enough to be a dropped frame, and this thread exists so the
            // interface never has to wait.
            let result = vcs(&repo)
                .file_diff(&repo, &scope, &path, context)
                .map(|rows| {
                    let spans = highlight_rows(&path, &rows);
                    (rows, spans)
                });
            Response::Diff {
                path,
                context,
                result,
            }
        }

        Request::Blame { repo, path } => Response::Blame {
            // Only asked of a backend that says it can: an empty list from
            // one that cannot would read as "nobody wrote this".
            result: if vcs(&repo).has_blame() {
                vcs(&repo).blame(&repo, &path)
            } else {
                Ok(Vec::new())
            },
            path,
        },

        Request::Agents => Response::Agents(crate::mux::current().agents()),

        Request::Send { pane, text } => Response::Sent(crate::mux::current().prompt(&pane, &text)),

        Request::Write(what) => Response::Wrote(match what {
            Write::Theme(t) => {
                crate::config::save_theme(t).map(|()| format!("theme → {}", t.name()))
            }
            Write::ThemeTemplate(name) => {
                crate::theme::write_template(&name).map(|p| format!("wrote {}", p.display()))
            }
            Write::KeymapTemplate => {
                super::keys::write_template().map(|p| format!("wrote {}", p.display()))
            }
        }),

        // `None` for the branch: the review is of what is in this checkout, so
        // a fresh worktree on a new branch would open the agent on a tree that
        // does not contain what the comments are about.
        Request::Spawn {
            repo,
            label,
            kind,
            text,
        } => Response::Sent(crate::mux::current().dispatch(&repo, None, &label, &kind, &text)),
    }
}

/// Colours a diff's rows, one span list per row.
///
/// The lexer is written for whole files, and a diff is not one: a hunk starts
/// mid-file, so a block comment opened above the first visible line is
/// invisible to it. Feeding it the rows in order is the closest thing
/// available, and it is right for everything a hunk contains rather than
/// inherits — which is what the colour is for.
fn highlight_rows(path: &str, rows: &[Row]) -> Vec<Vec<crate::syntax::Span>> {
    let Some(lang) = crate::syntax::of_path(path) else {
        return Vec::new();
    };
    let joined: String = rows
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut spans = crate::syntax::highlight(lang, &joined);
    // `lines()` drops a trailing empty line that `join` did not create, so the
    // two can differ by one; the pane indexes by row and must not go short.
    spans.resize(rows.len(), Vec::new());
    spans
}

/// Reports a failure the way the status bar wants to read it.
pub fn brief(e: &Error) -> String {
    e.brief()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diffline::model::Kind;

    fn row(text: &str) -> Row {
        Row {
            kind: Kind::Context,
            old: Some(1),
            new: Some(1),
            text: text.into(),
        }
    }

    #[test]
    fn a_span_list_is_produced_per_row() {
        // the pane indexes spans by row; a short list would panic or mis-colour
        let rows = vec![row("let x = 1;"), row("// note"), row("")];
        let spans = highlight_rows("a.rs", &rows);
        assert_eq!(spans.len(), rows.len());
    }

    #[test]
    fn a_language_nobody_knows_is_simply_uncoloured() {
        assert!(highlight_rows("notes.xyz", &[row("anything")]).is_empty());
    }

    #[test]
    fn the_rows_are_coloured_as_the_code_they_are() {
        let rows = vec![row("const X: i32 = 42;")];
        let spans = highlight_rows("a.rs", &rows);
        assert!(
            spans[0]
                .iter()
                .any(|s| s.kind == crate::syntax::Kind::Number),
            "42 should be a number"
        );
    }

    #[test]
    fn no_rows_is_no_spans_rather_than_a_panic() {
        assert!(highlight_rows("a.rs", &[]).is_empty());
    }
}
