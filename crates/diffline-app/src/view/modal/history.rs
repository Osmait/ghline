//! The commit history, as a picker.
//!
//! A picker rather than a pager: the list exists to reach a commit and
//! review it, and the review itself happens in the panes this floats over.
//! Type to filter — the subject, the author and the short sha are all
//! searched, because all three are how a commit is remembered.

use ratatui::layout::{Rect, Size};

use ratatui::buffer::Buffer;

use crate::app::App;
use crate::hit::{Region, Target};
use crate::model::Scope;
use crate::tui::theme;
use crate::tui::{Dialog, put, put_right, put_trunc, scroll_into_view};

pub(crate) fn history(buf: &mut Buffer, area: Rect, app: &mut App) {
    let hits = app.history_hits();
    let body = Dialog::new("HISTORY")
        .hint("⏎ review · esc")
        .accent(theme::orange())
        // A fixed height rather than one grown from the answer, so the box
        // does not jump the moment the log lands.
        .size(Size::new(
            area.width.saturating_sub(8).min(100),
            area.height * 3 / 4,
        ))
        .over_content()
        .open(buf, area);

    app.hits.push(Region::plain(Target::Modal, body.outer));
    let list = body.query(buf, &app.query, "❯ ", "filter commits…", app.blink);
    put_right(
        buf,
        list.outer.right() - 2,
        list.outer.y + 1,
        &format!("{} commits", hits.len()),
        ratatui::style::Style::default()
            .bg(theme::panel())
            .fg(theme::dimmer()),
    );

    if hits.is_empty() {
        let (said, color) = if app.log_state.is_loading() {
            ("reading history…".to_string(), theme::dimmer())
        } else if let Some(e) = app.log_state.error() {
            (e, theme::red())
        } else if app.log.is_empty() {
            ("no commits yet".into(), theme::dimmer())
        } else {
            (format!("no commit matches {}", app.query), theme::dimmer())
        };
        put_trunc(
            buf,
            list.inner.x + 2,
            list.inner.y + 1,
            list.inner.right() - 1,
            &said,
            ratatui::style::Style::default()
                .bg(theme::panel())
                .fg(color),
        );
        return;
    }

    let mut scroll = 0;
    scroll_into_view(&mut scroll, app.sel, list.inner.height as usize, hits.len());
    app.hits.push(Region::rows(
        Target::Modal,
        list.inner,
        1,
        scroll,
        hits.len(),
    ));

    for slot in list.rows(buf, hits.len(), 1, app.sel, scroll) {
        let Some(c) = hits.get(slot.index).and_then(|i| app.log.get(*i)) else {
            continue;
        };
        let s = slot.style;
        let y = slot.area.y;

        // The commit already under review carries a dot, so the picker also
        // answers "where am I" — the same mark the agent list uses.
        if matches!(&app.scope, Scope::Commit { sha } if *sha == c.sha) {
            put(
                buf,
                slot.area.x + 1,
                y,
                slot.area.right(),
                "●",
                s.fg(theme::yellow()),
            );
        }
        put(
            buf,
            slot.area.x + 3,
            y,
            slot.area.right(),
            c.short(),
            s.fg(theme::yellow()),
        );
        let when = format!("{} · {}", c.author, crate::shared::ago::since(c.when));
        let rx = put_right(buf, slot.area.right() - 1, y, &when, s.fg(theme::dimmer()));
        put_trunc(
            buf,
            slot.area.x + 12,
            y,
            rx.saturating_sub(1),
            &c.subject,
            s.fg(if slot.selected {
                theme::bright()
            } else {
                theme::fg()
            }),
        );
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
    use crate::app::{Load, Modal};
    use crate::model::LogEntry;
    use crate::tui::probe;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
            None,
        );
        a.modal = Some(Modal::History);
        a
    }

    fn draw(a: &mut App) -> String {
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| crate::view::draw(f, a)).unwrap();
        probe::screen(&term)
    }

    #[test]
    fn a_commit_row_names_the_sha_the_subject_and_the_author() {
        let mut a = app();
        a.log_state = Load::Ready;
        a.log = vec![LogEntry {
            sha: "a3f19c2000000000000000000000000000000000".into(),
            subject: "fix the parser".into(),
            author: "Maria Okonkwo".into(),
            when: 1_700_000_000,
        }];
        let screen = draw(&mut a);
        assert!(screen.contains("a3f19c2"), "{screen}");
        assert!(screen.contains("fix the parser"), "{screen}");
        assert!(screen.contains("Maria Okonkwo"), "{screen}");
        assert!(screen.contains("1 commits"), "{screen}");

        let rows = a
            .hits
            .iter()
            .rev()
            .find(|r| r.target == Target::Modal && r.len > 0)
            .expect("the rows are clickable");
        assert_eq!(rows.len, 1);
    }

    #[test]
    fn while_the_log_is_on_its_way_the_picker_says_so() {
        let mut a = app();
        a.log_state = Load::Loading;
        let screen = draw(&mut a);
        assert!(screen.contains("reading history…"), "{screen}");
    }

    #[test]
    fn the_commit_under_review_carries_the_dot() {
        let mut a = app();
        a.log_state = Load::Ready;
        let sha = "b".repeat(40);
        a.scope = Scope::Commit { sha: sha.clone() };
        a.log = vec![
            LogEntry {
                sha: "a".repeat(40),
                subject: "newest".into(),
                author: "x".into(),
                when: 1,
            },
            LogEntry {
                sha,
                subject: "current".into(),
                author: "x".into(),
                when: 1,
            },
        ];
        let screen = draw(&mut a);
        let dotted = screen
            .lines()
            .find(|l| l.contains('●'))
            .expect("one row carries the dot");
        assert!(dotted.contains("current"), "{dotted}");
    }
}
