//! Help text rendered from the same definitions the parser validates.

use crate::spec::Kind;
use crate::{ArgSpec, ValueSpec};

pub(crate) fn render<I>(
    name: &str,
    about: &str,
    version: Option<&str>,
    after_help: &str,
    args: &[ArgSpec<I>],
) -> String {
    let mut help = name.to_owned();
    if !about.is_empty() {
        help.push_str(" — ");
        help.push_str(about);
    }

    help.push_str("\n\nUsage:\n  ");
    help.push_str(name);
    // `-h` and `--help` always exist, even when the application defines no
    // options of its own.
    help.push_str(" [options]");
    for argument in args
        .iter()
        .filter(|argument| !argument.hidden && matches!(argument.kind, Kind::Positional { .. }))
    {
        if let Kind::Positional { value } = &argument.kind {
            help.push(' ');
            push_value_label(&mut help, value);
        }
    }

    let mut options = args
        .iter()
        .filter(|argument| {
            !argument.hidden
                && argument.long.is_some()
                && !matches!(argument.kind, Kind::Mode { .. })
        })
        .map(option_row)
        .collect::<Vec<_>>();
    options.push(("-h, --help".into(), "print help and exit".into()));
    if version.is_some() {
        options.push(("-V, --version".into(), "print version and exit".into()));
    }
    push_rows(&mut help, "Options", &options);

    let modes = args
        .iter()
        .filter(|argument| !argument.hidden && matches!(argument.kind, Kind::Mode { .. }))
        .map(option_row)
        .collect::<Vec<_>>();
    push_rows(&mut help, "Modes", &modes);

    let positionals = args
        .iter()
        .filter(|argument| !argument.hidden && matches!(argument.kind, Kind::Positional { .. }))
        .map(|argument| {
            let Kind::Positional { value } = &argument.kind else {
                return (String::new(), String::new());
            };
            (value_label(value), argument.help.clone())
        })
        .collect::<Vec<_>>();
    push_rows(&mut help, "Arguments", &positionals);

    if !after_help.is_empty() {
        help.push_str("\n\n");
        help.push_str(after_help);
    }
    help
}

fn option_row<I>(argument: &ArgSpec<I>) -> (String, String) {
    let mut label = String::new();
    if let Some(short) = argument.short {
        label.push('-');
        label.push(short);
        label.push_str(", ");
    } else {
        label.push_str("    ");
    }
    if let Some(long) = &argument.long {
        label.push_str("--");
        label.push_str(long);
    }
    match &argument.kind {
        Kind::Option { value } => {
            label.push(' ');
            push_value_label(&mut label, value);
        }
        Kind::Mode { values } => {
            for value in values {
                label.push(' ');
                push_value_label(&mut label, value);
            }
        }
        Kind::Flag | Kind::Positional { .. } => {}
    }
    (label, argument.help.clone())
}

fn value_label(value: &ValueSpec) -> String {
    let mut label = String::new();
    push_value_label(&mut label, value);
    label
}

fn push_value_label(label: &mut String, value: &ValueSpec) {
    if value.required {
        label.push('<');
        label.push_str(&value.name);
        label.push('>');
    } else {
        label.push('[');
        label.push_str(&value.name);
        label.push(']');
    }
}

fn push_rows(help: &mut String, title: &str, rows: &[(String, String)]) {
    if rows.is_empty() {
        return;
    }
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    help.push_str("\n\n");
    help.push_str(title);
    help.push_str(":\n");
    for (label, description) in rows {
        help.push_str("  ");
        help.push_str(label);
        let padding = width.saturating_sub(label.chars().count()) + 2;
        help.extend(std::iter::repeat_n(' ', padding));
        help.push_str(description);
        help.push('\n');
    }
    if help.ends_with('\n') {
        help.pop();
    }
}
