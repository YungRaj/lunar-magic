use super::{
    Command, Cow, OsString, OverworldTransferCommand, PathBuf, parse_mapper, parse_number,
};

pub(crate) fn parse_overworld_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, slot, _, _, _] if command == "overworld-export" => Ok(Some(
            Command::OverworldTransfer(OverworldTransferCommand::Export {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                layout: PathBuf::from(&args[4]),
                size_modes: PathBuf::from(&args[5]),
                output: PathBuf::from(&args[6]),
            }),
        )),
        [command, _, _, mapper, slot, _, _, _, checksum, start, end]
            if command == "overworld-import" =>
        {
            Ok(Some(Command::OverworldTransfer(
                OverworldTransferCommand::Import {
                    input_rom: PathBuf::from(&args[1]),
                    output_rom: PathBuf::from(&args[2]),
                    mapper: parse_mapper(mapper)?,
                    slot: usize::try_from(parse_number(slot)?)?,
                    layout: PathBuf::from(&args[5]),
                    size_modes: PathBuf::from(&args[6]),
                    overworld_file: PathBuf::from(&args[7]),
                    checksum_field: usize::try_from(parse_number(checksum)?)?,
                    search_start: usize::try_from(parse_number(start)?)?,
                    search_end: usize::try_from(parse_number(end)?)?,
                    ownership_manifest: None,
                },
            )))
        }
        [
            command,
            _,
            _,
            mapper,
            slot,
            _,
            _,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "overworld-import-owned" => Ok(Some(Command::OverworldTransfer(
            OverworldTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                layout: PathBuf::from(&args[5]),
                size_modes: PathBuf::from(&args[6]),
                overworld_file: PathBuf::from(&args[7]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: Some(PathBuf::from(&args[11])),
            },
        ))),
        _ => Ok(None),
    }
}
