//! Writing a review: notes against lines, kept until they are sent as one.
//!
//! GitHub takes a review whole — a verdict, a body, and the line comments that
//! go with it — so a note written here is a draft until then, kept per pull
//! request and drawn in the diff where its thread will be, marked as not yet
//! said. One per line: a second `i` on the same line edits the first, which is
//! what a reader reaching for a line they have already written on wants.
//!
//! Drafts are made only against every change the pull request makes, never a
//! single commit: a review comment is anchored to the pull request's last
//! version, and a line number from an earlier commit would land somewhere
//! else once sent.

use super::App;
use crate::data::{DiffKind, DiffSide, Draft, Kind, Thread, ThreadComment, Verdict};
use crate::service::Request;
use crate::shared::key::{Key, Press};

/// Where a note is going: a line of one side of one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub path: String,
    pub line: u32,
    pub side: DiffSide,
}

/// What a draft's head row says. The view styles a head that starts so.
pub const DRAFT: &str = "✎ draft";

impl App {
    /// The drafts of the pull request on screen.
    pub fn drafts(&self) -> &[Draft] {
        self.drafts
            .get(&self.draft_key())
            .map_or(&[], Vec::as_slice)
    }

    fn draft_key(&self) -> (String, i64) {
        (self.item_repo_key(), self.current().map_or(0, |c| c.num))
    }

    /// The drafts on `path`, shaped as threads so they are laid into the diff
    /// by the same code, in the same place, as what they will become.
    pub fn drafts_on(&self, path: &str) -> Vec<Thread> {
        if self.diff_commit.is_some() {
            return Vec::new();
        }
        self.drafts()
            .iter()
            .filter(|d| d.path == path)
            .map(|d| Thread {
                path: d.path.clone(),
                line: Some(d.line),
                side: d.side,
                resolved: false,
                pending: true,
                comments: vec![ThreadComment {
                    author: DRAFT.into(),
                    body: d.body.clone(),
                    when: String::new(),
                }],
            })
            .collect()
    }

    /// The line of code the cursor is on, or the one a comment under the
    /// cursor belongs to. `None` on a hunk header, or above the first line.
    pub(super) fn cursor_anchor(&mut self) -> Option<Anchor> {
        let path = self.diff_file()?.path.clone();
        let row = self.cursor_row();
        let prep = self.prepared();
        let r = prep
            .rows
            .get(..=row)?
            .iter()
            .rev()
            .find(|r| !r.kind.is_note())?;
        let (side, num) = match r.kind {
            DiffKind::Add | DiffKind::Ctx => (DiffSide::Right, &r.ln),
            DiffKind::Del => (DiffSide::Left, &r.lo),
            _ => return None,
        };
        Some(Anchor {
            path,
            line: num.parse().ok()?,
            side,
        })
    }

    fn draft_at(&self, a: &Anchor) -> Option<usize> {
        self.drafts()
            .iter()
            .position(|d| d.path == a.path && d.line == a.line && d.side == a.side)
    }

    /// `i`: a note on the cursor's line — a new one, or the one already there.
    pub(super) fn open_note(&mut self) {
        if self.diff_commit.is_some() {
            self.flash_warn("notes go on the whole pull request: v, then All changes");
            return;
        }
        let Some(a) = self.cursor_anchor() else {
            self.flash_warn("a note goes on a line of code");
            return;
        };
        self.note_text = self
            .draft_at(&a)
            .and_then(|i| self.drafts().get(i))
            .map(|d| d.body.clone())
            .unwrap_or_default();
        self.note_open = Some(a);
    }

    /// The note editor owns every key while it is up: the letters are the note.
    pub(super) fn note_key(&mut self, press: Press) {
        match press.key {
            Key::Esc => {
                self.note_open = None;
                self.note_text.clear();
            }
            // ^j is a newline: enter is taken by saving, and a terminal sends
            // ^j as the line feed it is
            Key::Char('j') if press.ctrl => self.note_text.push('\n'),
            Key::Enter => self.save_note(),
            Key::Backspace => {
                self.note_text.pop();
            }
            Key::Char(c) if !press.ctrl => self.note_text.push(c),
            _ => {}
        }
    }

    /// Keeps the note, or — written down to nothing — lets it go.
    fn save_note(&mut self) {
        let Some(a) = self.note_open.take() else {
            return;
        };
        let body = std::mem::take(&mut self.note_text).trim().to_string();
        let at = self.draft_at(&a);
        let key = self.draft_key();
        let list = self.drafts.entry(key).or_default();
        match (at, body.is_empty()) {
            (Some(i), true) => {
                list.remove(i);
            }
            (Some(i), false) => {
                if let Some(d) = list.get_mut(i) {
                    d.body = body;
                }
            }
            (None, true) => {}
            (None, false) => list.push(Draft {
                path: a.path,
                line: a.line,
                side: a.side,
                body,
            }),
        }
    }

    /// `X`: drops the draft on the cursor's line.
    pub(super) fn delete_note(&mut self) {
        let Some(a) = self.cursor_anchor() else {
            return;
        };
        let Some(i) = self.draft_at(&a) else {
            self.flash_ok("no draft on this line");
            return;
        };
        let key = self.draft_key();
        if let Some(list) = self.drafts.get_mut(&key) {
            list.remove(i);
        }
        self.flash_ok("draft dropped");
    }

    /// `R`: the dialog that sends the review — every draft, a verdict, and
    /// what to say of the pull request as a whole.
    pub(super) fn open_review(&mut self) {
        if self.current().map(crate::data::Item::kind) != Some(Kind::Pr) {
            return;
        }
        self.review_open = true;
        self.review_verdict = Verdict::Comment;
        self.review_text.clear();
    }

    /// The dialog owns every key: the letters are the review's body, so the
    /// verdict moves on tab and the arrows, as the agent picker's rows do.
    pub(super) fn review_key(&mut self, press: Press) {
        let at = Verdict::ALL
            .iter()
            .position(|v| *v == self.review_verdict)
            .unwrap_or(0);
        let n = Verdict::ALL.len();
        match press.key {
            Key::Esc => self.review_open = false,
            Key::Tab | Key::Down => self.review_verdict = Verdict::ALL[(at + 1) % n],
            Key::BackTab | Key::Up => self.review_verdict = Verdict::ALL[(at + n - 1) % n],
            Key::Char('j') if press.ctrl => self.review_text.push('\n'),
            Key::Enter => self.send_review(),
            Key::Backspace => {
                self.review_text.pop();
            }
            Key::Char(c) if !press.ctrl => self.review_text.push(c),
            _ => {}
        }
    }

    /// Why this review cannot be sent as it stands, in GitHub's own terms —
    /// said here rather than after a round trip that ends in a 422.
    pub fn review_refusal(&self) -> Option<&'static str> {
        let body = self.review_text.trim();
        match self.review_verdict {
            Verdict::RequestChanges if body.is_empty() => {
                Some("requesting changes needs a word on what to change")
            }
            Verdict::Comment if body.is_empty() && self.drafts().is_empty() => {
                Some("a comment needs a note or something to say")
            }
            _ => None,
        }
    }

    /// Sends it. The drafts stay until GitHub says it has them: a review that
    /// failed to send must not take the notes it carried with it.
    fn send_review(&mut self) {
        if let Some(why) = self.review_refusal() {
            self.flash_warn(why);
            return;
        }
        let (repo, num) = self.draft_key();
        let request = Request::SubmitReview {
            repo,
            num,
            verdict: self.review_verdict,
            body: self.review_text.trim().to_string(),
            drafts: self.drafts().to_vec(),
        };
        self.review_open = false;
        // busy first: a worker that is gone clears it again on the way out
        self.busy = true;
        self.flash_ok("sending the review…");
        self.ask(request);
    }
}
