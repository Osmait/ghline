//! A pull parser over raw operating-system arguments.

use std::any::type_name;
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::str::FromStr;

use crate::{Arg, Error};

/// Classifies raw arguments while leaving option names and command structure
/// to the caller.
#[derive(Debug)]
pub struct Parser {
    args: VecDeque<OsString>,
    options: bool,
}

impl Parser {
    /// Creates a parser over arguments that do not include the executable name.
    pub fn new(args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            args: args.into_iter().collect(),
            options: true,
        }
    }

    /// Creates a parser over the current process's command line.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(std::env::args_os().skip(1))
    }

    /// Returns the next syntax-level argument.
    ///
    /// `--` is consumed rather than returned and causes every following item
    /// to be emitted as [`Arg::Value`].
    pub fn next_arg(&mut self) -> Result<Option<Arg>, Error> {
        while let Some(raw) = self.args.pop_front() {
            if !self.options {
                return Ok(Some(Arg::Value(raw)));
            }
            if raw == OsStr::new("--") {
                self.options = false;
                continue;
            }

            let bytes = raw.as_os_str().as_encoded_bytes();
            if raw == OsStr::new("-") || !bytes.starts_with(b"-") {
                return Ok(Some(Arg::Value(raw)));
            }

            let Some(text) = raw.to_str() else {
                return Err(Error::InvalidOption(raw));
            };
            if let Some(option) = text.strip_prefix("--") {
                let (name, value) = option
                    .split_once('=')
                    .map_or((option, None), |(name, value)| {
                        (name, Some(OsString::from(value)))
                    });
                if name.is_empty() {
                    return Err(Error::MissingOptionName(raw));
                }
                return Ok(Some(Arg::Long {
                    name: name.into(),
                    value,
                }));
            }

            if let Some(option) = text.strip_prefix('-') {
                return Ok(Some(Arg::Short(option.into())));
            }
            return Ok(Some(Arg::Value(raw)));
        }
        Ok(None)
    }

    /// Takes an option value, preferring its `--name=value` form.
    pub fn value(
        &mut self,
        argument: impl Into<String>,
        inline: Option<OsString>,
    ) -> Result<OsString, Error> {
        inline
            .or_else(|| self.args.pop_front())
            .ok_or_else(|| Error::MissingValue {
                argument: argument.into(),
            })
    }

    /// Takes an option value and converts it with [`FromStr`].
    pub fn value_as<T>(
        &mut self,
        argument: impl Into<String>,
        inline: Option<OsString>,
    ) -> Result<T, Error>
    where
        T: FromStr,
    {
        let argument = argument.into();
        let value = self.value(argument.clone(), inline)?;
        value_as(&value, argument)
    }
}

/// Converts a raw positional or option value with [`FromStr`].
pub fn value_as<T>(value: &OsStr, argument: impl Into<String>) -> Result<T, Error>
where
    T: FromStr,
{
    let argument = argument.into();
    value
        .to_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| Error::InvalidValue {
            argument,
            value: value.to_owned(),
            expected: type_name::<T>(),
        })
}

/// Converts a raw value to Unicode text without changing it lossily.
pub fn value_text(value: &OsStr, argument: impl Into<String>) -> Result<String, Error> {
    value
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::InvalidValue {
            argument: argument.into(),
            value: value.to_owned(),
            expected: "UTF-8 text",
        })
}
