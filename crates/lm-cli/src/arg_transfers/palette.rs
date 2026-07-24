use super::{Command, Cow, OsString, PaletteTransferCommand, PathBuf, parse_mapper, parse_number};

pub(crate) fn parse_palette_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, palette, pointers, colors, _] if command == "palette-export" => Ok(
            Some(Command::PaletteTransfer(PaletteTransferCommand::Export {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                palette: usize::try_from(parse_number(palette)?)?,
                pointer_table: usize::try_from(parse_number(pointers)?)?,
                colors: usize::try_from(parse_number(colors)?)?,
                output: PathBuf::from(&args[6]),
            })),
        ),
        [
            command,
            _,
            _,
            mapper,
            palette,
            pointers,
            colors,
            _,
            checksum,
            start,
            end,
        ] if command == "palette-import" => Ok(Some(Command::PaletteTransfer(
            PaletteTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                palette: usize::try_from(parse_number(palette)?)?,
                pointer_table: usize::try_from(parse_number(pointers)?)?,
                colors: usize::try_from(parse_number(colors)?)?,
                palette_file: PathBuf::from(&args[7]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: None,
            },
        ))),
        [
            command,
            _,
            _,
            mapper,
            palette,
            pointers,
            colors,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "palette-import-owned" => Ok(Some(Command::PaletteTransfer(
            PaletteTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                palette: usize::try_from(parse_number(palette)?)?,
                pointer_table: usize::try_from(parse_number(pointers)?)?,
                colors: usize::try_from(parse_number(colors)?)?,
                palette_file: PathBuf::from(&args[7]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: Some(PathBuf::from(&args[11])),
            },
        ))),
        _ => Ok(None),
    }
}
