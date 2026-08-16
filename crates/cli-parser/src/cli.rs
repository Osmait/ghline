//! Schema-driven parsing and help generation.

use std::ffi::OsString;

use crate::matches::Occurrence;
use crate::spec::Kind;
use crate::{Arg, ArgSpec, Error, Matches, Outcome, Parser};

/// A complete, application-independent command-line definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cli<I> {
    name: String,
    about: String,
    version: Option<String>,
    after_help: String,
    args: Vec<ArgSpec<I>>,
}

impl<I> Cli<I> {
    /// Starts a schema named after the executable users invoke.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: String::new(),
            version: None,
            after_help: String::new(),
            args: Vec::new(),
        }
    }

    /// Sets the one-line application description.
    #[must_use]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = about.into();
        self
    }

    /// Enables the built-in `-V` and `--version` response.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Adds application-specific text below generated argument help.
    #[must_use]
    pub fn after_help(mut self, after_help: impl Into<String>) -> Self {
        self.after_help = after_help.into();
        self
    }

    /// Adds one typed argument definition.
    #[must_use]
    pub fn arg(mut self, argument: ArgSpec<I>) -> Self {
        self.args.push(argument);
        self
    }

    /// Generates help from the same schema used for parsing.
    #[must_use]
    pub fn help(&self) -> String {
        crate::help::render(
            &self.name,
            &self.about,
            self.version.as_deref(),
            &self.after_help,
            &self.args,
        )
    }
}

impl<I> Cli<I>
where
    I: Clone + PartialEq,
{
    /// Parses raw arguments against this schema.
    pub fn parse(&self, args: impl IntoIterator<Item = OsString>) -> Result<Outcome<I>, Error> {
        self.validate()?;
        let mut parser = Parser::new(args);
        let Some(first) = parser.next_arg()? else {
            return self.finish(Vec::new(), 0);
        };

        self.parse_options(&mut parser, first)
    }

    /// Parses the current process's arguments against this schema.
    pub fn parse_env(&self) -> Result<Outcome<I>, Error> {
        self.parse(std::env::args_os().skip(1))
    }

    fn parse_options(&self, parser: &mut Parser, first: Arg) -> Result<Outcome<I>, Error> {
        let mut occurrences = Vec::new();
        let mut position = 0;
        let mut current = Some(first);

        loop {
            let argument = if let Some(argument) = current.take() {
                argument
            } else if let Some(argument) = parser.next_arg()? {
                argument
            } else {
                return self.finish(occurrences, position);
            };

            if let Some(outcome) = self.builtin(&argument) {
                return Ok(outcome);
            }

            match argument {
                Arg::Long { name, value } => {
                    let Some(spec) = self.args.iter().find(|spec| {
                        spec.long.as_deref() == Some(name.as_str())
                            && !matches!(spec.kind, Kind::Positional { .. })
                    }) else {
                        return Err(Error::unexpected(Arg::Long { name, value }));
                    };
                    let label = format!("--{name}");
                    let values = match &spec.kind {
                        Kind::Flag if value.is_none() => Vec::new(),
                        Kind::Flag => return Err(Error::unexpected(Arg::Long { name, value })),
                        Kind::Option { .. } => vec![parser.value(label.clone(), value)?],
                        Kind::Mode { .. } => {
                            return self.parse_mode(parser, spec, value, occurrences, position);
                        }
                        Kind::Positional { .. } => {
                            return Err(Error::unexpected(Arg::Long { name, value }));
                        }
                    };
                    push_occurrence(&mut occurrences, spec, values, label)?;
                }
                Arg::Short(name) => {
                    let mut chars = name.chars();
                    let Some(short) = chars.next() else {
                        return Err(Error::unexpected(Arg::Short(name)));
                    };
                    if chars.next().is_some() {
                        return Err(Error::unexpected(Arg::Short(name)));
                    }
                    let Some(spec) = self.args.iter().find(|spec| spec.short == Some(short)) else {
                        return Err(Error::unexpected(Arg::Short(name)));
                    };
                    let label = format!("-{short}");
                    let values = match &spec.kind {
                        Kind::Flag => Vec::new(),
                        Kind::Option { .. } => vec![parser.value(label.clone(), None)?],
                        Kind::Mode { .. } | Kind::Positional { .. } => {
                            return Err(Error::unexpected(Arg::Short(name)));
                        }
                    };
                    push_occurrence(&mut occurrences, spec, values, label)?;
                }
                Arg::Value(value) => {
                    let Some(spec) = self.positionals().nth(position) else {
                        return Err(Error::unexpected(Arg::Value(value)));
                    };
                    push_occurrence(&mut occurrences, spec, vec![value], positional_name(spec))?;
                    position += 1;
                }
            }
        }
    }

    fn parse_mode(
        &self,
        parser: &mut Parser,
        mode: &ArgSpec<I>,
        inline: Option<OsString>,
        mut occurrences: Vec<Occurrence<I>>,
        position: usize,
    ) -> Result<Outcome<I>, Error> {
        if let Some(missing) = self
            .positionals()
            .nth(position)
            .filter(|argument| positional_value(argument).is_some_and(|value| value.required))
        {
            return Err(Error::MissingValue {
                argument: positional_name(missing),
            });
        }

        let Kind::Mode { values: schema } = &mode.kind else {
            return Err(Error::InvalidDefinition {
                detail: "a non-mode reached mode parsing".into(),
            });
        };
        let mut values = Vec::new();
        if let Some(value) = inline {
            values.push(value);
        }
        while let Some(argument) = parser.next_arg()? {
            match argument {
                Arg::Value(value) if values.len() < schema.len() => values.push(value),
                argument => return Err(Error::unexpected(argument)),
            }
        }
        if let Some(missing) = schema.get(values.len()).filter(|value| value.required) {
            return Err(Error::MissingValue {
                argument: missing.name.clone(),
            });
        }

        let label = mode
            .long
            .as_deref()
            .map_or_else(|| "mode".into(), |long| format!("--{long}"));
        push_occurrence(&mut occurrences, mode, values, label)?;
        Ok(Outcome::Matches(Matches { occurrences }))
    }

    fn finish(
        &self,
        occurrences: Vec<Occurrence<I>>,
        position: usize,
    ) -> Result<Outcome<I>, Error> {
        if let Some(missing) = self
            .positionals()
            .nth(position)
            .filter(|argument| positional_value(argument).is_some_and(|value| value.required))
        {
            return Err(Error::MissingValue {
                argument: positional_name(missing),
            });
        }
        Ok(Outcome::Matches(Matches { occurrences }))
    }

    fn builtin(&self, argument: &Arg) -> Option<Outcome<I>> {
        match argument {
            Arg::Long { name, value } if name == "help" && value.is_none() => {
                Some(Outcome::Help(self.help()))
            }
            Arg::Short(name) if name == "h" => Some(Outcome::Help(self.help())),
            Arg::Long { name, value }
                if name == "version" && value.is_none() && self.version.is_some() =>
            {
                self.version_text().map(Outcome::Version)
            }
            Arg::Short(name) if name == "V" && self.version.is_some() => {
                self.version_text().map(Outcome::Version)
            }
            _ => None,
        }
    }

    fn version_text(&self) -> Option<String> {
        self.version
            .as_ref()
            .map(|version| format!("{} {version}", self.name))
    }

    fn positionals(&self) -> impl Iterator<Item = &ArgSpec<I>> {
        self.args
            .iter()
            .filter(|argument| matches!(argument.kind, Kind::Positional { .. }))
    }

    fn validate(&self) -> Result<(), Error> {
        for (index, argument) in self.args.iter().enumerate() {
            if self.args[..index]
                .iter()
                .any(|earlier| earlier.id == argument.id)
            {
                return Err(invalid("two arguments share one ID"));
            }
            if let Some(long) = &argument.long {
                if long.is_empty() || long.starts_with('-') || long.contains('=') {
                    return Err(invalid(format!("'{long}' is not a valid long name")));
                }
                if long == "help" || long == "version" {
                    return Err(invalid(format!("--{long} is reserved")));
                }
                if self.args[..index]
                    .iter()
                    .any(|earlier| earlier.long.as_ref() == Some(long))
                {
                    return Err(invalid(format!("--{long} is defined twice")));
                }
            }
            if let Some(short) = argument.short {
                if short == 'h' || short == 'V' {
                    return Err(invalid(format!("-{short} is reserved")));
                }
                if self.args[..index]
                    .iter()
                    .any(|earlier| earlier.short == Some(short))
                {
                    return Err(invalid(format!("-{short} is defined twice")));
                }
            }
            match &argument.kind {
                Kind::Option { value } => validate_values(std::slice::from_ref(value))?,
                Kind::Mode { values } => {
                    if argument.short.is_some() {
                        return Err(invalid("modes cannot have short aliases"));
                    }
                    validate_values(values)?;
                }
                Kind::Positional { .. } if argument.short.is_some() => {
                    return Err(invalid("positional values cannot have short aliases"));
                }
                Kind::Flag | Kind::Positional { .. } => {}
            }
        }

        let positionals = self
            .positionals()
            .filter_map(positional_value)
            .cloned()
            .collect::<Vec<_>>();
        validate_values(&positionals)
    }
}

fn push_occurrence<I>(
    occurrences: &mut Vec<Occurrence<I>>,
    spec: &ArgSpec<I>,
    values: Vec<OsString>,
    label: String,
) -> Result<(), Error>
where
    I: Clone + PartialEq,
{
    if occurrences
        .iter()
        .any(|occurrence| occurrence.id == spec.id)
    {
        return Err(Error::duplicate(label));
    }
    occurrences.push(Occurrence {
        id: spec.id.clone(),
        values,
    });
    Ok(())
}

fn validate_values(values: &[crate::ValueSpec]) -> Result<(), Error> {
    let mut optional_seen = false;
    for value in values {
        if value.name.is_empty() {
            return Err(invalid("a value name is empty"));
        }
        if value.required && optional_seen {
            return Err(invalid(format!(
                "required value '{}' follows an optional value",
                value.name
            )));
        }
        optional_seen |= !value.required;
    }
    Ok(())
}

fn invalid(detail: impl Into<String>) -> Error {
    Error::InvalidDefinition {
        detail: detail.into(),
    }
}

fn positional_value<I>(argument: &ArgSpec<I>) -> Option<&crate::ValueSpec> {
    let Kind::Positional { value } = &argument.kind else {
        return None;
    };
    Some(value)
}

fn positional_name<I>(argument: &ArgSpec<I>) -> String {
    positional_value(argument).map_or_else(|| "value".into(), |value| value.name.clone())
}
