//! Drawing diffline.
//!
//! Cell painting rather than widgets, and the same primitives the GitHub
//! browser uses — the two look alike because they are drawn with the same
//! hands. The palette is Catppuccin Mocha.
//!
//! One file per thing on screen. It was one file of two thousand lines, which
//! is readable exactly once: after that you are searching it, and a search
//! does not tell you whether the pane you found is the only place it is drawn.

mod diff;
mod header;
mod parts;
mod queue;
mod status;
mod tree;

mod modal {
    pub mod agents;
    pub mod comment;
    pub mod deps;
    pub mod finder;
    pub mod help;
    pub mod palette;
    pub mod themes;
}

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::diffline::app::{App, Modal, Pane};
use crate::diffline::hit::{Region, Target};
use crate::shared::theme;
use crate::tui::{clear, hline, put, vline};

/// The file tree's width, and the queue's. Both fixed: the diff is what the
/// screen is for, and it takes whatever is left.
const TREE_W: u16 = 32;
pub(super) const QUEUE_W: u16 = 44;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    let buf = f.buffer_mut();
    clear(buf, area, theme::bg());

    if area.height < 10 || area.width < 60 {
        put(
            buf,
            0,
            0,
            area.width,
            "terminal too small",
            Style::default().fg(theme::red()).bg(theme::bg()),
        );
        return;
    }

    let header = Rect {
        x: 0,
        y: 0,
        width: area.width,
        height: 1,
    };
    let status = Rect {
        x: 0,
        y: area.height - 1,
        width: area.width,
        height: 1,
    };
    let body = Rect {
        x: 0,
        y: 1,
        width: area.width,
        height: area.height - 2,
    };

    // Cleared first: they describe *this* frame, and a stale rectangle is a
    // click landing on what used to be there.
    app.hits.clear();

    header::header_bar(buf, header, app);
    hline(buf, 0, 1, area.width, theme::border());

    // The side panes give way on a narrow terminal rather than squeezing the
    // diff into nothing: reading the change is the job.
    let tree_w = if app.tree_shown && area.width >= 110 {
        TREE_W
    } else {
        0
    };
    let queue_w = if app.queue_shown && area.width >= 150 {
        QUEUE_W
    } else {
        0
    };
    let body = Rect {
        y: body.y + 1,
        height: body.height - 1,
        ..body
    };

    if tree_w > 0 {
        let r = Rect {
            width: tree_w,
            ..body
        };
        tree::tree(buf, r, app);
        vline(buf, tree_w, body.y, body.height, theme::border());
        app.hits.push(Region::plain(Target::Pane(Pane::Tree), r));
    }
    if queue_w > 0 {
        let x = area.width - queue_w;
        vline(buf, x - 1, body.y, body.height, theme::border());
        let r = Rect {
            x,
            width: queue_w,
            ..body
        };
        queue::queue(buf, r, app);
        app.hits.push(Region::plain(Target::Pane(Pane::Queue), r));
    }
    let mid_x = tree_w + u16::from(tree_w > 0);
    let mid_w = area
        .width
        .saturating_sub(mid_x)
        .saturating_sub(queue_w + u16::from(queue_w > 0));
    let mid = Rect {
        x: mid_x,
        width: mid_w,
        ..body
    };
    diff::diff(buf, mid, app);
    app.hits.push(Region::plain(Target::Pane(Pane::Diff), mid));

    // Drawn last of the body, over the diff's top edge: while the queue is
    // away this is the only thing saying how much is in it.
    if queue_w == 0 {
        header::queue_tab(buf, area, app);
    }

    status::status_bar(buf, status, app);

    match app.modal {
        Some(Modal::Finder) => modal::finder::finder(buf, area, app),
        Some(Modal::Palette) => modal::palette::palette(buf, area, app),
        Some(Modal::Comment) => modal::comment::comment(buf, area, app),
        Some(Modal::Agents) => modal::agents::agents(buf, area, app),
        Some(Modal::Themes) => modal::themes::themes(buf, area, app),
        Some(Modal::Deps) => modal::deps::deps(buf, area, app),
        Some(Modal::Help) => modal::help::help(buf, area, app),
        None => {}
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
    use crate::diffline::model::{ChangedFile, Kind, Row, Scope, Status};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// A file whose lines are far longer than any pane.
    fn app() -> App {
        let mut a = App::new(
            "/tmp/r".into(),
            Scope::WorkingTree,
            vec![Scope::WorkingTree],
        );
        a.service = None;
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

    /// Every row of the screen, as one string.
    fn rows(term: &Terminal<TestBackend>) -> Vec<String> {
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()).to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_frame_records_what_a_click_could_land_on() {
        use crate::diffline::hit::Target;
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(160, 30)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();

        let targets: Vec<Target> = a.hits.iter().map(|r| r.target).collect();
        assert!(
            targets
                .iter()
                .any(|t| matches!(t, Target::Pane(Pane::Tree))),
            "{targets:?}"
        );
        assert!(
            targets
                .iter()
                .any(|t| matches!(t, Target::Pane(Pane::Diff))),
            "{targets:?}"
        );

        // The diff's rows carry a length, or a click could not tell which
        // line it hit.
        let rows = a
            .hits
            .iter()
            .find(|r| matches!(r.target, Target::Pane(Pane::Diff)) && r.len > 0)
            .expect("the diff should offer clickable rows");
        assert_eq!(rows.len, a.diff_rows().len());
    }
    #[test]
    fn the_regions_are_this_frame_and_not_the_last_one() {
        // Stale geometry is a click landing on what used to be there.
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(160, 30)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let first = a.hits.len();
        term.draw(|f| draw(f, &mut a)).unwrap();
        assert_eq!(a.hits.len(), first, "they accumulated instead of clearing");
    }
    #[test]
    fn clicking_a_diff_row_puts_the_cursor_on_it_but_not_on_a_header() {
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(160, 30)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();

        let header = a
            .diff_rows()
            .iter()
            .position(|r| !r.kind.is_code())
            .expect("the fixture opens on a hunk header");
        let code = crate::diffline::app::first_code(a.diff_rows(), 0);

        a.cursor = code;
        a.click_row(header);
        assert_eq!(a.cursor, code, "a header is a coordinate, not a line");
        a.click_row(code);
        assert_eq!(a.cursor, code);
    }
    #[test]
    fn the_panes_give_way_before_the_diff_does() {
        // narrow enough that the side panes have to go
        let mut a = app();
        let mut term = Terminal::new(TestBackend::new(100, 20)).unwrap();
        term.draw(|f| draw(f, &mut a)).unwrap();
        let screen = rows(&term).join("\n");
        assert!(
            screen.contains("MARKER"),
            "the diff is the point and should survive a narrow terminal"
        );
    }
}
