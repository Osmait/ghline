//! What a frame costs, and what the three things inside it cost.
//!
//! The last few rounds of work here were about per-frame allocation — a colour
//! lookup that built a `Vec` every time it was asked, a file read on every
//! draw — and each of them was found by reading the code rather than by
//! measuring it. That works until it stops working: the next one will be a
//! change that looks free and is not, and nothing in the repository would have
//! said so.
//!
//! Four benchmarks, chosen for being on the path that runs while somebody is
//! holding a key down:
//!
//! - `draw` — one whole frame of the demo, which is what every keystroke and
//!   every 16ms tick pays for.
//! - `highlight` — the lexer, over a file the size of the ones this is used on.
//!   Whole-file by design, so it is paid per file rather than per line.
//! - `rank` — the fuzzy matcher over a list the size of a repository's, which
//!   is what the finder pays on every character typed into it.
//! - `wrap_ranges` — wrapping one long line, which the diff pane pays per
//!   visible row.
//!
//! ```sh
//! cargo bench                      # all of them
//! cargo bench -- draw              # one
//! ```
//!
//! These are not run in CI. A shared runner's numbers vary by more than the
//! changes worth catching, so a red tick there would mean the machine was
//! busy; the useful comparison is two runs on the same desk.

use divan::Bencher;
use divan::black_box;

use github_tui::github::snapshot;
use github_tui::shared::{fuzzy, syntax};

fn main() {
    divan::main();
}

/// One frame of the demo, drawn into an off-screen terminal at the size the
/// design was drawn at.
///
/// The app is built once, outside the timer: building it parses the fixture,
/// which is not what a frame costs.
#[divan::bench]
fn draw(bencher: Bencher<'_, '_>) {
    let mut app = snapshot::demo("", 0);
    let mut term = match ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 44)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("no terminal to draw into: {e}");
            return;
        }
    };
    bencher.bench_local(|| {
        let _ = term.draw(|f| github_tui::github::ui::draw(f, &mut app));
    });
}

/// The lexer over a file of a realistic size — six hundred lines with strings,
/// comments and a block comment in them, since a block comment is the case
/// that makes the whole file one pass rather than one line at a time.
#[divan::bench]
fn highlight(bencher: Bencher<'_, '_>) {
    let unit = r#"
/// A doc comment, which is the common case in this repository.
pub fn one(name: &str, count: usize) -> String {
    let greeting = "hello, world"; // a string and a trailing comment
    /* a block comment
       running over two lines */
    format!("{greeting} {name} {count}")
}
"#;
    let text = unit.repeat(75);
    let Some(lang) = syntax::of_path("src/main.rs") else {
        eprintln!("no lexer for .rs");
        return;
    };
    bencher.bench(|| syntax::highlight(lang, black_box(&text)));
}

/// The finder, over a list the size of somebody's repositories, on a query of
/// the length people actually type.
#[divan::bench]
fn rank(bencher: Bencher<'_, '_>) {
    let items: Vec<String> = (0..500)
        .map(|i| format!("marasanz/some-repository-name-{i}"))
        .collect();
    bencher.bench(|| fuzzy::rank(black_box("srn2"), &items, String::as_str));
}

/// Wrapping one long line, which the diff pane does per visible row when the
/// content is wider than the pane.
#[divan::bench]
fn wrap_ranges(bencher: Bencher<'_, '_>) {
    let line = "    let content = cols.saturating_sub(sidebar); // a comment that runs on and on past the edge of any pane it is drawn into".repeat(4);
    bencher.bench(|| syntax::wrap_ranges(black_box(&line), 80));
}
