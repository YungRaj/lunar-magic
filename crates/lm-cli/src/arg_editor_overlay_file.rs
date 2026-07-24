use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "editor-overlay-file" => Some(Command::EditorOverlayFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "editor-overlay-file" => Some(Command::EditorOverlayFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "editor-overlay-file" => {
            Some(Command::EditorOverlayFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: Some(PathBuf::from(&args[3])),
            })
        }
        _ => None,
    }
}
