//! The bar across the top: what is being diffed, and how much of it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::parts::{count_style, put_right_parts};
use crate::diffline::app::App;
use crate::diffline::hit::{Region, Target};
use crate::tui::theme;
use crate::tui::{fill, put, put_trunc};

pub(super) fn header_bar(buf: &mut Buffer, area: Rect, app: &mut App) {
    fill(buf, area, theme::panel());
    let base = Style::default().bg(theme::panel());

    let mut x = put(
        buf,
        0,
        0,
        area.right(),
        " DIFFLINE ",
        Style::default()
            .bg(theme::yellow())
            .fg(theme::panel())
            .add_modifier(Modifier::BOLD),
    );

    x = put(buf, x + 1, 0, area.right(), "⎇ ", base.fg(theme::purple()));
    x = put_trunc(
        buf,
        x,
        0,
        area.right() / 2,
        &app.scope.to_string(),
        base.fg(theme::bright()),
    );

    // The scopes, as tabs. The one in force is inverted.
    x += 2;
    for (i, s) in app.scopes.iter().enumerate() {
        let on = *s == app.scope;
        let style = if on {
            base.bg(theme::fg()).fg(theme::panel())
        } else {
            base.fg(theme::dim())
        };
        let w = s.to_string().width() as u16 + 2;
        app.hits.push(Region::plain(
            Target::Scope(i),
            Rect {
                x,
                y: 0,
                width: w,
                height: 1,
            },
        ));
        x = put(buf, x, 0, area.right(), &format!(" {s} "), style);
        x += 1;
    }

    let (add, del) = app
        .files
        .iter()
        .fold((0u32, 0u32), |(a, d), f| (a + f.add, d + f.del));
    let dim = base.fg(theme::dimmer());
    let rest = format!(
        "  │  {} files  │  {} queued ",
        app.files.len(),
        app.comments.len()
    );
    put_right_parts(
        buf,
        area.right(),
        0,
        &[
            (&format!("+{add}"), count_style(base, add, true)),
            ("  ", dim),
            (&format!("−{del}"), count_style(base, del, false)),
            (&rest, dim),
        ],
    );
}

/// The count of queued comments, as a tab hanging off the top edge.
///
/// Only while the queue itself is hidden. It names its own key, because a
/// pane you cannot see is a pane you have to be told how to open.
pub(super) fn queue_tab(buf: &mut Buffer, area: Rect, app: &mut App) {
    let n = app.comments.len();
    let label = if n == 0 {
        " no comments · ␣c ".to_string()
    } else {
        format!(" ● {n} queued · ␣c ")
    };
    let w = label.width() as u16;
    if w + 4 > area.width {
        return;
    }
    // On the rule under the header, right-aligned: out of the way of the code
    // and of the file names, and on the one row that is already a border.
    let style = Style::default().fg(theme::panel()).bg(if n == 0 {
        theme::dimmer()
    } else {
        theme::yellow()
    });
    let x = area.width - w - 2;
    put(buf, x, 1, area.width, &label, style);
    app.hits.push(Region::plain(
        Target::QueueTab,
        Rect {
            x,
            y: 1,
            width: w,
            height: 1,
        },
    ));
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
    use crate::diffline::model::State;
    use crate::diffline::model::{ChangedFile, Kind, Row, Scope, Status};
    use crate::diffline::view::draw;
    use crate::tui::probe;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// A file whose lines are far longer than any pane.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
            None,
        );
        a.files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: Status::Added,
            add: 3,
            del: 0,
        }];
        a.files_state = crate::diffline::app::Load::Ready;
        let long = "// ".to_string() + &"MARKER_".repeat(40);
        a.rows.insert(
            "src/a.rs".into(),
            vec![
                Row {
                    kind: Kind::Header,
                    old: None,
                    new: None,
                    text: "@@ -0,0 +1,3 @@".into(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(1),
                    text: long.clone(),
                },
                Row {
                    kind: Kind::Added,
                    old: None,
                    new: Some(2),
                    text: long,
                },
            ],
        );
        a.rows_state
            .insert("src/a.rs".into(), crate::diffline::app::Load::Ready);
        // Coloured, as the real thing is: the uncoloured path and the
        // coloured one write the line differently, and only one of them was
        // being exercised before.
        let rows = a.rows["src/a.rs"].clone();
        let spans = rows
            .iter()
            .map(|r| {
                crate::shared::syntax::of_path("a.rs")
                    .map(|l| {
                        crate::shared::syntax::highlight(l, &r.text)
                            .pop()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
            })
            .collect();
        a.spans.insert("src/a.rs".into(), spans);
        a.cursor = 1;
        a
    }

    #[test]
    fn the_tab_says_how_many_are_queued_while_the_queue_is_away() {
        // The queue is hidden to begin with, so this count is the only thing
        // saying there is anything in it at all.
        let mut a = app();
        a.queue_shown = false;
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = probe::screen(&term);
        assert!(screen.contains("no comments"), "{screen}");

        a.comments.push(crate::diffline::model::Comment {
            anchors: vec![],
            file: "src/a.rs".into(),
            snippet: "fn main() {".into(),
            body: "look at this".into(),
            state: State::Queued,
        });
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = probe::screen(&term);
        assert!(screen.contains("1 queued"), "{screen}");

        // and it gets out of the way once the queue itself is on screen
        a.queue_shown = true;
        let mut term = Terminal::new(TestBackend::new(160, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = probe::screen(&term);
        assert!(!screen.contains("queued · "), "{screen}");
    }
    #[test]
    fn the_counts_are_green_and_red_and_a_zero_is_neither() {
        let mut a = app();
        a.files = vec![
            ChangedFile {
                path: "src/a.rs".into(),
                status: Status::Added,
                add: 12,
                del: 0,
            },
            ChangedFile {
                path: "src/b.rs".into(),
                status: Status::Modified,
                add: 0,
                del: 7,
            },
        ];
        let mut term = Terminal::new(TestBackend::new(150, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let buf = term.backend().buffer();

        // Walk the cells and collect the colour each run of digits was in,
        // keyed by the sign in front of it.
        let mut seen: Vec<(char, ratatui::style::Color)> = Vec::new();
        for y in 0..buf.area.height {
            let mut sign = None;
            for x in 0..buf.area.width {
                let Some(cell) = buf.cell((x, y)) else {
                    continue;
                };
                match cell.symbol() {
                    "+" => sign = Some('+'),
                    "−" => sign = Some('-'),
                    sym if sym.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                        if let Some(s) = sign.take()
                            && let Some(fg) = cell.fg.into()
                        {
                            seen.push((s, fg));
                        }
                    }
                    _ => sign = None,
                }
            }
        }

        assert!(
            seen.iter().any(|(s, c)| *s == '+' && *c == theme::green()),
            "an addition count should be green: {seen:?}"
        );
        assert!(
            seen.iter().any(|(s, c)| *s == '-' && *c == theme::red()),
            "a deletion count should be red: {seen:?}"
        );
        assert!(
            seen.iter().any(|(_, c)| *c == theme::dimmer()),
            "a zero should stay quiet rather than shout its colour: {seen:?}"
        );
    }
}
