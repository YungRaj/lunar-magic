use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "revision-patch-file" => Some(Command::RevisionPatchFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "revision-patch-file" => Some(Command::RevisionPatchFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "revision-patch-file" => {
            Some(Command::RevisionPatchFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: Some(PathBuf::from(&args[3])),
            })
        }
        _ => None,
    }
}
