//! The worker thread.
//!
//! `git diff` on a large repository takes long enough to drop a frame, and
//! `herdr agent list` spawns a process; neither belongs between two renders.
//! The same shape the GitHub browser uses next door: requests in, responses
//! out, and the interface polls.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use super::git;
use super::model::{ChangedFile, Row, Scope};
use crate::data::Agent;
use crate::error::{Error, Result as Res};

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

    /// Hands a request to the worker, reporting whether it got there.
    ///
    /// The answer matters: a request dropped because the thread is gone would
    /// otherwise leave whatever asked for it marked `Loading` forever, with a
    /// skeleton animating over data that is never coming.
    pub fn send(&self, req: Request) -> bool {
        self.tx.send(req).is_ok()
    }

    pub fn poll(&self) -> Option<Response> {
        self.rx.try_recv().ok()
    }
}

fn handle(req: Request) -> Response {
    match req {
        Request::Files { repo, scope } => Response::Files {
            result: git::changed_files(&repo, &scope),
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
            let result = git::file_diff(&repo, &scope, &path, context).map(|rows| {
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
            result: git::blame(&repo, &path),
            path,
        },

        Request::Agents => Response::Agents(crate::herdr::agents()),

        Request::Send { pane, text } => Response::Sent(crate::herdr::prompt(&pane, &text)),
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
