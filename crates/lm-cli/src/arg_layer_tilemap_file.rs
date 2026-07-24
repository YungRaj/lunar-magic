use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "layer-tilemap-file" => Some(Command::LayerTilemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "layer-tilemap-file" => Some(Command::LayerTilemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "layer-tilemap-file" => Some(Command::LayerTilemapFile {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_supported_output_groups_without_lossy_paths() {
        let args = [
            OsString::from("layer-tilemap-file"),
            OsString::from("source title.lmtile"),
            OsString::from("normalized title.lmtile"),
            OsString::from("title observation.obs"),
        ];
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args[..2], &text[..2]),
            Some(Command::LayerTilemapFile {
                normalized_output: None,
                observation: None,
                ..
            })
        ));
        assert!(matches!(
            parse(&args, &text),
            Some(Command::LayerTilemapFile {
                normalized_output: Some(_),
                observation: Some(_),
                ..
            })
        ));
    }
}
