use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _, _] if command == "custom-object-library" => {
            Some(Command::CustomObjectLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                normalized_outputs: None,
                observation: None,
            })
        }
        [command, _, _, _, _] if command == "custom-object-library" => {
            Some(Command::CustomObjectLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                normalized_outputs: Some((PathBuf::from(&args[3]), PathBuf::from(&args[4]))),
                observation: None,
            })
        }
        [command, _, _, _, _, _] if command == "custom-object-library" => {
            Some(Command::CustomObjectLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                normalized_outputs: Some((PathBuf::from(&args[3]), PathBuf::from(&args[4]))),
                observation: Some(PathBuf::from(&args[5])),
            })
        }
        [command, _, _, _] if command == "custom-sprite-library" => {
            Some(Command::CustomSpriteLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                sprite_lengths: PathBuf::from(&args[3]),
                normalized_outputs: None,
                observation: None,
            })
        }
        [command, _, _, _, _, _] if command == "custom-sprite-library" => {
            Some(Command::CustomSpriteLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                sprite_lengths: PathBuf::from(&args[3]),
                normalized_outputs: Some((PathBuf::from(&args[4]), PathBuf::from(&args[5]))),
                observation: None,
            })
        }
        [command, _, _, _, _, _, _] if command == "custom-sprite-library" => {
            Some(Command::CustomSpriteLibrary {
                data: PathBuf::from(&args[1]),
                descriptions: PathBuf::from(&args[2]),
                sprite_lengths: PathBuf::from(&args[3]),
                normalized_outputs: Some((PathBuf::from(&args[4]), PathBuf::from(&args[5]))),
                observation: Some(PathBuf::from(&args[6])),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_sprite_library_forms() {
        let args: Vec<OsString> = [
            "custom-sprite-library",
            "a.mw2",
            "a.mwt",
            "lengths.bin",
            "b.mw2",
            "b.mwt",
            "sprites.obs",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text),
            Some(Command::CustomSpriteLibrary {
                observation: Some(_),
                ..
            })
        ));
    }
}
