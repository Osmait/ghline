//! The command contract of the `github-tui` executable.

use std::ffi::OsString;
use std::path::PathBuf;

use cli_parser::Error;

use super::{Command, RenderArgs, RenderKind, RunArgs, parse_args};

fn parse(args: &[&str]) -> Result<Command, Error> {
    parse_args(args.iter().map(OsString::from))
}

#[test]
fn no_arguments_start_the_interactive_reader_with_mouse_capture() {
    assert_eq!(
        parse(&[]),
        Ok(Command::Run(RunArgs {
            mouse: true,
            log: None,
        }))
    );
}

#[test]
fn interactive_options_are_independent_of_their_order() {
    assert_eq!(
        parse(&["--log", "run.log", "--no-mouse"]),
        Ok(Command::Run(RunArgs {
            mouse: false,
            log: Some(PathBuf::from("run.log")),
        }))
    );
}

#[test]
fn an_inline_log_value_is_accepted() {
    assert_eq!(
        parse(&["--log=run.log"]),
        Ok(Command::Run(RunArgs {
            mouse: true,
            log: Some(PathBuf::from("run.log")),
        }))
    );
}

#[test]
fn help_and_version_accept_their_short_aliases() {
    let Ok(Command::Help(help)) = parse(&["-h"]) else {
        panic!("-h should produce generated help");
    };
    assert!(help.contains("--no-mouse"));
    assert!(help.contains("--log <file>"));
    assert!(help.contains("-h, --help"));
    assert!(help.contains("-V, --version"));
    assert!(!help.contains("--snapshot"));
    assert!(!help.contains("--svg"));

    assert_eq!(
        parse(&["-V"]),
        Ok(Command::Version("github-tui 0.1.0".into()))
    );
}

#[test]
fn every_renderer_has_the_same_defaults() {
    for (argument, kind) in [
        ("--snapshot", RenderKind::Snapshot),
        ("--svg", RenderKind::Svg),
        ("--svg-loading", RenderKind::SvgLoading),
        ("--svg-live", RenderKind::SvgLive),
    ] {
        assert_eq!(
            parse(&[argument]),
            Ok(Command::Render(RenderArgs {
                kind,
                keys: String::new(),
                width: 160,
                height: 44,
                ticks: 0,
            }))
        );
    }
}

#[test]
fn a_renderer_converts_each_positional_value() {
    assert_eq!(
        parse(&["--svg", "j<enter>", "120", "30", "4"]),
        Ok(Command::Render(RenderArgs {
            kind: RenderKind::Svg,
            keys: "j<enter>".into(),
            width: 120,
            height: 30,
            ticks: 4,
        }))
    );
}

#[test]
fn a_renderer_accepts_its_keys_as_an_inline_value() {
    assert_eq!(
        parse(&["--svg=j<enter>", "120", "30", "4"]),
        Ok(Command::Render(RenderArgs {
            kind: RenderKind::Svg,
            keys: "j<enter>".into(),
            width: 120,
            height: 30,
            ticks: 4,
        }))
    );
}

#[test]
fn an_invalid_dimension_names_the_field_and_value() {
    assert_eq!(
        parse(&["--svg", "", "wide"]),
        Err(Error::InvalidValue {
            argument: "width".into(),
            value: "wide".into(),
            expected: "u16",
        })
    );
}

#[test]
fn a_log_without_a_path_is_rejected() {
    assert_eq!(
        parse(&["--log"]),
        Err(Error::MissingValue {
            argument: "--log".into(),
        })
    );
}

#[test]
fn duplicate_log_destinations_are_rejected() {
    assert_eq!(
        parse(&["--log", "one.log", "--log", "two.log"]),
        Err(Error::duplicate("--log"))
    );
}

#[test]
fn unknown_options_and_extra_renderer_values_are_rejected() {
    assert!(matches!(
        parse(&["--unknown"]),
        Err(Error::UnexpectedArgument(_))
    ));
    assert!(matches!(
        parse(&["--svg", "", "160", "44", "0", "extra"]),
        Err(Error::UnexpectedArgument(_))
    ));
}
