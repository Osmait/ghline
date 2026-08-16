//! Schema parsing, built-in responses and generated help stay in step.

use std::ffi::OsString;

use cli_parser::{ArgSpec, Cli, Error, Outcome, ValueSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    Quiet,
    Output,
    Snapshot,
    Input,
}

fn cli() -> Cli<Id> {
    Cli::new("tool")
        .about("does one thing")
        .version("1.2.3")
        .arg(
            ArgSpec::flag(Id::Quiet, "quiet")
                .short('q')
                .help("say nothing"),
        )
        .arg(
            ArgSpec::option(Id::Output, "output", "file")
                .short('o')
                .help("write here"),
        )
        .arg(
            ArgSpec::mode(
                Id::Snapshot,
                "snapshot",
                [ValueSpec::required("keys"), ValueSpec::optional("width")],
            )
            .hidden(),
        )
        .arg(ArgSpec::positional(Id::Input, ValueSpec::optional("input")).help("read this file"))
        .after_help("Press ? once inside.")
}

fn parse(args: &[&str]) -> Result<Outcome<Id>, Error> {
    cli().parse(args.iter().map(OsString::from))
}

#[test]
fn one_schema_parses_flags_options_and_positionals_into_typed_ids() {
    let Ok(Outcome::Matches(matches)) = parse(&["-q", "--output=result", "source"]) else {
        panic!("the command line should produce matches");
    };

    assert!(matches.contains(&Id::Quiet));
    assert_eq!(matches.value(&Id::Output), Some("result".as_ref()));
    assert_eq!(matches.value(&Id::Input), Some("source".as_ref()));
}

#[test]
fn help_and_version_are_built_in_without_application_ids() {
    assert_eq!(
        parse(&["--version"]),
        Ok(Outcome::Version("tool 1.2.3".into()))
    );
    assert!(matches!(parse(&["-h"]), Ok(Outcome::Help(_))));
}

#[test]
fn help_is_generated_from_visible_definitions_only() {
    let help = cli().help();

    assert_eq!(
        help,
        concat!(
            "tool — does one thing\n",
            "\n",
            "Usage:\n",
            "  tool [options] [input]\n",
            "\n",
            "Options:\n",
            "  -q, --quiet          say nothing\n",
            "  -o, --output <file>  write here\n",
            "  -h, --help           print help and exit\n",
            "  -V, --version        print version and exit\n",
            "\n",
            "Arguments:\n",
            "  [input]  read this file\n",
            "\n",
            "Press ? once inside."
        )
    );
    assert!(!help.contains("snapshot"));
}

#[test]
fn a_mode_uses_its_own_positional_schema() {
    let Ok(Outcome::Matches(matches)) = parse(&["--snapshot", "keys", "120"]) else {
        panic!("the mode should produce matches");
    };

    assert_eq!(
        matches.values(&Id::Snapshot),
        Some([OsString::from("keys"), OsString::from("120")].as_slice())
    );
}

#[test]
fn required_mode_values_and_extra_values_are_rejected() {
    assert_eq!(
        parse(&["--snapshot"]),
        Err(Error::MissingValue {
            argument: "keys".into(),
        })
    );
    assert!(matches!(
        parse(&["--snapshot", "keys", "120", "extra"]),
        Err(Error::UnexpectedArgument(_))
    ));
}

#[test]
fn a_schema_cannot_put_a_required_value_after_an_optional_one() {
    let schema = Cli::new("tool").arg(ArgSpec::mode(
        Id::Snapshot,
        "snapshot",
        [ValueSpec::optional("first"), ValueSpec::required("second")],
    ));

    assert!(matches!(
        schema.parse([]),
        Err(Error::InvalidDefinition { .. })
    ));
}
