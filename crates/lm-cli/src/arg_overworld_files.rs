use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "overworld-path" => Some(Command::OverworldPath {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "overworld-path" => Some(Command::OverworldPath {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "overworld-path" => Some(Command::OverworldPath {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        [command, _] if command == "overworld-metadata" => Some(Command::OverworldMetadata {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "overworld-metadata" => Some(Command::OverworldMetadata {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "overworld-metadata" => Some(Command::OverworldMetadata {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        _ => None,
    }
}
