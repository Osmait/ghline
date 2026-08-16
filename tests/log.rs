//! What `--log` writes, and that it plays back.
//!
//! Its own test binary because the log is a process-wide `OnceLock`: opening
//! one inside the library's suite would decide for every other test in it.
//! Here there is one test and nothing else to disturb.

use std::io;

use github_tui::shared::key::{Button, Key, Motion, Mouse, Press, parse_keys};
use github_tui::shared::log;

/// The promise the feature is for: the last line of the file is a command,
/// and running it replays the session that was recorded.
#[test]
fn the_last_line_replays_the_session() -> io::Result<()> {
    let path = std::env::temp_dir().join(format!("diffline-log-{}.log", std::process::id()));
    log::to(&path, "diffline")?;

    // A session with the awkward presses in it: a chord, a named key, and the
    // two characters the notation itself is made of.
    let session = [
        Press::new(Key::Char('j')),
        Press::new(Key::Char('/')),
        Press::new(Key::Char('<')),
        Press::ctrl(Key::Char('c')),
        Press::new(Key::Enter),
        Press::new(Key::PageDown),
    ];
    for p in session {
        log::key(p);
    }
    log::mouse(Mouse {
        col: 12,
        row: 4,
        what: Motion::Down(Button::Left),
    });
    log::finish();

    let written = std::fs::read_to_string(&path)?;
    let _ = std::fs::remove_file(&path);

    let last = written.lines().last().unwrap_or_default();
    let keys = last
        .split_once('"')
        .and_then(|(_, rest)| rest.rsplit_once('"'))
        .map(|(k, _)| k)
        .unwrap_or_default();

    assert_eq!(
        parse_keys(keys),
        session.to_vec(),
        "the replay line says {keys:?}, which is not what was pressed",
    );

    // And the rest of it is readable by a person, which is the other half of
    // why it exists.
    assert!(written.starts_with("+"), "every line is stamped: {written}");
    assert!(
        written.contains("mouse down Left at 12,4"),
        "the click is not in it: {written}",
    );
    assert!(
        written.contains("key <c-c>"),
        "the chord is not in it: {written}",
    );
    Ok(())
}
