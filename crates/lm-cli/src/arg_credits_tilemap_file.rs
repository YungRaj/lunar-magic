use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "credits-tilemap-file" => Some(Command::CreditsTilemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "credits-tilemap-file" => Some(Command::CreditsTilemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "credits-tilemap-file" => {
            Some(Command::CreditsTilemapFile {
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
    fn parses_all_output_groups_and_preserves_native_paths() {
        let args = [
            OsString::from("credits-tilemap-file"),
            OsString::from("Credits 日本語.lmcred"),
            OsString::from("Normalized Credits.lmcred"),
            OsString::from("Credits Observation.obs"),
        ];
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args[..2], &text[..2]),
            Some(Command::CreditsTilemapFile {
                normalized_output: None,
                observation: None,
                ..
            })
        ));
        assert!(matches!(
            parse(&args, &text),
            Some(Command::CreditsTilemapFile {
                normalized_output: Some(_),
                observation: Some(_),
                ..
            })
        ));
    }
}
