use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _, _] if command == "smw-shared-palette-export" => {
            Some(Command::SmwSharedPaletteExport {
                rom: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        [command, _, _, _] if command == "smw-shared-palette-import" => {
            Some(Command::SmwSharedPaletteImport {
                input_rom: PathBuf::from(&args[1]),
                palette: PathBuf::from(&args[2]),
                output_rom: PathBuf::from(&args[3]),
            })
        }
        _ => None,
    }
}
