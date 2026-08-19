//! The command line of `diffline`, before a repository or worker is opened.
//!
//! `cli-parser` owns syntax, validation and help generation. This module owns
//! the IDs, defaults and command model specific to reviewing a diff.

use std::ffi::OsString;
use std::path::PathBuf;

use cli_parser::{ArgSpec, Cli, Error, Matches, Outcome, ValueSpec, value_as, value_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    NoMouse,
    Log,
    Svg,
    Repository,
}

/// What `diffline` should do after its command line has been parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Review a repository in the terminal.
    Run(RunArgs),
    /// Render one repository frame as SVG.
    Render(RenderArgs),
    /// Print generated usage and stop.
    Help(String),
    /// Print the generated package version and stop.
    Version(String),
}

/// Options used by the interactive reviewer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunArgs {
    /// Repository to review, defaulting to the current directory.
    pub repository: PathBuf,
    /// Whether the application should capture mouse events.
    pub mouse: bool,
    /// The optional session log destination.
    pub log: Option<PathBuf>,
}

/// Arguments used to draw one repository frame without taking the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderArgs {
    /// Repository whose diff should be drawn.
    pub repository: PathBuf,
    /// Keys to replay before drawing.
    pub keys: String,
    /// Terminal width in cells.
    pub width: u16,
    /// Terminal height in cells.
    pub height: u16,
}

/// Parses `diffline` arguments without reading a repository.
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, Error> {
    match definition().parse(args)? {
        Outcome::Matches(matches) => command(&matches),
        Outcome::Help(help) => Ok(Command::Help(help)),
        Outcome::Version(version) => Ok(Command::Version(version)),
    }
}

fn definition() -> Cli<Id> {
    Cli::new("diffline")
        .about("review a diff, and hand notes to a coding agent")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            ArgSpec::flag(Id::NoMouse, "no-mouse")
                .help("leave the terminal's own click-to-select alone"),
        )
        .arg(
            ArgSpec::option(Id::Log, "log", "file")
                .help("record the session; the last line replays it"),
        )
        .arg(
            ArgSpec::mode(
                Id::Svg,
                "svg",
                [
                    ValueSpec::optional("keys"),
                    ValueSpec::optional("width"),
                    ValueSpec::optional("height"),
                ],
            )
            .hidden(),
        )
        .arg(
            ArgSpec::positional(Id::Repository, ValueSpec::optional("path"))
                .help("repository to read; default the current directory"),
        )
        .after_help(
            "[s ]s  working tree · this branch · the last commit\n\
             V ␣n   select a range, note on it\n\
             ␣a ␣s  pick an agent, send the queue\n\
             ␣?     everything else · ␣ is the leader",
        )
}

fn command(matches: &Matches<Id>) -> Result<Command, Error> {
    let repository = matches
        .value(&Id::Repository)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    if let Some(values) = matches.values(&Id::Svg) {
        let keys = values
            .first()
            .map(|value| value_text(value, "keys"))
            .transpose()?
            .unwrap_or_default();
        let width = values
            .get(1)
            .map(|value| value_as(value, "width"))
            .transpose()?
            .unwrap_or(160);
        let height = values
            .get(2)
            .map(|value| value_as(value, "height"))
            .transpose()?
            .unwrap_or(44);
        return Ok(Command::Render(RenderArgs {
            repository,
            keys,
            width,
            height,
        }));
    }

    Ok(Command::Run(RunArgs {
        repository,
        mouse: !matches.contains(&Id::NoMouse),
        log: matches.value(&Id::Log).map(PathBuf::from),
    }))
}

#[cfg(test)]
mod tests;
