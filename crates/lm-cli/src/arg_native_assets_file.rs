use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _, _] if command == "native-assets-file" => Some(Command::NativeAssetsFile {
            input: PathBuf::from(&args[1]),
            profile: PathBuf::from(&args[2]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _, _] if command == "native-assets-file" => Some(Command::NativeAssetsFile {
            input: PathBuf::from(&args[1]),
            profile: PathBuf::from(&args[2]),
            normalized_output: Some(PathBuf::from(&args[3])),
            observation: None,
        }),
        [command, _, _, _, _] if command == "native-assets-file" => {
            Some(Command::NativeAssetsFile {
                input: PathBuf::from(&args[1]),
                profile: PathBuf::from(&args[2]),
                normalized_output: Some(PathBuf::from(&args[3])),
                observation: Some(PathBuf::from(&args[4])),
            })
        }
        _ => None,
    }
}
