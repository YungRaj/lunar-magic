use crate::arg_values::{ArgsError, parse_number};
use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

pub fn parse(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, palette_row, columns, _] if command == "render-graphics" => {
            Some(Command::RenderGraphics {
                graphics: PathBuf::from(&args[1]),
                palette: PathBuf::from(&args[2]),
                palette_row: to_usize(palette_row, "palette row")?,
                columns: to_usize(columns, "column count")?,
                output: PathBuf::from(&args[5]),
            })
        }
        [command, _, columns, cell_size, _] if command == "render-palette" => {
            Some(Command::RenderPalette {
                palette: PathBuf::from(&args[1]),
                columns: to_usize(columns, "column count")?,
                cell_size: to_usize(cell_size, "cell size")?,
                output: PathBuf::from(&args[4]),
            })
        }
        _ => None,
    })
}

fn to_usize(value: &str, name: &str) -> Result<usize, ArgsError> {
    usize::try_from(parse_number(value)?)
        .map_err(|_| ArgsError(format!("{name} does not fit usize")))
}
