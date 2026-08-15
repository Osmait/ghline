//! gh-tui — a GitHub TUI built from the `GitHub TUI.dc.html` design.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use github_tui::app::{App, Source};
use github_tui::{config, gh, snapshot, ui};

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

    let mut term = TerminalGuard::enter(mouse)?;
    let res = run(&mut term, source);
    // the guard restores the terminal even if `run` returns an error
    drop(term);

    if let Err(e) = &res {
        eprintln!("gh-tui: {e}");
    }
    res
}

/// Holds the terminal in the alternate screen while alive and restores it on
/// drop, even if the thread panics. Without it a panic would leave the console
/// in raw mode with no echo.
struct TerminalGuard {
    term: Terminal<CrosstermBackend<io::Stdout>>,
    /// Whether mouse capture was on, so suspending and resuming puts back what
    /// was there rather than what the default happens to be.
    mouse: bool,
}

impl TerminalGuard {
    fn enter(mouse: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        let entered = if mouse {
            execute!(out, EnterAlternateScreen, EnableMouseCapture)
        } else {
            execute!(out, EnterAlternateScreen)
        };
        if let Err(e) = entered {
            let _ = disable_raw_mode();
            return Err(e);
        }
        // a panic only skips the guard's Drop when it aborts; on unwind the
        // hook leaves the terminal usable before the message is printed
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));

        let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        term.hide_cursor()?;
        Ok(Self { term, mouse })
    }

    fn inner(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.term
    }

    /// Hands the terminal to another program and takes it back afterwards.
    ///
    /// An editor wants the real terminal: its own screen, its own raw mode,
    /// its own mouse handling. Anything less and it draws into ours. The
    /// restore on the way back is unconditional, so an editor that dies badly
    /// still leaves this program with a terminal it can use.
    fn suspend<T>(&mut self, run: impl FnOnce() -> T) -> io::Result<T> {
        restore();
        let out = run();
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        if self.mouse {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        self.term.hide_cursor()?;
        // the editor scribbled over every cell we thought we had drawn
        self.term.clear()?;
        Ok(out)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
        let _ = self.term.show_cursor();
    }
}

/// Puts the terminal back to normal. Idempotent and infallible: it is called
/// from the panic hook, where there is nobody to return an error to.
fn restore() {
    let _ = disable_raw_mode();
    // Mouse capture is released first: leaving it on would keep the terminal
    // sending escape sequences at a shell that has no idea what they are.
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
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

fn run(term: &mut TerminalGuard, source: Source) -> io::Result<()> {
    let mut app = App::new(source);
    let mut last_tick = Instant::now();
    let mut last_blink = Instant::now();
    let mut last_anim = Instant::now();
    let mut last_find = Instant::now();

    loop {
        // request whatever the current view needs (non-blocking: it goes to the gh thread)
        app.ensure();
        term.inner().draw(|f| ui::draw(f, &mut app))?;

        // a short wait while requests are in flight, so the response is drawn
        // as soon as it arrives
        let waiting = app.waiting();
        let timeout = TICK
            .saturating_sub(last_tick.elapsed())
            .min(BLINK.saturating_sub(last_blink.elapsed()))
            .min(if waiting {
                ANIM.saturating_sub(last_anim.elapsed())
            } else {
                Duration::MAX
            })
            .min(if app.finder_open {
                FIND.saturating_sub(last_find.elapsed())
            } else {
                Duration::MAX
            })
            .max(Duration::from_millis(16));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                // Moves arrive for every cell the pointer crosses and mean
                // nothing here; dropping them early keeps the loop quiet.
                Event::Mouse(m) if !matches!(m.kind, MouseEventKind::Moved) => app.on_mouse(m),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        while let Some(res) = app.poll_service() {
            app.apply(res);
        }

        // Ratatui writes only the cells that differ from the last frame, which
        // is what makes it quick and also what makes a terminal that got out
        // of step with it stay that way. `clear` forgets what it thought was
        // on screen, so the next frame paints every cell.
        if std::mem::take(&mut app.wants_redraw) {
            term.inner().clear()?;
        }

        // The editor takes the whole terminal, so this happens between frames
        // rather than inside one.
        if let Some((path, line)) = app.edit_request.take() {
            term.suspend(|| edit(&path, line))??;
        }

        if last_tick.elapsed() >= TICK {
            app.tick();
            last_tick = Instant::now();
        }
        if last_find.elapsed() >= FIND {
            app.finder_tick();
            last_find = Instant::now();
        }
        if waiting && last_anim.elapsed() >= ANIM {
            app.anim = app.anim.wrapping_add(1);
            last_anim = Instant::now();
        }
        if last_blink.elapsed() >= BLINK {
            app.blink = !app.blink;
            last_blink = Instant::now();
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
