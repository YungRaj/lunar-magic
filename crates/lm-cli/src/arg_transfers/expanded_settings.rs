use super::{
    Command, Cow, ExpandedSettingsTransferCommand, OsString, PathBuf, parse_mapper, parse_number,
};

pub(crate) fn parse_expanded_settings_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, slot, table, entries, stride, _]
            if command == "expanded-settings-export" =>
        {
            Ok(Some(Command::ExpandedSettingsTransfer(
                ExpandedSettingsTransferCommand::Export {
                    rom: PathBuf::from(&args[1]),
                    mapper: parse_mapper(mapper)?,
                    slot: usize::try_from(parse_number(slot)?)?,
                    table_offset: usize::try_from(parse_number(table)?)?,
                    entries: usize::try_from(parse_number(entries)?)?,
                    stride: usize::try_from(parse_number(stride)?)?,
                    output: PathBuf::from(&args[7]),
                },
            )))
        }
        [
            command,
            _,
            _,
            mapper,
            slot,
            table,
            entries,
            stride,
            _,
            checksum,
        ] if command == "expanded-settings-import" => Ok(Some(Command::ExpandedSettingsTransfer(
            ExpandedSettingsTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                table_offset: usize::try_from(parse_number(table)?)?,
                entries: usize::try_from(parse_number(entries)?)?,
                stride: usize::try_from(parse_number(stride)?)?,
                record: PathBuf::from(&args[8]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
            },
        ))),
        _ => Ok(None),
    }
}
