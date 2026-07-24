use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    let (normalized, observation) = match text {
        [command, _, _] if command == "native-level-file" => (None, None),
        [command, _, _, _] if command == "native-level-file" => (Some(3), None),
        [command, _, _, _, _] if command == "native-level-file" => (Some(3), Some(4)),
        _ => return None,
    };
    Some(Command::NativeLevelFile {
        input: PathBuf::from(&args[1]),
        sprite_lengths: (text[2].as_ref() != "standard").then(|| PathBuf::from(&args[2])),
        normalized_output: normalized.map(|index| PathBuf::from(&args[index])),
        observation: observation.map(|index| PathBuf::from(&args[index])),
    })
}
