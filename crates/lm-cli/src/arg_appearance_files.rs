use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    let (overworld, normalized, observation) = match text {
        [command, _] if command == "appearance-file" => (false, None, None),
        [command, _, _] if command == "appearance-file" => (false, Some(2), None),
        [command, _, _, _] if command == "appearance-file" => (false, Some(2), Some(3)),
        [command, _] if command == "overworld-appearance-file" => (true, None, None),
        [command, _, _] if command == "overworld-appearance-file" => (true, Some(2), None),
        [command, _, _, _] if command == "overworld-appearance-file" => (true, Some(2), Some(3)),
        _ => return None,
    };
    let input = PathBuf::from(&args[1]);
    let normalized_output = normalized.map(|index| PathBuf::from(&args[index]));
    let observation = observation.map(|index| PathBuf::from(&args[index]));
    Some(if overworld {
        Command::OverworldAppearanceFile {
            input,
            normalized_output,
            observation,
        }
    } else {
        Command::AppearanceFile {
            input,
            normalized_output,
            observation,
        }
    })
}
