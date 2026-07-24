use super::{
    Command, Cow, ExAnimationTransferCommand, OsString, PathBuf, parse_mapper, parse_number,
};

pub(crate) fn parse_exanimation_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, slot, pointers, records, encoded, _, _]
            if command == "exanimation-export" =>
        {
            Ok(Some(Command::ExAnimationTransfer(
                ExAnimationTransferCommand::Export {
                    rom: PathBuf::from(&args[1]),
                    mapper: parse_mapper(mapper)?,
                    slot: usize::try_from(parse_number(slot)?)?,
                    pointer_table: usize::try_from(parse_number(pointers)?)?,
                    maximum_records: usize::try_from(parse_number(records)?)?,
                    maximum_encoded_len: usize::try_from(parse_number(encoded)?)?,
                    size_modes: PathBuf::from(&args[7]),
                    output: PathBuf::from(&args[8]),
                },
            )))
        }
        [
            command,
            _,
            _,
            mapper,
            slot,
            pointers,
            records,
            encoded,
            _,
            _,
            checksum,
            start,
            end,
        ] if command == "exanimation-import" => Ok(Some(Command::ExAnimationTransfer(
            ExAnimationTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointers)?)?,
                maximum_records: usize::try_from(parse_number(records)?)?,
                maximum_encoded_len: usize::try_from(parse_number(encoded)?)?,
                size_modes: PathBuf::from(&args[8]),
                animation_file: PathBuf::from(&args[9]),
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
            slot,
            pointers,
            records,
            encoded,
            _,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "exanimation-import-owned" => Ok(Some(Command::ExAnimationTransfer(
            ExAnimationTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointers)?)?,
                maximum_records: usize::try_from(parse_number(records)?)?,
                maximum_encoded_len: usize::try_from(parse_number(encoded)?)?,
                size_modes: PathBuf::from(&args[8]),
                animation_file: PathBuf::from(&args[9]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: Some(PathBuf::from(&args[13])),
            },
        ))),
        _ => Ok(None),
    }
}
