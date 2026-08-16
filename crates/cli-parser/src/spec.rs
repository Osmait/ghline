//! Declarative argument metadata shared by parsing and help generation.

/// One positional value accepted by an option, mode or application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSpec {
    pub(crate) name: String,
    pub(crate) required: bool,
}

impl ValueSpec {
    /// Defines a value that must be present.
    pub fn required(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required: true,
        }
    }

    /// Defines a value that may be omitted.
    pub fn optional(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            required: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Flag,
    Option { value: ValueSpec },
    Mode { values: Vec<ValueSpec> },
    Positional { value: ValueSpec },
}

/// One application-defined argument in a [`crate::Cli`] schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec<I> {
    pub(crate) id: I,
    pub(crate) long: Option<String>,
    pub(crate) short: Option<char>,
    pub(crate) help: String,
    pub(crate) hidden: bool,
    pub(crate) kind: Kind,
}

impl<I> ArgSpec<I> {
    /// Defines a switch that carries no value.
    pub fn flag(id: I, long: impl Into<String>) -> Self {
        Self::named(id, long, Kind::Flag)
    }

    /// Defines an option that takes exactly one value.
    pub fn option(id: I, long: impl Into<String>, value_name: impl Into<String>) -> Self {
        Self::named(
            id,
            long,
            Kind::Option {
                value: ValueSpec::required(value_name),
            },
        )
    }

    /// Defines an exclusive mode whose following positional values have a
    /// fixed schema.
    pub fn mode(
        id: I,
        long: impl Into<String>,
        values: impl IntoIterator<Item = ValueSpec>,
    ) -> Self {
        Self::named(
            id,
            long,
            Kind::Mode {
                values: values.into_iter().collect(),
            },
        )
    }

    /// Defines a top-level positional value.
    pub fn positional(id: I, value: ValueSpec) -> Self {
        Self {
            id,
            long: None,
            short: None,
            help: String::new(),
            hidden: false,
            kind: Kind::Positional { value },
        }
    }

    /// Adds a one-character alias such as `-V`.
    #[must_use]
    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    /// Describes the argument in generated help.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// Keeps the argument parseable while omitting it from generated help.
    #[must_use]
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    fn named(id: I, long: impl Into<String>, kind: Kind) -> Self {
        Self {
            id,
            long: Some(long.into()),
            short: None,
            help: String::new(),
            hidden: false,
            kind,
        }
    }
}
