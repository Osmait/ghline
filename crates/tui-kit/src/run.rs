//! Holding the terminal, and turning it over to a program.
//!
//! Both binaries did this themselves and had drifted: `restore` existed
//! twice, doing the same thing two different ways, and only one of the copies
//! carried the comment explaining why the mouse must be released before the
//! alternate screen — which is the part you would want if you were reading it
//! to change it.
//!
//! ## Why this one is a trait
//!
//! Everywhere else in this crate a component takes data and hands back
//! geometry, because the caller always knows what it is drawing. Here it is
//! the other way round: the runtime owns the loop and holds a program it did
//! not write and cannot name. That is the case a trait is for, and it is the
//! first one in the crate that has actually turned up.
//!
//! What the runtime keeps is what does not vary: entering and leaving the
//! alternate screen, restoring on a panic, reading events and throwing away
//! the ones nobody wants, clearing when the terminal has got out of step, and
//! never sleeping so long that the answer to a keystroke arrives late. What
//! the program keeps is its own timers — a blink, a skeleton, a debounce —
//! because those are about what it draws, and a runtime that tried to own
//! them would need to be told about each one anyway.

use std::io;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::key::{Button, Key, Motion, Mouse, Press};

/// The floor on how long the loop may wait.
///
/// Not a frame rate: nothing here animates on its own. It is the longest a
/// keystroke may sit unread, and sixteen milliseconds is under what anybody
/// notices while being long enough that an idle program is not spinning.
const FLOOR: Duration = Duration::from_millis(16);

/// Turns a terminal's idea of a keystroke into this program's.
///
/// The only place either program needs to know how crossterm spells a key.
fn press(ev: crossterm::event::KeyEvent) -> Press {
    use crossterm::event::{KeyCode, KeyModifiers};
    let key = match ev.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        _ => Key::Other,
    };
    Press {
        key,
        ctrl: ev.modifiers.contains(KeyModifiers::CONTROL),
        alt: ev.modifiers.contains(KeyModifiers::ALT),
    }
}

/// The same for the mouse.
fn mouse(ev: crossterm::event::MouseEvent) -> Mouse {
    use crossterm::event::MouseButton;
    let button = |b| match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    };
    let what = match ev.kind {
        MouseEventKind::Down(b) => Motion::Down(button(b)),
        MouseEventKind::Up(b) => Motion::Up(button(b)),
        MouseEventKind::Drag(b) => Motion::Drag(button(b)),
        MouseEventKind::ScrollUp => Motion::ScrollUp,
        MouseEventKind::ScrollDown => Motion::ScrollDown,
        _ => Motion::Moved,
    };
    Mouse {
        col: ev.column,
        row: ev.row,
        what,
    }
}

/// Something the runtime can run.
pub trait Program {
    /// Ask for whatever the current view needs. Must not block: the whole
    /// point of the worker thread is that this returns immediately.
    fn ensure(&mut self) {}

    /// Paints the current state, once per pass of the loop.
    ///
    /// `&mut self` rather than `&self` because a pane's height is not known
    /// until it is being drawn into, and that is where the scroll offset is
    /// settled against it. The alternative was a layout pass computing every
    /// rectangle a second time to keep this method honest.
    fn draw(&mut self, f: &mut Frame<'_>);

    /// A keystroke, already normalised.
    ///
    /// Only presses arrive: a terminal that also reports releases would
    /// otherwise act on every key twice. It comes as this crate's `Press`
    /// rather than crossterm's event, so a program never names the backend.
    fn on_key(&mut self, press: Press);

    /// A click, a scroll or a drag.
    ///
    /// Never a pointer merely crossing the screen — motion is dropped in the
    /// loop before it can cost a frame. Empty by default: a program that does
    /// not want the mouse says so by not implementing this, and whether the
    /// events are captured at all is `Terminal_::enter`'s argument.
    fn on_mouse(&mut self, _mouse: Mouse) {}

    /// Take whatever the worker has answered since the last pass.
    fn drain(&mut self) {}

    /// How long the loop may sleep before this program wants waking.
    ///
    /// The program keeps its own timers; this is only the soonest of them.
    /// `Duration::MAX` means "only when something happens".
    fn next_wake(&self) -> Duration {
        Duration::MAX
    }

    /// Whatever fell due while it slept.
    fn on_wake(&mut self) {}

    /// True when the terminal has got out of step and the next frame should
    /// paint every cell rather than only what changed.
    fn wants_redraw(&mut self) -> bool {
        false
    }

    /// Something that wants the whole terminal — an editor.
    ///
    /// Taken rather than borrowed, so it happens once. The runtime gives the
    /// terminal back, runs it, and takes the terminal again afterwards; a
    /// program that has no such thing never implements this.
    fn take_handover(&mut self) -> Option<Handover> {
        None
    }

    /// Asked at the end of every pass, once the event and whatever fell due
    /// have both been handled — so a program can decide to quit inside
    /// `on_key` and still finish the pass it is in.
    fn should_quit(&self) -> bool;
}

/// A program that wants the terminal to itself for a moment.
pub type Handover = Box<dyn FnOnce() -> io::Result<()>>;

/// Holds the terminal in the alternate screen and gives it back on drop, even
/// through a panic. Without it a panic leaves the console in raw mode with no
/// echo, which is a working shell you cannot see yourself typing into.
pub struct Terminal_ {
    term: Terminal<CrosstermBackend<io::Stdout>>,
    /// Whether the mouse was captured, so a handover puts back what was
    /// there rather than whatever the default happens to be.
    mouse: bool,
}

impl Terminal_ {
    /// Takes the terminal: raw mode, the alternate screen, and the mouse if
    /// `mouse` is set.
    ///
    /// Installs a panic hook that restores it before the message is printed,
    /// so a panic lands in a console you can still read. On failure it puts
    /// back whatever it managed to set — half-entered is worse than not
    /// entered, because the shell underneath is the one left broken.
    pub fn enter(mouse: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let entered = if mouse {
            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
        } else {
            execute!(io::stdout(), EnterAlternateScreen)
        };
        if let Err(e) = entered {
            // half-entered is worse than not entered: put the terminal back
            let _ = disable_raw_mode();
            return Err(e);
        }
        // A panic only skips `Drop` when it aborts; on unwind this leaves the
        // terminal usable before the message is printed into it.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));

        let mut term = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        term.hide_cursor()?;
        Ok(Self { term, mouse })
    }

    /// Hands the terminal to something else and takes it back afterwards.
    ///
    /// An editor wants the real terminal: its own screen, its own raw mode,
    /// its own mouse handling. Anything less and it draws into ours. The
    /// taking back is unconditional, so an editor that dies badly still
    /// leaves a terminal that works.
    fn handover<T>(&mut self, run: impl FnOnce() -> T) -> io::Result<T> {
        restore();
        let out = run();
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        if self.mouse {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        self.term.hide_cursor()?;
        // whatever it was scribbled over every cell we thought we had drawn
        self.term.clear()?;
        Ok(out)
    }
}

impl Drop for Terminal_ {
    fn drop(&mut self) {
        restore();
        let _ = self.term.show_cursor();
    }
}

/// Puts the terminal back to normal.
///
/// Idempotent and infallible: it is called from the panic hook, where there is
/// nobody to return an error to. Mouse capture is released before the
/// alternate screen goes, because leaving it on would keep the terminal
/// sending escape sequences at a shell that has no idea what they are.
pub fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
}

/// Runs `program` until it says to stop.
///
/// One pass, in the order it happens. The left column is the loop; the right
/// is what the program is asked, and everything the program does is inside
/// one of those calls:
///
/// ```text
///   ┌─► ensure()          "ask for whatever this view needs" — must not
///   │                      block, so it only posts to the worker thread
///   │
///   │   draw()             the whole frame, from state as it stands
///   │
///   │   poll(timeout)      timeout = next_wake(), floored at 16ms
///   │     ├── key ───────► on_key(Press)
///   │     ├── click ─────► on_mouse(Mouse)
///   │     └── nothing      the program's own timer fell due
///   │
///   │   drain()            take whatever the worker answered meanwhile
///   │
///   │   wants_redraw()     true after something else wrote to the terminal;
///   │                      clears so the next frame paints every cell
///   │
///   │   take_handover()    an editor wants the real terminal — done between
///   │                      frames, never inside one
///   │
///   └── on_wake()          whatever fell due while it slept
///       should_quit()
/// ```
///
/// The order is the argument. `ensure` comes before `draw` so a view that has
/// just been opened has already asked for its data by the time it is first
/// drawn — one pass later and every view would flash empty before its skeleton
/// appeared. `drain` comes after `poll` so an answer that landed during the
/// wait is applied before the loop comes back round to `draw`, which is the
/// very next thing it does.
pub fn run(term: &mut Terminal_, program: &mut impl Program) -> io::Result<()> {
    loop {
        program.ensure();
        term.term.draw(|f| program.draw(f))?;

        // Never longer than the program asked for, never shorter than the
        // floor: a zero here would spin the CPU on a program with a timer
        // that has already fallen due.
        let timeout = program.next_wake().max(FLOOR);
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    program.on_key(press(key));
                }
                // Motion arrives for every cell the pointer crosses whether
                // or not anything is pressed, and nothing here follows a
                // pointer. Dropped before it can cost a frame.
                Event::Mouse(event) if event.kind != MouseEventKind::Moved => {
                    program.on_mouse(mouse(event));
                }
                _ => {}
            }
        }

        program.drain();

        // Ratatui writes only the cells that differ from the last frame,
        // which is what makes it quick and also what makes a terminal that
        // got out of step with it stay that way. Clearing forgets what it
        // thought was on screen, so the next frame paints every cell.
        if program.wants_redraw() {
            term.term.clear()?;
        }

        // Between frames rather than inside one: whatever this is wants the
        // terminal, and we are in the middle of drawing to it.
        if let Some(job) = program.take_handover() {
            term.handover(job)??;
        }

        program.on_wake();

        if program.should_quit() {
            return Ok(());
        }
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

    /// A program that quits at once, to exercise the trait's defaults.
    struct Nothing;

    impl Program for Nothing {
        fn draw(&mut self, _f: &mut Frame<'_>) {}
        fn on_key(&mut self, _press: Press) {}
        fn should_quit(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_program_needs_three_methods_and_no_more() {
        // `Nothing` above is everything a program has to write. Every other
        // method has a default, because a program without a mouse, without a
        // worker and without timers is still a program.
        let mut p = Nothing;
        assert!(p.should_quit());
        assert_eq!(
            p.next_wake(),
            Duration::MAX,
            "wake only when something happens"
        );
        assert!(!p.wants_redraw());
        assert!(p.take_handover().is_none());
    }

    /// One that asks to be woken sooner than the floor.
    struct Impatient;

    impl Program for Impatient {
        fn draw(&mut self, _f: &mut Frame<'_>) {}
        fn on_key(&mut self, _press: Press) {}
        fn next_wake(&self) -> Duration {
            Duration::ZERO
        }
        fn should_quit(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_timer_that_has_already_fallen_due_does_not_spin_the_loop() {
        // A program whose timer is overdue asks for zero, and a zero timeout
        // is a busy wait. The floor is what stops that being a hot CPU.
        let p = Impatient;
        assert_eq!(p.next_wake().max(FLOOR), FLOOR);
    }

    #[test]
    fn the_floor_does_not_cap_a_long_wait() {
        let p = Nothing;
        assert_eq!(p.next_wake().max(FLOOR), Duration::MAX);
    }
}
