//! The syntax accepted by the application-independent argument parser.

use std::ffi::{OsStr, OsString};

use cli_parser::{Arg, Error, Parser, value_as, value_text};

fn parser(args: &[&str]) -> Parser {
    Parser::new(args.iter().map(OsString::from))
}

#[test]
fn long_options_keep_an_inline_value_separate_from_the_name() {
    let mut args = parser(&["--log=run.log"]);

    assert_eq!(
        args.next_arg(),
        Ok(Some(Arg::Long {
            name: "log".into(),
            value: Some("run.log".into()),
        }))
    );
    assert_eq!(args.next_arg(), Ok(None));
}

#[test]
fn short_option_groups_are_left_for_the_application_to_interpret() {
    let mut args = parser(&["-abc"]);

    assert_eq!(args.next_arg(), Ok(Some(Arg::Short("abc".into()))));
}

#[test]
fn the_separator_makes_every_remaining_argument_a_value() {
    let mut args = parser(&["--", "--help", "-V"]);

    assert_eq!(args.next_arg(), Ok(Some(Arg::Value("--help".into()))));
    assert_eq!(args.next_arg(), Ok(Some(Arg::Value("-V".into()))));
}

#[test]
fn a_single_dash_is_a_positional_value() {
    let mut args = parser(&["-"]);

    assert_eq!(args.next_arg(), Ok(Some(Arg::Value("-".into()))));
}

#[test]
fn a_separate_option_value_may_begin_with_a_dash() {
    let mut args = parser(&["--log", "--literal-file-name"]);
    let Some(Arg::Long { name, value }) = args.next_arg().unwrap() else {
        panic!("the first argument should be a long option");
    };

    assert_eq!(
        args.value(format!("--{name}"), value),
        Ok(OsString::from("--literal-file-name"))
    );
    assert_eq!(args.next_arg(), Ok(None));
}

#[test]
fn a_required_value_reports_which_argument_is_missing() {
    let mut args = parser(&[]);

    assert_eq!(
        args.value("--log", None),
        Err(Error::MissingValue {
            argument: "--log".into(),
        })
    );
}

#[test]
fn typed_values_report_the_value_and_expected_type() {
    assert_eq!(
        value_as::<u16>(OsStr::new("wide"), "width"),
        Err(Error::InvalidValue {
            argument: "width".into(),
            value: "wide".into(),
            expected: "u16",
        })
    );
}

#[test]
fn text_values_are_never_converted_lossily() {
    assert_eq!(value_text(OsStr::new("keys"), "keys"), Ok("keys".into()));
}

#[cfg(unix)]
#[test]
fn non_unicode_positional_values_remain_intact() {
    use std::os::unix::ffi::OsStringExt;

    let raw = OsString::from_vec(vec![0xff]);
    let mut args = Parser::new([raw.clone()]);

    assert_eq!(args.next_arg(), Ok(Some(Arg::Value(raw))));
}
