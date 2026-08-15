//! The worker thread.
//!
//! `git diff` on a large repository takes long enough to drop a frame, and
//! `herdr agent list` spawns a process; neither belongs between two renders.
//! The same shape the GitHub browser uses next door: requests in, responses
//! out, and the interface polls.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use crate::shared::worker::{Gone, Worker};
use std::thread;

use crate::diffline::model::{ChangedFile, Row, Scope};
use crate::shared::error::{Error, Result as Res};
use crate::shared::mux::Agent;

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
        result: Res<(Vec<Row>, Vec<Vec<crate::shared::syntax::Span>>)>,
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
    /// Remember which theme was chosen.
    Theme(crate::tui::theme::Theme),
    /// Put `text` at `path`, making the directory if it is not there.
    ///
    /// Text and path rather than a name of something to generate: what goes
    /// in a keymap template is a fact about the keymap, and the keymap is
    /// state. This layer knows how to write a file and nothing about what is
    /// worth writing.
    File {
        path: std::path::PathBuf,
        text: String,
    },
}

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

impl Worker<Request, Response> for Service {
    fn send(&self, req: Request) -> bool {
        self.tx.send(req).is_ok()
    }

    /// `Disconnected` is not the same as `Empty` and must not be flattened
    /// into it: the send side already refuses to leave a request in the air,
    /// but a worker that dies *after* taking one would otherwise leave that
    /// one loading for ever — a `None` looks exactly like an answer that has
    /// not come yet.
    fn poll(&self) -> Result<Option<Response>, Gone> {
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
    let vcs = |repo: &str| crate::diffline::vcs::of(repo).unwrap_or(&crate::diffline::git::Git);

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

        Request::Agents => Response::Agents(crate::shared::mux::current().agents()),

        Request::Send { pane, text } => {
            Response::Sent(crate::shared::mux::current().prompt(&pane, &text))
        }

        Request::Write(what) => Response::Wrote(match what {
            Write::Theme(t) => {
                crate::shared::config::save_theme(t).map(|()| format!("theme → {}", t.name()))
            }
            Write::File { path, text } => write_file(&path, &text),
        }),

        // `None` for the branch: the review is of what is in this checkout, so
        // a fresh worktree on a new branch would open the agent on a tree that
        // does not contain what the comments are about.
        Request::Spawn {
            repo,
            label,
            kind,
            text,
        } => Response::Sent(
            crate::shared::mux::current().dispatch(&repo, None, &label, &kind, &text),
        ),
    }
}

/// Colours a diff's rows, one span list per row.
///
/// The lexer is written for whole files, and a diff is not one: a hunk starts
/// mid-file, so a block comment opened above the first visible line is
/// invisible to it. Feeding it the rows in order is the closest thing
/// available, and it is right for everything a hunk contains rather than
/// inherits — which is what the colour is for.
fn highlight_rows(path: &str, rows: &[Row]) -> Vec<Vec<crate::shared::syntax::Span>> {
    let Some(lang) = crate::shared::syntax::of_path(path) else {
        return Vec::new();
    };
    let joined: String = rows
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut spans = crate::shared::syntax::highlight(lang, &joined);
    // `lines()` drops a trailing empty line that `join` did not create, so the
    // two can differ by one; the pane indexes by row and must not go short.
    spans.resize(rows.len(), Vec::new());
    spans
}

/// Reports a failure the way the status bar wants to read it.
pub fn brief(e: &Error) -> String {
    e.brief()
}

/// The next answer, if one has arrived.
///
/// `Disconnected` is not the same as `Empty` and must not be flattened
/// into it: the send side already refuses to leave a request in the air,
/// but a worker that dies *after* taking one would otherwise leave that
/// one loading for ever — `poll` returning `None` looks exactly like an
/// answer that has not come yet.
/// A service whose worker is already gone, for testing what happens then.
/// Puts `text` at `path`, making the directory on the way if it is not there.
fn write_file(path: &std::path::Path, text: &str) -> std::io::Result<String> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::File::create(path)?.write_all(text.as_bytes())?;
    Ok(format!("wrote {}", path.display()))
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
                .any(|s| s.kind == crate::shared::syntax::Kind::Number),
            "42 should be a number"
        );
    }

    #[test]
    fn no_rows_is_no_spans_rather_than_a_panic() {
        assert!(highlight_rows("a.rs", &[]).is_empty());
    }
}
