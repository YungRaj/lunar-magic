use crate::arg_values::{ArgsError, parse_number};
use crate::command_types::{
    Command, PngMap16ImportCommand, RgbMap16ImportCommand, RgbaMap16ImportCommand,
};
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_rgb_map16_import(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, _, _, _, row, acts, page, _, _, _, _] if command == "import-rgb-map16" => {
            Some(Command::ImportRgbMap16(RgbMap16ImportCommand {
                rgb: PathBuf::from(&args[1]),
                palette: PathBuf::from(&args[2]),
                palette_access: PathBuf::from(&args[3]),
                graphics: PathBuf::from(&args[4]),
                occupancy: PathBuf::from(&args[5]),
                palette_row: u8::try_from(parse_number(row)?)
                    .map_err(|_| ArgsError("palette row exceeds one byte".into()))?,
                acts_like: u16::try_from(parse_number(acts)?)
                    .map_err(|_| ArgsError("Acts Like value exceeds 16 bits".into()))?,
                source_page: u16::try_from(parse_number(page)?)
                    .map_err(|_| ArgsError("source page exceeds 16 bits".into()))?,
                palette_output: PathBuf::from(&args[9]),
                graphics_output: PathBuf::from(&args[10]),
                occupancy_output: PathBuf::from(&args[11]),
                page_output: PathBuf::from(&args[12]),
            }))
        }
        _ => None,
    })
}

pub(crate) fn parse_rgba_map16_import(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, _, _, _, row, acts, page, _, _, _, _] if command == "import-rgba-map16" => {
            Some(Command::ImportRgbaMap16(RgbaMap16ImportCommand {
                rgba: PathBuf::from(&args[1]),
                palette: PathBuf::from(&args[2]),
                palette_access: PathBuf::from(&args[3]),
                graphics: PathBuf::from(&args[4]),
                occupancy: PathBuf::from(&args[5]),
                palette_row: u8::try_from(parse_number(row)?)
                    .map_err(|_| ArgsError("palette row exceeds one byte".into()))?,
                acts_like: u16::try_from(parse_number(acts)?)
                    .map_err(|_| ArgsError("Acts Like value exceeds 16 bits".into()))?,
                source_page: u16::try_from(parse_number(page)?)
                    .map_err(|_| ArgsError("source page exceeds 16 bits".into()))?,
                palette_output: PathBuf::from(&args[9]),
                graphics_output: PathBuf::from(&args[10]),
                occupancy_output: PathBuf::from(&args[11]),
                page_output: PathBuf::from(&args[12]),
            }))
        }
        _ => None,
    })
}

pub(crate) fn parse_png_map16_import(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, _, _, _, row, acts, page, _, _, _, _] if command == "import-png-map16" => {
            Some(Command::ImportPngMap16(PngMap16ImportCommand {
                png: PathBuf::from(&args[1]),
                palette: PathBuf::from(&args[2]),
                palette_access: PathBuf::from(&args[3]),
                graphics: PathBuf::from(&args[4]),
                occupancy: PathBuf::from(&args[5]),
                palette_row: u8::try_from(parse_number(row)?)
                    .map_err(|_| ArgsError("palette row exceeds one byte".into()))?,
                acts_like: u16::try_from(parse_number(acts)?)
                    .map_err(|_| ArgsError("Acts Like value exceeds 16 bits".into()))?,
                source_page: u16::try_from(parse_number(page)?)
                    .map_err(|_| ArgsError("source page exceeds 16 bits".into()))?,
                palette_output: PathBuf::from(&args[9]),
                graphics_output: PathBuf::from(&args[10]),
                occupancy_output: PathBuf::from(&args[11]),
                page_output: PathBuf::from(&args[12]),
            }))
        }
        _ => None,
    })
}

pub(crate) fn parse_indexed_map16_import(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _, _, _, palette, acts_like, source_page, _, _, _]
            if command == "import-indexed-map16" =>
        {
            Some(Command::ImportIndexedMap16 {
                indices: PathBuf::from(&args[1]),
                graphics: PathBuf::from(&args[2]),
                occupancy: PathBuf::from(&args[3]),
                palette_row: u8::try_from(parse_number(palette)?)
                    .map_err(|_| ArgsError("palette row exceeds one byte".into()))?,
                acts_like: u16::try_from(parse_number(acts_like)?)
                    .map_err(|_| ArgsError("Acts Like value exceeds 16 bits".into()))?,
                source_page: u16::try_from(parse_number(source_page)?)
                    .map_err(|_| ArgsError("source page exceeds 16 bits".into()))?,
                graphics_output: PathBuf::from(&args[7]),
                occupancy_output: PathBuf::from(&args[8]),
                page_output: PathBuf::from(&args[9]),
            })
        }
        _ => None,
    })
}
