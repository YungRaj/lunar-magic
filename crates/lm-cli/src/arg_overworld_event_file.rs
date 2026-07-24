use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "overworld-event-file" => Some(Command::OverworldEventFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "overworld-event-file" => Some(Command::OverworldEventFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "overworld-event-file" => {
            Some(Command::OverworldEventFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: Some(PathBuf::from(&args[3])),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_optional_outputs_without_lossy_path_conversion() {
        let args = [
            OsString::from("overworld-event-file"),
            OsString::from("Events 日本語.lmevt"),
            OsString::from("Normalized Events.lmevt"),
            OsString::from("Events.obs"),
        ];
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args[..2], &text[..2]),
            Some(Command::OverworldEventFile {
                normalized_output: None,
                observation: None,
                ..
            })
        ));
        assert!(matches!(
            parse(&args, &text),
            Some(Command::OverworldEventFile {
                normalized_output: Some(_),
                observation: Some(_),
                ..
            })
        ));
    }
}
