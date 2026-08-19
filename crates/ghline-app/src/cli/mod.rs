//! The command line of `ghline`, separate from starting either renderer.
//!
//! `cli-parser` owns syntax, validation and help generation. This module owns
//! only the IDs, defaults and command model that belong to this program.

use std::ffi::OsString;
use std::path::PathBuf;

use cli_parser::{ArgSpec, Cli, Error, Matches, Outcome, ValueSpec, value_as, value_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    NoMouse,
    Log,
    Snapshot,
    Svg,
    SvgLoading,
    SvgLive,
}

/// What `ghline` should do after its command line has been parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start the interactive GitHub reader.
    Run(RunArgs),
    /// Draw a deterministic or live frame without taking the terminal.
    Render(RenderArgs),
    /// Print generated usage and stop.
    Help(String),
    /// Print the generated package version and stop.
    Version(String),
}

/// Options used by the interactive reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunArgs {
    /// Whether the application should capture mouse events.
    pub mouse: bool,
    /// The optional session log destination.
    pub log: Option<PathBuf>,
}

/// One of the four headless renderers exposed for snapshots and session logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderKind {
    /// ANSI text backed by the deterministic fixture.
    Snapshot,
    /// SVG backed by the deterministic fixture.
    Svg,
    /// SVG showing a chosen loading animation frame.
    SvgLoading,
    /// SVG backed by real GitHub data.
    SvgLive,
}

/// Arguments shared by the headless renderers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderArgs {
    /// Which renderer receives the remaining values.
    pub kind: RenderKind,
    /// Keys to replay before drawing.
    pub keys: String,
    /// Terminal width in cells.
    pub width: u16,
    /// Terminal height in cells.
    pub height: u16,
    /// Settling ticks or, for loading SVGs, the animation frame.
    pub ticks: usize,
}

/// Parses `ghline` arguments without starting any application service.
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, Error> {
    match definition().parse(args)? {
        Outcome::Matches(matches) => command(&matches),
        Outcome::Help(help) => Ok(Command::Help(help)),
        Outcome::Version(version) => Ok(Command::Version(version)),
    }
}

fn definition() -> Cli<Id> {
    let render_values = || {
        [
            ValueSpec::optional("keys"),
            ValueSpec::optional("width"),
            ValueSpec::optional("height"),
            ValueSpec::optional("ticks"),
        ]
    };

    Cli::new("ghline")
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            ArgSpec::flag(Id::NoMouse, "no-mouse")
                .help("leave the terminal's own click-to-select alone"),
        )
        .arg(
            ArgSpec::option(Id::Log, "log", "file")
                .help("record the session; the last line replays it"),
        )
        .arg(ArgSpec::mode(Id::Snapshot, "snapshot", render_values()).hidden())
        .arg(ArgSpec::mode(Id::Svg, "svg", render_values()).hidden())
        .arg(ArgSpec::mode(Id::SvgLoading, "svg-loading", render_values()).hidden())
        .arg(ArgSpec::mode(Id::SvgLive, "svg-live", render_values()).hidden())
        .after_help("?  every key, once you are inside")
}

fn command(matches: &Matches<Id>) -> Result<Command, Error> {
    if let Some(values) = matches.values(&Id::Snapshot) {
        return parse_render(RenderKind::Snapshot, values);
    }
    if let Some(values) = matches.values(&Id::Svg) {
        return parse_render(RenderKind::Svg, values);
    }
    if let Some(values) = matches.values(&Id::SvgLoading) {
        return parse_render(RenderKind::SvgLoading, values);
    }
    if let Some(values) = matches.values(&Id::SvgLive) {
        return parse_render(RenderKind::SvgLive, values);
    }

    Ok(Command::Run(RunArgs {
        mouse: !matches.contains(&Id::NoMouse),
        log: matches.value(&Id::Log).map(PathBuf::from),
    }))
}

fn parse_render(kind: RenderKind, values: &[OsString]) -> Result<Command, Error> {
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
    let ticks = values
        .get(3)
        .map(|value| value_as(value, "ticks"))
        .transpose()?
        .unwrap_or(0);

    Ok(Command::Render(RenderArgs {
        kind,
        keys,
        width,
        height,
        ticks,
    }))
}

#[cfg(test)]
mod tests;
