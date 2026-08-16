//! A file to say what happened, because the screen is taken.
//!
//! Every other program can print a line when something goes wrong. This one
//! is holding the terminal: `println!` lands in the middle of a drawn frame
//! and is gone at the next redraw, which is why `print_stdout` is a lint here
//! and why a bug report has so far been whatever the reader remembered doing.
//!
//! `--log <file>` is the answer to that, and what it writes is chosen for one
//! purpose: to be replayable. Keystrokes are recorded in the notation
//! `parse_keys` reads, and the last line of the file is the `--snapshot`
//! command that plays the session back into a frame. What a reader sends is
//! then not a description of the bug — it is the bug.
//!
//! No logging crate. `log` and `tracing` are levels, targets, filters and
//! subscribers, and what is wanted here is a file with lines in it: one
//! writer, one format, off unless asked for. The whole thing is a `OnceLock`
//! and a `Mutex`.
//!
//! Off costs an atomic load. `say` takes `fmt::Arguments`, so a message that
//! is never written is also never formatted.

use std::fmt;
use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::shared::key::{Motion, Mouse, Press};

struct Sink {
    file: Mutex<File>,
    /// Everything is stamped as milliseconds from here rather than as a wall
    /// clock. What a log of a terminal session is read for is the gap between
    /// two lines — the keystroke that took a second and a half to answer —
    /// and a date on every line is eleven columns of the same eleven columns.
    start: Instant,
    /// The session so far, in `parse_keys` notation.
    keys: Mutex<String>,
    /// Which of the two programs this is. `CARGO_PKG_NAME` is the crate's
    /// name and the crate builds both, so a diffline log headed `github-tui`
    /// would be the first line of the file being wrong.
    program: &'static str,
}

static SINK: OnceLock<Sink> = OnceLock::new();

/// Starts logging to `path`. Called once, by a binary reading its arguments.
///
/// Truncates: a log from the run before is a log of a different bug, and
/// appending would leave the reader to find where one ended.
pub fn to(path: &Path, program: &'static str) -> std::io::Result<()> {
    let file = File::create(path)?;
    // A second call is not an error and is not a second log — whichever
    // opened first keeps the file.
    let _ = SINK.set(Sink {
        file: Mutex::new(file),
        start: Instant::now(),
        keys: Mutex::new(String::new()),
        program,
    });
    let unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    say(format_args!(
        "{program} {} — unix {unix}",
        env!("CARGO_PKG_VERSION"),
    ));
    Ok(())
}

/// Whether anything is being written, for a caller with a message that costs
/// something to build.
pub fn on() -> bool {
    SINK.get().is_some()
}

/// One line, stamped.
///
/// Takes `fmt::Arguments` — write `say(format_args!("…"))` — so that nothing
/// is formatted, and nothing allocated, when no log is open.
///
/// A write that fails is dropped. The alternative is a program that stops
/// drawing because its log is on a full disk.
pub fn say(args: fmt::Arguments<'_>) {
    let Some(sink) = SINK.get() else { return };
    let Ok(mut file) = sink.file.lock() else {
        return;
    };
    let _ = writeln!(file, "+{:>7}ms {args}", sink.start.elapsed().as_millis());
}

/// A keystroke, both as a line and as one more character of the replay.
pub fn key(press: Press) {
    let Some(sink) = SINK.get() else { return };
    let spelt = press.spell();
    if let Ok(mut keys) = sink.keys.lock() {
        keys.push_str(&spelt);
    }
    say(format_args!("key {spelt}"));
}

/// A click, a drag or a scroll, with where it landed.
///
/// Not part of the replay: `--snapshot` takes keys and nothing else, and a
/// mouse position means nothing without the layout that was on screen when it
/// happened. It is here because "the pane I clicked" is most of what a report
/// says.
pub fn mouse(m: Mouse) {
    if !on() {
        return;
    }
    let what = match m.what {
        Motion::Down(b) => format!("down {b:?}"),
        Motion::Up(b) => format!("up {b:?}"),
        Motion::Drag(b) => format!("drag {b:?}"),
        Motion::ScrollUp => "scroll up".into(),
        Motion::ScrollDown => "scroll down".into(),
        Motion::Moved => return,
    };
    say(format_args!("mouse {what} at {},{}", m.col, m.row));
}

/// The last line: the command that plays this session back.
///
/// Written where the loop ends rather than from a `Drop`, because the thing
/// that would carry the `Drop` is a static and statics are not dropped.
pub fn finish() {
    let Some(sink) = SINK.get() else { return };
    let keys = sink.keys.lock().map(|k| k.clone()).unwrap_or_default();
    say(format_args!(
        "replay: {} --snapshot \"{keys}\" 160 44",
        sink.program,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::key::Key;

    /// The static is a static: a test that opened a log would decide for every
    /// other test in the binary. What can be checked without it is that the
    /// functions are safe to call with nothing open, which is the state they
    /// are in for every run that did not ask for a log — including every run
    /// of this suite.
    #[test]
    fn nothing_is_written_and_nothing_panics_when_no_log_is_open() {
        assert!(!on(), "no test opens one");
        say(format_args!("into the void"));
        key(Press::new(Key::Char('j')));
        mouse(Mouse {
            col: 1,
            row: 1,
            what: Motion::ScrollUp,
        });
        finish();
    }
}
