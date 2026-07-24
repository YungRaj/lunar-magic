use super::{ShellCommand, ShellCommandError};
use std::path::PathBuf;

pub(super) fn no_argument(
    argument: &str,
    command: &'static str,
    parsed: ShellCommand,
) -> Result<ShellCommand, ShellCommandError> {
    if argument.is_empty() {
        Ok(parsed)
    } else {
        Err(ShellCommandError::UnexpectedArgument(command))
    }
}

pub(super) fn path_argument(
    argument: &str,
    command: &'static str,
) -> Result<PathBuf, ShellCommandError> {
    if argument.is_empty() {
        Err(ShellCommandError::MissingArgument(command))
    } else {
        Ok(PathBuf::from(argument))
    }
}

pub(super) fn hex_argument(
    argument: &str,
    command: &'static str,
) -> Result<u16, ShellCommandError> {
    if argument.is_empty() {
        return Err(ShellCommandError::MissingArgument(command));
    }
    if argument.split_whitespace().count() != 1 {
        return Err(ShellCommandError::UnexpectedArgument(command));
    }
    let value = hex_value(argument, command)?;
    u16::try_from(value).map_err(|_| ShellCommandError::InvalidHex {
        command,
        value: argument.to_owned(),
    })
}

pub(super) fn hex_value(argument: &str, command: &'static str) -> Result<u64, ShellCommandError> {
    let value = argument
        .strip_prefix("0x")
        .or_else(|| argument.strip_prefix("0X"))
        .unwrap_or(argument);
    u64::from_str_radix(value, 16).map_err(|_| ShellCommandError::InvalidHex {
        command,
        value: argument.to_owned(),
    })
}

pub(super) fn hex_usize(argument: &str, command: &'static str) -> Result<usize, ShellCommandError> {
    usize::try_from(hex_value(argument, command)?).map_err(|_| ShellCommandError::InvalidRange {
        command,
        value: argument.into(),
    })
}

pub(super) fn decimal_argument(
    argument: &str,
    command: &'static str,
) -> Result<usize, ShellCommandError> {
    if argument.is_empty() {
        return Err(ShellCommandError::MissingArgument(command));
    }
    if argument.split_whitespace().count() != 1 {
        return Err(ShellCommandError::UnexpectedArgument(command));
    }
    argument
        .parse()
        .map_err(|_| ShellCommandError::InvalidIndex {
            command,
            value: argument.into(),
        })
}

pub(super) fn single_string_argument(
    argument: &str,
    command: &'static str,
) -> Result<String, ShellCommandError> {
    if argument.is_empty() {
        return Err(ShellCommandError::MissingArgument(command));
    }
    if argument.split_whitespace().count() != 1 {
        return Err(ShellCommandError::UnexpectedArgument(command));
    }
    Ok(argument.into())
}
