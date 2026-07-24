use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

#[must_use]
pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "graphics-remap-file" => Some(Command::GraphicsRemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "graphics-remap-file" => Some(Command::GraphicsRemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "graphics-remap-file" => {
            Some(Command::GraphicsRemapFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: Some(PathBuf::from(&args[3])),
            })
        }
        [command, _, _, _] if command == "graphics-remap-apply" => {
            Some(Command::GraphicsRemapApply {
                stream: PathBuf::from(&args[1]),
                scratch: PathBuf::from(&args[2]),
                output: PathBuf::from(&args[3]),
                observation: None,
            })
        }
        [command, _, _, _, _] if command == "graphics-remap-apply" => {
            Some(Command::GraphicsRemapApply {
                stream: PathBuf::from(&args[1]),
                scratch: PathBuf::from(&args[2]),
                output: PathBuf::from(&args[3]),
                observation: Some(PathBuf::from(&args[4])),
            })
        }
        _ => None,
    }
}
