//! Filesystem change notices for a repository.
//!
//! The watcher says only that something may have changed. Git remains the
//! authority on whether the active diff actually did, which keeps editor
//! temporary files and ignored build output from becoming application state.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError, channel, sync_channel};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// One observation made by the filesystem watcher.
#[derive(Debug)]
pub enum Notice {
    /// At least one path may have changed.
    Changed,
    /// The backend reported a failure after it had started.
    Failed(notify::Error),
    /// The backend stopped without reporting why.
    Gone,
    /// Nothing has arrived since the previous poll.
    Quiet,
}

/// A repository watcher whose change channel coalesces bursts.
///
/// Editors commonly save by writing, renaming and removing several files.
/// The one-place channel turns that burst into one refresh while the separate
/// error channel keeps a useful failure from being dropped behind it.
#[derive(Debug)]
pub struct Watch {
    _watcher: RecommendedWatcher,
    changes: Receiver<()>,
    errors: Receiver<notify::Error>,
}

impl Watch {
    /// Starts watching `repository` recursively using the platform backend.
    pub fn start(repository: &Path) -> notify::Result<Self> {
        let (change_tx, changes) = sync_channel(1);
        let (error_tx, errors) = channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                match result {
                    // Merely reading the repository must not make Git read it
                    // again, especially on backends that report access events.
                    Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                        let _ = change_tx.try_send(());
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = error_tx.send(error);
                    }
                }
            })?;
        watcher.watch(repository, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            changes,
            errors,
        })
    }

    /// Takes the next failure or coalesced change without blocking the UI.
    pub fn poll(&self) -> Notice {
        match self.errors.try_recv() {
            Ok(error) => return Notice::Failed(error),
            Err(TryRecvError::Disconnected) => return Notice::Gone,
            Err(TryRecvError::Empty) => {}
        }
        match self.changes.try_recv() {
            Ok(()) => Notice::Changed,
            Err(TryRecvError::Empty) => Notice::Quiet,
            Err(TryRecvError::Disconnected) => Notice::Gone,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use super::*;

    #[test]
    fn writing_a_file_produces_one_coalesced_change() {
        let tag = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("diffline-watch-{tag}"));
        std::fs::create_dir(&root).unwrap();
        let watch = Watch::start(&root).unwrap();

        std::fs::write(root.join("changed.rs"), "fn changed() {}\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let changed = loop {
            match watch.poll() {
                Notice::Changed => break true,
                Notice::Failed(error) => panic!("watch failed: {error}"),
                Notice::Gone => panic!("watch stopped"),
                Notice::Quiet if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Notice::Quiet => break false,
            }
        };

        drop(watch);
        std::fs::remove_dir_all(root).unwrap();
        assert!(changed, "the write should reach the watcher");
    }
}
