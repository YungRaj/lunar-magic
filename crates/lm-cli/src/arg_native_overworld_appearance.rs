use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    let (outputs, observation) = match text {
        [command, _, _] if command == "native-overworld-appearance-file" => (None, None),
        [command, _, _, _, _] if command == "native-overworld-appearance-file" => {
            (Some((3, 4)), None)
        }
        [command, _, _, _, _, _] if command == "native-overworld-appearance-file" => {
            (Some((3, 4)), Some(5))
        }
        _ => return None,
    };
    Some(Command::NativeOverworldAppearanceFile {
        definitions: PathBuf::from(&args[1]),
        sprite_map16: PathBuf::from(&args[2]),
        normalized_outputs: outputs
            .map(|(first, second)| (PathBuf::from(&args[first]), PathBuf::from(&args[second]))),
        observation: observation.map(|index| PathBuf::from(&args[index])),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_the_complete_optional_output_pair() {
        let args = [
            "native-overworld-appearance-file",
            "sprites.sscov",
            "sprites.s16ov",
            "normalized.sscov",
            "normalized.s16ov",
            "sprites.obs",
        ]
        .map(OsString::from);
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text),
            Some(Command::NativeOverworldAppearanceFile {
                normalized_outputs: Some(_),
                observation: Some(_),
                ..
            })
        ));
        let incomplete = &args[..4];
        let text: Vec<_> = incomplete
            .iter()
            .map(|value| value.to_string_lossy())
            .collect();
        assert!(parse(incomplete, &text).is_none());
    }
}
