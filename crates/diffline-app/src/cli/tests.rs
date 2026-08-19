//! The command contract of the `diffline` executable.

use std::ffi::OsString;
use std::path::PathBuf;

use cli_parser::Error;

use super::{Command, RenderArgs, RunArgs, parse_args};

fn parse(args: &[&str]) -> Result<Command, Error> {
    parse_args(args.iter().map(OsString::from))
}

#[test]
fn no_arguments_review_the_current_directory_with_mouse_capture() {
    assert_eq!(
        parse(&[]),
        Ok(Command::Run(RunArgs {
            repository: PathBuf::from("."),
            mouse: true,
            log: None,
        }))
    );
}

#[test]
fn the_log_value_is_not_mistaken_for_the_repository() {
    assert_eq!(
        parse(&["--log", "run.log", "repo", "--no-mouse"]),
        Ok(Command::Run(RunArgs {
            repository: PathBuf::from("repo"),
            mouse: false,
            log: Some(PathBuf::from("run.log")),
        }))
    );
}

#[test]
fn a_renderer_may_follow_the_repository_and_converts_its_values() {
    assert_eq!(
        parse(&["repo", "--svg", "j", "120", "30"]),
        Ok(Command::Render(RenderArgs {
            repository: PathBuf::from("repo"),
            keys: "j".into(),
            width: 120,
            height: 30,
        }))
    );
}

#[test]
fn a_renderer_uses_the_same_defaults_as_the_terminal() {
    assert_eq!(
        parse(&["--svg"]),
        Ok(Command::Render(RenderArgs {
            repository: PathBuf::from("."),
            keys: String::new(),
            width: 160,
            height: 44,
        }))
    );
}

#[test]
fn help_is_generated_and_keeps_the_renderer_private() {
    let Ok(Command::Help(help)) = parse(&["-h"]) else {
        panic!("-h should produce generated help");
    };

    assert!(help.contains("[path]"));
    assert!(help.contains("--no-mouse"));
    assert!(help.contains("--log <file>"));
    assert!(help.contains("-h, --help"));
    assert!(help.contains("-V, --version"));
    assert!(help.contains("V ␣n   select a range"));
    assert!(!help.contains("--svg"));
}

#[test]
fn version_accepts_its_short_alias() {
    assert_eq!(
        parse(&["-V"]),
        Ok(Command::Version("diffline 0.1.0".into()))
    );
}

#[test]
fn invalid_dimensions_are_rejected_instead_of_defaulted() {
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
fn missing_values_unknown_options_and_extra_arguments_are_rejected() {
    assert_eq!(
        parse(&["--log"]),
        Err(Error::MissingValue {
            argument: "--log".into(),
        })
    );
    assert!(matches!(
        parse(&["--unknown"]),
        Err(Error::UnexpectedArgument(_))
    ));
    assert!(matches!(
        parse(&["--svg", "", "160", "44", "extra"]),
        Err(Error::UnexpectedArgument(_))
    ));
}

/// The footer of `--help` names four keys, and it is hand-written: nothing
/// recompiles it when a binding moves. It had drifted a whole keymap — it
/// still advertised `c`, `a` and `S`, none of which are bound at all — and a
/// help text that names keys the program does not have is worse than none.
#[test]
fn the_help_footer_names_keys_that_are_actually_bound() {
    use crate::state::keys::{Action, DEFAULTS};

    let Ok(Command::Help(footer)) = parse(&["--help"]) else {
        panic!("--help no longer prints help");
    };
    for (chord, action) in [
        ("[s", Action::ScopePrev),
        ("]s", Action::ScopeNext),
        ("V", Action::Visual),
        ("<leader>n", Action::Note),
        ("<leader>a", Action::Agents),
        ("<leader>s", Action::Send),
        ("<leader>?", Action::Help),
    ] {
        assert!(
            DEFAULTS.contains(&(chord, action)),
            "{chord} is no longer bound to {action:?}, and --help still says it is"
        );
        // `<leader>x` is printed as `␣x`, and a bare chord as itself.
        let shown = chord.replace("<leader>", "␣");
        assert!(
            footer.contains(&shown),
            "--help no longer shows {shown}, which is what {action:?} is on"
        );
    }
}
