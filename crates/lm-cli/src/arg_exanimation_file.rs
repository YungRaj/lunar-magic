use crate::arg_values::{ArgsError, parse_number};
use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn parse(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    let (normalized, observation) = match text {
        [command, _, _, _] if command == "exanimation-file" => (None, None),
        [command, _, _, _, _] if command == "exanimation-file" => (Some(4), None),
        [command, _, _, _, _, _] if command == "exanimation-file" => (Some(4), Some(5)),
        _ => return Ok(None),
    };
    Ok(Some(Command::ExAnimationFile {
        input: PathBuf::from(&args[1]),
        size_modes: PathBuf::from(&args[2]),
        maximum_records: usize::try_from(parse_number(&text[3])?)
            .map_err(|_| ArgsError("maximum record count does not fit usize".into()))?,
        normalized_output: normalized.map(|index| PathBuf::from(&args[index])),
        observation: observation.map(|index| PathBuf::from(&args[index])),
    }))
}
