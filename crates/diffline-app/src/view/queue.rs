//! The review queue: what has been written, and where it is going.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::app::{App, Pane};
use crate::hit::{Region, Target};
use crate::model::State;
use crate::tui::theme;
use crate::tui::{Section, fill, hline, put, put_right, put_trunc, scroll_into_view};

pub(super) fn queue(buf: &mut Buffer, area: Rect, app: &mut App) {
    Section::new("REVIEW QUEUE")
        .count(app.comments.len())
        .focused(app.pane == Pane::Queue)
        .open(buf, area);

    // The footer names the target and what sending would do.
    let foot_y = area.bottom() - 2;
    hline(buf, area.x, foot_y - 1, area.width, theme::border_soft());
    let fs = Style::default().bg(theme::panel_alt());
    // A pending new agent wins over the running one: it is what sending would
    // actually reach, and a footer naming the other would be a lie at exactly
    // the moment it is being read.
    let (who, dot) = match (&app.new_kind, app.agent()) {
        (Some(kind), _) => (format!("a new {kind} · here"), theme::green()),
        (None, Some(a)) => (
            format!("{} · {}", a.kind, a.where_short()),
            match a.status {
                crate::shared::mux::AgentStatus::Working => theme::yellow(),
                crate::shared::mux::AgentStatus::Blocked => theme::red(),
                crate::shared::mux::AgentStatus::Idle | crate::shared::mux::AgentStatus::Done => {
                    theme::green()
                }
                crate::shared::mux::AgentStatus::Unknown => theme::dimmer(),
            },
        ),
        (None, None) => ("no agent — press a".into(), theme::dimmer()),
    };
    put(buf, area.x + 1, foot_y, area.right(), "●", fs.fg(dot));
    put_trunc(
        buf,
        area.x + 3,
        foot_y,
        area.right() - 6,
        &who,
        fs.fg(theme::fg()),
    );
    put_right(buf, area.right() - 1, foot_y, "a", fs.fg(theme::dimmer()));

    let send = format!(" ⏎ S · send {} ", app.comments.len());
    let ready = !app.comments.is_empty();
    put(
        buf,
        area.x + 1,
        area.bottom() - 1,
        area.right(),
        &send,
        Style::default()
            .bg(if ready {
                theme::yellow()
            } else {
                theme::panel()
            })
            .fg(if ready {
                theme::panel()
            } else {
                theme::dimmer()
            }),
    );

    let list = Rect {
        y: area.y + 2,
        height: foot_y.saturating_sub(area.y + 3),
        ..area
    };

    if app.comments.is_empty() && app.replies.is_empty() {
        for (n, line) in [
            "No comments yet.",
            "",
            "Move to a line and press c.",
            "V first to take a range.",
        ]
        .iter()
        .enumerate()
        {
            put_trunc(
                buf,
                list.x + 2,
                list.y + n as u16,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::dimmer()),
            );
        }
        return;
    }

    // Cards are three rows and a gap. The window scrolls to keep the
    // selected one inside it, the way every other list here does — a queue
    // deeper than the pane used to walk its selection straight off screen.
    let cards = (list.height as usize / 4).max(1);
    scroll_into_view(
        &mut app.queue_scroll,
        app.queue_sel,
        cards,
        app.comments.len(),
    );
    app.hits.push(Region::rows(
        Target::Pane(Pane::Queue),
        list,
        4,
        app.queue_scroll,
        app.comments.len(),
    ));

    let focused = app.pane == Pane::Queue;
    let mut y = list.y;
    for (i, c) in app.comments.iter().enumerate().skip(app.queue_scroll) {
        if y + 2 >= list.bottom() {
            break;
        }
        let sel = focused && i == app.queue_sel;
        let border = match c.state {
            State::Sending => theme::green(),
            State::Queued if sel => theme::yellow(),
            State::Queued => theme::border(),
        };
        let base = Style::default().bg(theme::bg());
        fill(
            buf,
            Rect {
                x: list.x,
                y,
                width: list.width,
                height: 3,
            },
            theme::bg(),
        );
        put(buf, list.x, y, area.right(), "▌", base.fg(border));

        put(
            buf,
            list.x + 2,
            y,
            area.right(),
            &format!("#{}", i + 1),
            base.fg(theme::yellow()),
        );
        let state = match c.state {
            State::Queued => "queued",
            State::Sending => "sending →",
        };
        let sx = put_right(buf, area.right() - 1, y, state, base.fg(border));
        put_trunc(
            buf,
            list.x + 6,
            y,
            sx.saturating_sub(1),
            &c.where_label(),
            base.fg(theme::dim()),
        );

        put_trunc(
            buf,
            list.x + 2,
            y + 1,
            area.right() - 1,
            &c.snippet,
            base.fg(theme::dimmer()),
        );
        put_trunc(
            buf,
            list.x + 2,
            y + 2,
            area.right() - 1,
            &c.body,
            base.fg(theme::fg()),
        );
        y += 4;
    }

    for reply in &app.replies {
        for line in reply.lines() {
            if y >= list.bottom() {
                return;
            }
            put_trunc(
                buf,
                list.x + 2,
                y,
                area.right() - 1,
                line,
                Style::default().bg(theme::panel_alt()).fg(theme::green()),
            );
            y += 1;
        }
        y += 1;
    }
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
    use crate::model::{ChangedFile, Comment, Scope, Status};
    use crate::tui::probe;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app_with_notes(n: usize) -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
            None,
        );
        a.files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: Status::Modified,
            add: 1,
            del: 0,
        }];
        a.files_state = crate::app::Load::Ready;
        a.queue_shown = true;
        for i in 0..n {
            a.comments.push(Comment {
                anchors: Vec::new(),
                file: "src/a.rs".into(),
                snippet: "…".into(),
                body: format!("note {i}"),
                state: State::Queued,
            });
        }
        a
    }

    #[test]
    fn the_queue_scrolls_to_keep_the_selected_note_on_screen() {
        // A queue deeper than the pane used to walk its selection straight
        // off the bottom: the cards never scrolled, only the selection moved.
        let mut a = app_with_notes(12);
        a.queue_sel = 11;
        let mut term = Terminal::new(TestBackend::new(160, 30)).unwrap();
        term.draw(|f| crate::view::draw(f, &mut a)).unwrap();

        assert!(a.queue_scroll > 0, "the window followed the selection down");
        let screen = probe::screen(&term);
        assert!(
            screen.contains("#12"),
            "the selected card is drawn:\n{screen}"
        );
        assert!(
            !screen.contains("#2 "),
            "and the early cards made room:\n{screen}"
        );
    }

    #[test]
    fn a_frame_records_which_card_sits_on_which_rows() {
        // Without this a click on the queue could say only "the queue",
        // which is a selection nothing can act on.
        let mut a = app_with_notes(3);
        let mut term = Terminal::new(TestBackend::new(160, 30)).unwrap();
        term.draw(|f| crate::view::draw(f, &mut a)).unwrap();

        let region = a
            .hits
            .iter()
            .rev()
            .find(|r| r.target == crate::hit::Target::Pane(Pane::Queue) && r.len > 0)
            .copied()
            .expect("the cards are a region");
        assert_eq!(region.len, 3);
        assert_eq!(region.row_h, 4, "three rows of card and one of gap");
        // The gap row under a card still reads as that card — a click in the
        // seam should not fall through to nothing.
        assert_eq!(region.index_at(region.area.y + 3), Some(0));
        assert_eq!(region.index_at(region.area.y + 4), Some(1));
    }
}
