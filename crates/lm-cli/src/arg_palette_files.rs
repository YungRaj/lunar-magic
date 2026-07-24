use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

enum Kind {
    Portable,
    Smw,
    Tpl,
    Raw,
    Mask,
    Rgb,
}

pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    let (kind, normalized, observation) = match text {
        [command, _] => (kind(command)?, None, None),
        [command, _, _] => (kind(command)?, Some(2), None),
        [command, _, _, _] => (kind(command)?, Some(2), Some(3)),
        _ => return None,
    };
    let input = PathBuf::from(&args[1]);
    let normalized_output = normalized.map(|index| PathBuf::from(&args[index]));
    let observation = observation.map(|index| PathBuf::from(&args[index]));
    Some(match kind {
        Kind::Portable => Command::PaletteFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Smw => Command::SmwPaletteFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Tpl => Command::TplPaletteFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Raw => Command::RawPaletteFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Mask => Command::PaletteMaskFile {
            input,
            normalized_output,
            observation,
        },
        Kind::Rgb => Command::RgbPaletteFile {
            input,
            normalized_output,
            observation,
        },
    })
}

fn kind(command: &str) -> Option<Kind> {
    Some(match command {
        "palette-file" => Kind::Portable,
        "smw-palette-file" => Kind::Smw,
        "tpl-palette-file" => Kind::Tpl,
        "raw-palette-file" => Kind::Raw,
        "palette-mask-file" => Kind::Mask,
        "rgb-palette-file" => Kind::Rgb,
        _ => return None,
    })
}
