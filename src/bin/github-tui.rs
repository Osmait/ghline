//! gh-tui — a GitHub TUI built from the `GitHub TUI.dc.html` design.
//!
//! Usage and `--version` write to stdout, which is what stdout is for in a
//! program with a command line; the lint against it guards the library.
#![allow(clippy::print_stdout, reason = "usage and --version are stdout's job")]

use std::io;
use std::time::{Duration, Instant};

use github_tui::tui::run::{Handover, Program, Terminal_, run};

use github_tui::github::app::{App, Source};
use github_tui::github::{gh, snapshot, ui};
use github_tui::shared::config;

fn usage() {
    println!("github-tui — repositories, issues, pull requests and Actions, over the gh CLI");
    println!();
    println!("  github-tui            read real GitHub through `gh`");
    println!("  --demo                the design's fixture, no network needed");
    println!("  --no-mouse            leave the terminal's own click-to-select alone");
    println!("  --log <file>          record the session; the last line replays it");
    println!("  --version             print the version and exit");
    println!();
    println!("  ?                     every key, once you are inside");
}

/// Heartbeat for the log stream (the design's 1400 ms `setInterval`).
const TICK: Duration = Duration::from_millis(1400);
/// Command-line cursor blink (`@keyframes om-blink`).
const BLINK: Duration = Duration::from_millis(500);
/// Frame rate of the loading skeletons, fast enough to read as motion.
const ANIM: Duration = Duration::from_millis(110);
/// How long typing has to pause before the finder asks GitHub. Long enough
/// that a word is one request, short enough not to feel deliberate.
const FIND: Duration = Duration::from_millis(260);

fn main() -> io::Result<()> {
    // `--snapshot [keys] [width] [height] [ticks]` prints one render and exits.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    if matches!(
        mode,
        "--snapshot" | "--svg" | "--svg-live" | "--svg-loading"
    ) {
        let keys = args.get(1).cloned().unwrap_or_default();
        let w = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(160);
        let h = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(44);
        let ticks = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
        return match mode {
            "--svg" => snapshot::svg(&keys, w, h, ticks),
            "--svg-live" => snapshot::svg_live(&keys, w, h, ticks),
            "--svg-loading" => snapshot::svg_loading(&keys, w, h, ticks as u64),
            _ => snapshot::run(&keys, w, h, ticks),
        };
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("github-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Deliberately after the headless modes: a snapshot has to render the same
    // frame on any machine, so it stays on the default theme regardless of
    // what this user picked.
    config::apply_theme();

    // `--demo` forces the design's data; with no signed-in `gh` we fall back to
    // demo anyway rather than starting an empty interface.
    let source = if args.iter().any(|a| a == "--demo") {
        Source::Demo
    } else if gh::available() {
        Source::Live
    } else {
        eprintln!("gh is unavailable or not signed in; starting in demo mode.");
        eprintln!("run `gh auth login` to use real data.");
        Source::Demo
    };

    // Capturing the mouse takes the terminal's own click-to-select with it,
    // so anyone who copies text out of here more than they click needs a way
    // to say no.
    let mouse = !args.iter().any(|a| a == "--no-mouse");

    // Before the terminal is taken: a file that cannot be opened is worth
    // saying so about while there is still a screen to say it on.
    if let Some(i) = args.iter().position(|a| a == "--log") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| {
            eprintln!("gh-tui: --log wants a file; writing to github-tui.log");
            "github-tui.log".into()
        });
        if let Err(e) = github_tui::shared::log::to(std::path::Path::new(&path), "github-tui") {
            eprintln!("gh-tui: cannot write to {path}: {e}");
            return Ok(());
        }
        github_tui::shared::log::say(format_args!(
            "source {}",
            if matches!(source, Source::Demo) {
                "demo"
            } else {
                "gh"
            }
        ));
    }

    // Not `?`: "it does not start" is the report a log is most wanted for,
    // and propagating here would close the file having written the header and
    // nothing else.
    let mut term = match Terminal_::enter(mouse) {
        Ok(t) => t,
        Err(e) => {
            github_tui::shared::log::say(format_args!("could not take the terminal: {e}"));
            return Err(e);
        }
    };
    let res = run(&mut term, &mut GithubTui::new(source));
    // the guard restores the terminal even if `run` returns an error
    drop(term);

    if let Err(e) = &res {
        github_tui::shared::log::say(format_args!("ended with: {e}"));
        eprintln!("gh-tui: {e}");
    }
    github_tui::shared::log::finish();
    res
}

/// Runs the reader's editor on a file, at a line.
///
/// `$VISUAL` then `$EDITOR` then nvim then vim: the first two are what the
/// reader has said they want, and the last two are what is most likely to be
/// there when they have said nothing.
fn edit(path: &std::path::Path, line: usize) -> io::Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if which("nvim") {
                "nvim".into()
            } else {
                "vim".into()
            }
        });

    // `+N` is understood by vi, vim, nvim, emacs, nano and kak alike. An
    // editor that does not know it is handed a file it can still open.
    let mut parts = editor.split_whitespace();
    let Some(bin) = parts.next() else {
        return Ok(());
    };
    std::process::Command::new(bin)
        .args(parts)
        .arg(format!("+{line}"))
        .arg(path)
        .status()
        .map(|_| ())
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(bin).is_file()))
}

/// github-tui's timers, and what the runtime needs to run it.
struct GithubTui {
    app: App,
    tick: Instant,
    blink: Instant,
    anim: Instant,
    find: Instant,
}

impl GithubTui {
    fn new(source: Source) -> Self {
        let now = Instant::now();
        Self {
            app: App::new(
                source,
                // Demo mode has nothing to ask, so it gets no thread.
                // Demo mode has nothing to ask, so it gets no thread.
                (source == Source::Live).then(|| {
                    Box::new(github_tui::github::service::Service::spawn())
                        as Box<dyn github_tui::shared::worker::Worker<_, _>>
                }),
            ),
            tick: now,
            blink: now,
            anim: now,
            find: now,
        }
    }
}

impl Program for GithubTui {
    fn ensure(&mut self) {
        self.app.ensure();
    }

    fn draw(&mut self, f: &mut ratatui::Frame<'_>) {
        ui::draw(f, &mut self.app);
    }

    fn on_key(&mut self, press: github_tui::shared::key::Press) {
        self.app.on_key(press);
    }

    fn on_mouse(&mut self, mouse: github_tui::shared::key::Mouse) {
        self.app.on_mouse(mouse);
    }

    fn drain(&mut self) {
        while let Some(res) = self.app.poll_service() {
            self.app.apply(res);
        }
    }

    /// The soonest of four, two of them conditional: skeletons only travel
    /// while something is on its way, and the finder's debounce only matters
    /// while the finder is open.
    fn next_wake(&self) -> Duration {
        TICK.saturating_sub(self.tick.elapsed())
            .min(BLINK.saturating_sub(self.blink.elapsed()))
            .min(if self.app.waiting() {
                ANIM.saturating_sub(self.anim.elapsed())
            } else {
                Duration::MAX
            })
            .min(if self.app.finder_open {
                FIND.saturating_sub(self.find.elapsed())
            } else {
                Duration::MAX
            })
    }

    fn on_wake(&mut self) {
        if self.tick.elapsed() >= TICK {
            self.app.tick();
            self.tick = Instant::now();
        }
        if self.find.elapsed() >= FIND {
            self.app.finder_tick();
            self.find = Instant::now();
        }
        if self.app.waiting() && self.anim.elapsed() >= ANIM {
            self.app.anim = self.app.anim.wrapping_add(1);
            self.anim = Instant::now();
        }
        if self.blink.elapsed() >= BLINK {
            self.app.blink = !self.app.blink;
            self.blink = Instant::now();
        }
    }

    fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.app.wants_redraw)
    }

    /// The editor. It wants the whole terminal, so the runtime gives it back
    /// between frames rather than in the middle of one.
    fn take_handover(&mut self) -> Option<Handover> {
        let (path, line) = self.app.edit_request.take()?;
        Some(Box::new(move || edit(&path, line)))
    }

    fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}
