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

            if bytes.starts_with(b"--") {
                let Some((name, value)) = split_long_option(&raw) else {
                    return Err(Error::InvalidOption(raw));
                };
                if name.is_empty() {
                    return Err(Error::MissingOptionName(raw));
                }
                return Ok(Some(Arg::Long { name, value }));
            }

            let Some(text) = raw.to_str() else {
                return Err(Error::InvalidOption(raw));
            };
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

/// Splits `--name` or `--name=value` without requiring `value` to be UTF-8.
///
/// The option name itself must still be valid UTF-8 so callers can match it
/// against their schema; the inline value is kept as raw OS bytes.
fn split_long_option(raw: &OsStr) -> Option<(String, Option<OsString>)> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let rest = raw.as_bytes().strip_prefix(b"--")?;
        if let Some(eq) = rest.iter().position(|&b| b == b'=') {
            let name = std::str::from_utf8(&rest[..eq]).ok()?;
            let value = OsStr::from_bytes(&rest[eq + 1..]).to_os_string();
            Some((name.to_owned(), Some(value)))
        } else {
            let name = std::str::from_utf8(rest).ok()?;
            Some((name.to_owned(), None))
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let wide: Vec<u16> = raw.encode_wide().collect();
        let rest = wide.strip_prefix(&[u16::from(b'-'), u16::from(b'-')])?;
        if let Some(eq) = rest.iter().position(|&u| u == u16::from(b'=')) {
            let name = String::from_utf16(&rest[..eq]).ok()?;
            let value = OsString::from_wide(&rest[eq + 1..]);
            Some((name, Some(value)))
        } else {
            let name = String::from_utf16(rest).ok()?;
            Some((name, None))
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let option = raw.to_str()?.strip_prefix("--")?;
        let (name, value) = option
            .split_once('=')
            .map_or((option, None), |(n, v)| (n, Some(OsString::from(v))));
        Some((name.to_owned(), value))
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
