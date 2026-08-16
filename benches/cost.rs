//! What each layer costs, on the paths that run while somebody holds a key
//! down.
//!
//! The last few rounds of work here were about per-frame allocation — a colour
//! lookup that built a `Vec` every time it was asked, a file read on every
//! draw — and each of them was found by reading the code rather than by
//! measuring it. That works until it stops working: the next one will be a
//! change that looks free and is not, and nothing in the repository would have
//! said so.
//!
//! Grouped the way the crate is, so a number can be read against the module it
//! came from: `shared` is what neither program owns, `tui` is the drawing
//! toolkit, and `github` and `diffline` are one program each. Within a group
//! the order is roughly parse → compute → draw, which is also the order a
//! keystroke travels.
//!
//! Only things on a hot path are here. A function called once at startup can
//! be as slow as it likes, and measuring it would only add a number nobody
//! should act on — `herdr`, `config`, `clones::scan` and the `gh` calls all
//! shell out or touch the disk, so they are absent by the same rule.
//!
//! ```sh
//! cargo bench                      # all of them
//! cargo bench -- draw              # every bench whose path contains "draw"
//! cargo bench -- shared::fuzzy     # one group
//! ```
//!
//! These are not run in CI. A shared runner's numbers vary by more than the
//! changes worth catching, so a red tick there would mean the machine was
//! busy; the useful comparison is two runs on the same desk.
//!
//! For *where* the time goes rather than how much of it there is, `make flame`
//! runs this same binary under `perf` and folds the result into a flame graph.
//! It rebuilds with symbols and frame pointers first, because the profile that
//! ships has neither — see the target in the Makefile.

use divan::Bencher;
use divan::black_box;

fn main() {
    divan::main();
}

// ------------------------------------------------------------------ fixtures

/// A source file the size of the ones the lexer is pointed at, with strings,
/// comments and a block comment in it — a block comment is the case that makes
/// the whole file one pass rather than one line at a time.
fn source_file() -> String {
    let unit = r#"
/// A doc comment, which is the common case in this repository.
pub fn one(name: &str, count: usize) -> String {
    let greeting = "hello, world"; // a string and a trailing comment
    /* a block comment
       running over two lines */
    format!("{greeting} {name} {count}")
}
"#;
    unit.repeat(75)
}

/// `git diff` output for one file, in the shape the parser actually meets:
/// a preamble to skip, headers to read coordinates out of, and runs of
/// additions and deletions long enough that the numbering has to keep up.
fn unified_diff(hunks: usize) -> String {
    let mut out = String::from(
        "diff --git a/src/layout/solver.rs b/src/layout/solver.rs\n\
         index 1a2b3c4..5d6e7f8 100644\n\
         --- a/src/layout/solver.rs\n\
         +++ b/src/layout/solver.rs\n",
    );
    for h in 0..hunks {
        let at = 1 + h * 40;
        out.push_str(&format!("@@ -{at},18 +{at},20 @@ impl Solver {{\n"));
        for i in 0..6 {
            out.push_str(&format!("     let width_{i} = cols.saturating_sub(bar);\n"));
        }
        for i in 0..4 {
            out.push_str(&format!("-    let old_{i} = area.width;\n"));
        }
        for i in 0..6 {
            out.push_str(&format!("+    let new_{i} = area.width.min(limit);\n"));
        }
        // A tab, because expanding them is what the parser pays per line.
        for i in 0..4 {
            out.push_str(&format!("     \t// trailing context {i}\n"));
        }
    }
    out
}

/// A list the size of somebody's repositories.
fn repo_names() -> Vec<String> {
    (0..500)
        .map(|i| format!("marasanz/some-repository-name-{i}"))
        .collect()
}

// -------------------------------------------------------------------- shared

mod shared {
    use super::{Bencher, black_box, repo_names, source_file};

    use github_tui::shared::{ago, clones, fuzzy, icons, key, settings, syntax, text};

    /// One candidate, which is what `rank` pays five hundred times over. Split
    /// out from it so a change to the scorer can be seen without the list
    /// around it.
    #[divan::bench]
    fn fuzzy_score() -> Option<(i32, Vec<usize>)> {
        fuzzy::score(
            black_box("srn2"),
            black_box("marasanz/some-repository-name-42"),
        )
    }

    /// The finder, over a list the size of somebody's repositories, on a query
    /// of the length people actually type. Paid on every character typed.
    #[divan::bench]
    fn fuzzy_rank(bencher: Bencher<'_, '_>) {
        let items = repo_names();
        bencher.bench(|| fuzzy::rank(black_box("srn2"), &items, String::as_str));
    }

    /// The same list against a query that matches nothing. The worse case by a
    /// long way: a hit stops at the character that fails, and a miss only
    /// stops after the whole haystack has been walked.
    #[divan::bench]
    fn fuzzy_rank_miss(bencher: Bencher<'_, '_>) {
        let items = repo_names();
        bencher.bench(|| fuzzy::rank(black_box("zzqx"), &items, String::as_str));
    }

    /// Which lexer a path gets. Called once per file rather than per line, but
    /// the file tree calls it for every visible row.
    #[divan::bench]
    fn syntax_of_path() -> Option<&'static syntax::Lang> {
        syntax::of_path(black_box("src/github/state/app/input.rs"))
    }

    /// The lexer over a file of a realistic size — whole-file by design, so it
    /// is paid per file opened rather than per line drawn.
    #[divan::bench]
    fn syntax_highlight(bencher: Bencher<'_, '_>) {
        let text = source_file();
        let Some(lang) = syntax::of_path("src/main.rs") else {
            eprintln!("no lexer for .rs");
            return;
        };
        bencher.bench(|| syntax::highlight(lang, black_box(&text)));
    }

    /// Wrapping one long line, which the diff pane does per visible row when
    /// the content is wider than the pane.
    #[divan::bench]
    fn syntax_wrap_ranges(bencher: Bencher<'_, '_>) {
        let line = "    let content = cols.saturating_sub(sidebar); // a comment that runs on and on past the edge of any pane it is drawn into".repeat(4);
        bencher.bench(|| syntax::wrap_ranges(black_box(&line), 80));
    }

    /// The common case, which is a line with no tab in it: the function's
    /// whole shape is built around returning that one borrowed rather than
    /// copied, and this is the number that says whether that was worth it.
    #[divan::bench]
    fn text_expand_tabs_clean(bencher: Bencher<'_, '_>) {
        let line = "    let content = cols.saturating_sub(sidebar);".repeat(3);
        bencher.bench(|| text::expand_tabs(black_box(&line)));
    }

    /// And the case that has to copy.
    #[divan::bench]
    fn text_expand_tabs_tabbed(bencher: Bencher<'_, '_>) {
        let line = "\tlet content = cols\n\t\t.saturating_sub(sidebar);".repeat(3);
        bencher.bench(|| text::expand_tabs(black_box(&line)));
    }

    /// The keymap notation. Read when a binding is loaded, not per keystroke —
    /// here because the tests and the demo drive the app through it, so it is
    /// on the path every snapshot pays.
    #[divan::bench]
    fn key_parse_keys() -> Vec<key::Press> {
        key::parse_keys(black_box("jjjk<c-d>/sidebar<enter><esc>"))
    }

    /// The settings file. Read on every `settings::current()` miss.
    #[divan::bench]
    fn settings_parse(bencher: Bencher<'_, '_>) {
        let text = "# a comment\ntheme = mocha\nfile-icons = nerd\nmultiplexer = zellij\n\nagent-kinds = claude, codex, pi\nagent-icon-claude = *\n".repeat(4);
        bencher.bench(|| settings::parse(black_box(&text)));
    }

    /// A timestamp into `3h ago`, once per row of every list.
    #[divan::bench]
    fn ago_since() -> String {
        ago::since(black_box(1_700_000_000))
    }

    /// The glyph a file gets, once per row of the tree and the explorer.
    #[divan::bench]
    fn icons_language() -> &'static str {
        icons::language(black_box("src/diffline/state/input.rs"))
    }

    /// A remote URL into `owner/repo`, once per checkout found.
    #[divan::bench]
    fn clones_slug_of() -> Option<String> {
        clones::slug_of(black_box("git@github.com:Osmait/github-tui.git"))
    }
}

// ----------------------------------------------------------------------- tui

mod tui {
    use super::{Bencher, black_box};

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use github_tui::tui::{atom, diff, geom, theme};

    /// The size the design was drawn at.
    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 160,
        height: 44,
    };

    /// Clearing the screen, which every frame starts with.
    #[divan::bench]
    fn atom_fill(bencher: Bencher<'_, '_>) {
        let mut buf = Buffer::empty(AREA);
        bencher.bench_local(|| atom::fill(&mut buf, black_box(AREA), theme::bg()));
    }

    /// One string into the buffer. The single most-called function in the
    /// crate: every label, every row, every number goes through it.
    #[divan::bench]
    fn atom_put(bencher: Bencher<'_, '_>) {
        let mut buf = Buffer::empty(AREA);
        let style = Style::default().fg(theme::fg()).bg(theme::bg());
        bencher.bench_local(|| {
            atom::put(
                &mut buf,
                2,
                4,
                158,
                black_box("src/github/state/app/input.rs"),
                style,
            )
        });
    }

    /// Fitting a name to a column, once per row of every list.
    #[divan::bench]
    fn atom_truncate_pad() -> String {
        atom::truncate_pad(black_box("src/github/state/app/input.rs"), 24)
    }

    /// Wrapping a paragraph, which the detail pane pays per body it shows.
    #[divan::bench]
    fn atom_wrap(bencher: Bencher<'_, '_>) {
        let text = "The layout solver was picking the wrong constraint when two panes asked for the same column, and the fix is to give the one on the right the remainder. ".repeat(8);
        bencher.bench(|| atom::wrap(black_box(&text), 72));
    }

    /// Keeping the selection on screen, once per keypress that moves it.
    #[divan::bench]
    fn geom_scroll_into_view(bencher: Bencher<'_, '_>) {
        bencher.bench_local(|| {
            let mut offset = 0usize;
            geom::scroll_into_view(&mut offset, black_box(420), 40, 500);
            offset
        });
    }

    /// Folding a file's lines into side-by-side pairs, once per file opened in
    /// the split view.
    #[divan::bench]
    fn diff_pair(bencher: Bencher<'_, '_>) {
        let mut sides = Vec::new();
        for _ in 0..100 {
            sides.push(diff::Side::Header);
            for _ in 0..6 {
                sides.push(diff::Side::Context);
            }
            for _ in 0..4 {
                sides.push(diff::Side::Deleted);
            }
            for _ in 0..6 {
                sides.push(diff::Side::Added);
            }
        }
        bencher.bench(|| diff::pair(black_box(&sides)));
    }

    /// A theme file over the top of Mocha. Once per theme switch, and the
    /// switch is meant to be instant.
    #[divan::bench]
    fn theme_parse_palette(bencher: Bencher<'_, '_>) {
        let (_, text) = theme::template("bench");
        bencher.bench(|| theme::parse_palette(black_box(&text)));
    }
}

// -------------------------------------------------------------------- github

mod github {
    use super::{Bencher, black_box};

    use github_tui::github::data::{Hunk, LogKind, RawLog};
    use github_tui::github::{snapshot, ui};
    use github_tui::shared::key::{Key, Press};

    /// One frame of the demo, drawn into an off-screen terminal at the size
    /// the design was drawn at, for each of the screens a key can reach.
    ///
    /// The app is built once, outside the timer: building it parses the
    /// fixture, which is not what a frame costs.
    fn draw_view(bencher: Bencher<'_, '_>, keys: &str) {
        let mut app = snapshot::demo(keys, 0);
        // Infallible from ratatui 0.30 on, so there is no error arm to write.
        let Ok(mut term) = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 44));
        bencher.bench_local(|| {
            let _ = term.draw(|f| ui::draw(f, &mut app));
        });
    }

    /// What every keystroke and every 16ms tick pays on the screen it opens on.
    #[divan::bench]
    fn draw(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "");
    }

    /// The diff, which is the widest thing drawn: two gutters, a split, a
    /// scrollbar, and the lexer's output underneath all of it.
    #[divan::bench]
    fn draw_diff(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "<enter>d");
    }

    #[divan::bench]
    fn draw_actions(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "3");
    }

    /// The file tree, which is the one with an icon and a language colour per
    /// row.
    #[divan::bench]
    fn draw_files(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "4");
    }

    /// A modal over a pane: the frame underneath is still drawn, and the scrim
    /// walks every cell of it a second time.
    #[divan::bench]
    fn draw_help(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "?");
    }

    /// The finder, which re-ranks on every character and draws the result.
    #[divan::bench]
    fn draw_finder(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "psid");
    }

    /// The input path with no drawing in it. Down and back up, so the app is
    /// where it started and the number is two keypresses rather than one — a
    /// bench that walked to the end of the list would be measuring the no-op
    /// at the bottom of it.
    #[divan::bench]
    fn on_key(bencher: Bencher<'_, '_>) {
        let mut app = snapshot::demo("", 0);
        let (down, up) = (Press::new(Key::Char('j')), Press::new(Key::Char('k')));
        bencher.bench_local(|| {
            app.on_key(black_box(down));
            app.on_key(black_box(up));
        });
    }

    /// The 16ms timer, which is what animates a spinner and expires a toast.
    #[divan::bench]
    fn tick(bencher: Bencher<'_, '_>) {
        let mut app = snapshot::demo("", 0);
        bencher.bench_local(|| app.tick());
    }

    /// Expanding a pull request's hunks into numbered lines, once per file of
    /// the diff.
    #[divan::bench]
    fn data_hunk_rows(bencher: Bencher<'_, '_>) {
        let hunks: Vec<Hunk> = (0..40)
            .map(|h| {
                let at = 1 + h * 40;
                let mut lines = Vec::new();
                for i in 0..6 {
                    lines.push((' ', format!("    let width_{i} = cols;")));
                }
                for i in 0..4 {
                    lines.push(('-', format!("    let old_{i} = area.width;")));
                }
                for i in 0..6 {
                    lines.push(('+', format!("    let new_{i} = area.width.min(l);")));
                }
                Hunk {
                    hdr: format!("@@ -{at},18 +{at},20 @@ impl Solver {{"),
                    lines,
                }
            })
            .collect();
        bencher.bench(|| Hunk::rows(black_box(&hunks)));
    }

    /// Narrowing a run's log to one step, which is paid again on every poll
    /// while the run is still going.
    #[divan::bench]
    fn data_filter_log(bencher: Bencher<'_, '_>) {
        let raw: Vec<RawLog> = (0..4000)
            .map(|i| RawLog {
                job: format!("build ({})", i % 3),
                step: format!("step {}", i % 12),
                time: "2026-08-15T18:04:11.2260000Z".into(),
                text: format!("    Compiling some-crate-{i} v0.1.0"),
                kind: LogKind::Plain,
            })
            .collect();
        bencher.bench(|| {
            github_tui::github::data::filter_log(black_box(&raw), "build (1)", Some("step 4"))
        });
    }
}

// ------------------------------------------------------------------ diffline

mod diffline {
    use super::{Bencher, black_box, unified_diff};

    use github_tui::diffline::model::{Kind, Row, pair_rows};
    use github_tui::diffline::view::snapshot;
    use github_tui::diffline::{git, view};
    use github_tui::shared::key::{Key, Press};

    fn draw_view(bencher: Bencher<'_, '_>, keys: &str) {
        let mut app = snapshot::demo(keys);
        // Infallible from ratatui 0.30 on, so there is no error arm to write.
        let Ok(mut term) = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 44));
        bencher.bench_local(|| {
            let _ = term.draw(|f| view::draw(f, &mut app));
        });
    }

    /// One frame of the review: tree, diff and queue.
    #[divan::bench]
    fn draw(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "");
    }

    /// The split view, which pairs the rows before it can draw them.
    #[divan::bench]
    fn draw_split(bencher: Bencher<'_, '_>) {
        draw_view(bencher, " v");
    }

    /// The finder over the changed files.
    #[divan::bench]
    fn draw_finder(bencher: Bencher<'_, '_>) {
        draw_view(bencher, "/sid");
    }

    /// The same two-keypress shape as github-tui's: down and back up.
    #[divan::bench]
    fn on_key(bencher: Bencher<'_, '_>) {
        let mut app = snapshot::demo("");
        let (down, up) = (Press::new(Key::Char('j')), Press::new(Key::Char('k')));
        bencher.bench_local(|| {
            app.on_key(black_box(down));
            app.on_key(black_box(up));
        });
    }

    /// `git diff` into rows, once per file opened. Forty hunks is a large file
    /// rather than a typical one, which is the size worth watching: a typical
    /// one is fast whatever this does.
    #[divan::bench]
    fn git_parse_unified(bencher: Bencher<'_, '_>) {
        let text = unified_diff(40);
        bencher.bench(|| git::parse_unified(black_box(&text)));
    }

    /// Pairing those rows for the split view, once per file shown in it.
    #[divan::bench]
    fn model_pair_rows(bencher: Bencher<'_, '_>) {
        let text = unified_diff(40);
        let rows: Vec<Row> = git::parse_unified(&text);
        debug_assert!(rows.iter().any(|r| r.kind == Kind::Added));
        bencher.bench(|| pair_rows(black_box(&rows)));
    }
}
