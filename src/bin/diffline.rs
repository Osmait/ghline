//! diffline — review the diff in front of you, and hand notes to an agent.
//!
//! Usage and the headless render write to stdout, which is what stdout is for
//! in a program with a command line; the lint against it guards the library,
//! not this.
#![allow(clippy::print_stdout, reason = "usage and --svg are stdout's job")]

use std::io;
use std::time::{Duration, Instant};

use github_tui::tui::run::{Program, Terminal_, run};

use github_tui::diffline::app::App;
use github_tui::diffline::model::Scope;
use github_tui::diffline::view as ui;

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

    github_tui::shared::config::apply_theme();

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

    // The worker is started here, which is the only place that should decide
    // there is one: a snapshot wants none, and a test wants to hand in its
    // own.
    let mut app = App::new(
        repo,
        opening,
        scopes,
        Some(Box::new(github_tui::diffline::service::Service::spawn())),
    );

    if let Some(i) = svg {
        let keys = args.get(i + 1).cloned().unwrap_or_default();
        let w = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(160);
        let h = args.get(i + 3).and_then(|s| s.parse().ok()).unwrap_or(44);
        return headless(&mut app, &keys, w, h);
    }

    let mouse = !args.iter().any(|a| a == "--no-mouse");
    let mut term = Terminal_::enter(mouse)?;
    let res = run(&mut term, &mut Diffline::new(&mut app));
    // the guard gives the terminal back even if `run` returned an error
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
    println!("  --no-mouse          leave the terminal's own click-to-select alone");
    println!("  --version           print the version and exit");
    println!();
    println!("  [ ]   working tree · this branch · the last commit");
    println!("  V c   select a range, comment on it");
    println!("  a S   pick an agent, send the queue");
    println!("  ?     everything else");
}

/// diffline's timers, and what the runtime needs to run it.
///
/// The timers stay here because they are about what this program draws — a
/// cursor that blinks, skeletons that travel, a toast that ages out — and a
/// runtime that owned them would have to be told about each one anyway.
struct Diffline<'a> {
    app: &'a mut App,
    blink: Instant,
    anim: Instant,
    tick: Instant,
}

impl<'a> Diffline<'a> {
    fn new(app: &'a mut App) -> Self {
        let now = Instant::now();
        Self {
            app,
            blink: now,
            anim: now,
            tick: now,
        }
    }
}

impl Program for Diffline<'_> {
    fn ensure(&mut self) {
        self.app.ensure();
    }

    fn draw(&mut self, f: &mut ratatui::Frame<'_>) {
        ui::draw(f, self.app);
    }

    fn on_key(&mut self, press: github_tui::shared::key::Press) {
        self.app.on_key(press);
    }

    fn on_mouse(&mut self, mouse: github_tui::shared::key::Mouse) {
        self.app.on_mouse(mouse);
    }

    fn drain(&mut self) {
        while let Some(res) = self.app.poll() {
            self.app.apply(res);
        }
    }

    /// The soonest of the three, and only the skeletons when something is
    /// actually on its way: an idle program has nothing to animate.
    fn next_wake(&self) -> Duration {
        BLINK
            .saturating_sub(self.blink.elapsed())
            .min(TICK.saturating_sub(self.tick.elapsed()))
            .min(if self.app.waiting() {
                ANIM.saturating_sub(self.anim.elapsed())
            } else {
                Duration::MAX
            })
    }

    fn on_wake(&mut self) {
        if self.app.waiting() && self.anim.elapsed() >= ANIM {
            self.app.anim = self.app.anim.wrapping_add(1);
            self.anim = Instant::now();
        }
        if self.blink.elapsed() >= BLINK {
            self.app.blink = !self.app.blink;
            self.blink = Instant::now();
        }
        if self.tick.elapsed() >= TICK {
            self.app.tick();
            self.tick = Instant::now();
        }
    }

    fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.app.wants_redraw)
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}

/// Draws one frame into an off-screen terminal and prints it as SVG.
///
/// `settle` waits for the worker rather than guessing at a delay, so the frame
/// is of the finished state and not of whatever had arrived by then.
fn headless(app: &mut App, keys: &str, w: u16, h: u16) -> io::Result<()> {
    use ratatui::backend::TestBackend;

    settle(app);
    for key in github_tui::shared::key::parse_keys(keys) {
        app.on_key(key);
        settle(app);
    }

    let mut term = ratatui::Terminal::new(TestBackend::new(w, h))?;
    term.draw(|f| ui::draw(f, app))?;
    print!(
        "{}",
        github_tui::github::snapshot::to_svg(term.backend().buffer(), w, h)
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
