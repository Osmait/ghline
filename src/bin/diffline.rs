//! diffline — review the diff in front of you, and hand notes to an agent.
//!
//! Usage and the headless render write to stdout, which is what stdout is for
//! in a program with a command line; the lint against it guards the library,
//! not this.
#![allow(clippy::print_stdout, reason = "usage and --svg are stdout's job")]

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use github_tui::diffline::app::App;
use github_tui::diffline::model::Scope;
use github_tui::diffline::ui;

/// Cursor blink, and the beat the toast fades on.
const BLINK: Duration = Duration::from_millis(500);
/// Frame rate of the loading skeletons, fast enough to read as motion.
const ANIM: Duration = Duration::from_millis(110);
/// How often the toast is aged out.
const TICK: Duration = Duration::from_millis(1200);

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return Ok(());
    }
    // Before the repository check below: `--version` has to answer from
    // anywhere, including the release runner's checkout and a bug report.
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("diffline {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // The repository is where you are unless you say otherwise, because that
    // is almost always the one whose diff you meant.
    let repo = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| ".".into());

    let repo = std::fs::canonicalize(&repo)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or(repo);

    // Which backend owns this directory, asked once. Nothing below here knows
    // it was git rather than something else.
    let Some(vcs) = github_tui::diffline::vcs::of(&repo) else {
        eprintln!("diffline: {repo} is not a repository anything here can read");
        return Ok(());
    };

    github_tui::config::apply_theme();

    // `--svg <keys> <w> <h>` draws one frame and exits. The same headless
    // check the other program has, and for the same reason: a terminal is not
    // something a test can hold.
    let svg = args.iter().position(|a| a == "--svg");

    let base = vcs.base_branch(&repo);
    let head = vcs.head_branch(&repo).unwrap_or_else(|| "HEAD".into());

    // Three scopes, widening: what is not committed, what this branch has
    // that base does not, and the last commit on its own.
    let mut scopes = vec![Scope::WorkingTree];
    if head != base {
        scopes.push(Scope::Branch { base: base.clone() });
    }
    scopes.push(Scope::Commit { sha: "HEAD".into() });

    // Open on whichever of the first two actually has something in it: a
    // clean tree should not greet you with an empty pane.
    let opening = scopes
        .iter()
        .find(|s| {
            vcs.changed_files(&repo, s)
                .map(|f| !f.is_empty())
                .unwrap_or(false)
        })
        .cloned()
        .unwrap_or(Scope::WorkingTree);

    let mut app = App::new(repo, opening, scopes);

    if let Some(i) = svg {
        let keys = args.get(i + 1).cloned().unwrap_or_default();
        let w = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(160);
        let h = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(44);
        return headless(&mut app, &keys, w, h);
    }

    let mut term = TerminalGuard::enter()?;
    let res = run(&mut term, &mut app);
    drop(term);

    if let Err(e) = &res {
        eprintln!("diffline: {e}");
    }
    res
}

fn usage() {
    println!("diffline — review a diff, and hand notes to a coding agent");
    println!();
    println!("  diffline [path]     the repository to read, default the current directory");
    println!("  --version           print the version and exit");
    println!();
    println!("  [ ]   working tree · this branch · the last commit");
    println!("  V c   select a range, comment on it");
    println!("  a S   pick an agent, send the queue");
    println!("  ?     everything else");
}

/// Holds the terminal in the alternate screen and restores it on drop, even
/// through a panic. Without it a panic leaves the console in raw mode.
struct TerminalGuard {
    term: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(e);
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));

        let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        term.hide_cursor()?;
        Ok(Self { term })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
        let _ = self.term.show_cursor();
    }
}

/// Idempotent and infallible: it is called from the panic hook, where there is
/// nobody to return an error to.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

fn run(term: &mut TerminalGuard, app: &mut App) -> io::Result<()> {
    let mut last_blink = Instant::now();
    let mut last_anim = Instant::now();
    let mut last_tick = Instant::now();

    loop {
        app.ensure();
        term.term.draw(|f| ui::draw(f, app))?;

        // A short wait while anything is in flight, so an answer is drawn as
        // soon as it lands rather than on the next keystroke.
        let waiting = app.waiting();
        let timeout = BLINK
            .saturating_sub(last_blink.elapsed())
            .min(TICK.saturating_sub(last_tick.elapsed()))
            .min(if waiting {
                ANIM.saturating_sub(last_anim.elapsed())
            } else {
                Duration::MAX
            })
            .max(Duration::from_millis(16));

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }

        while let Some(res) = app.poll() {
            app.apply(res);
        }

        // Ratatui writes only the cells that differ from the last frame, so a
        // terminal that got out of step with it stays that way until told to
        // forget what it thought was there.
        if std::mem::take(&mut app.wants_redraw) {
            term.term.clear()?;
        }

        if waiting && last_anim.elapsed() >= ANIM {
            app.anim = app.anim.wrapping_add(1);
            last_anim = Instant::now();
        }
        if last_blink.elapsed() >= BLINK {
            app.blink = !app.blink;
            last_blink = Instant::now();
        }
        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Draws one frame into an off-screen terminal and prints it as SVG.
///
/// `settle` waits for the worker rather than guessing at a delay, so the frame
/// is of the finished state and not of whatever had arrived by then.
fn headless(app: &mut App, keys: &str, w: u16, h: u16) -> io::Result<()> {
    use ratatui::backend::TestBackend;

    settle(app);
    for key in github_tui::snapshot::parse_keys(keys) {
        app.on_key(key);
        settle(app);
    }

    let mut term = Terminal::new(TestBackend::new(w, h))?;
    term.draw(|f| ui::draw(f, app))?;
    print!(
        "{}",
        github_tui::snapshot::to_svg(term.backend().buffer(), w, h)
    );
    Ok(())
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.ensure();
        while let Some(res) = app.poll() {
            app.apply(res);
        }
        if !app.waiting() || Instant::now() > deadline {
            app.ensure();
            if !app.waiting() || Instant::now() > deadline {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
