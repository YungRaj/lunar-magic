use super::{Command, Cow, Map16TransferCommand, OsString, PathBuf, parse_mapper, parse_number};

pub(crate) fn parse_map16_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, page, graphics, acts_like, _] if command == "map16-export" => {
            Ok(Some(Command::Map16Transfer(Map16TransferCommand::Export {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                page: usize::try_from(parse_number(page)?)?,
                graphics_table: usize::try_from(parse_number(graphics)?)?,
                acts_like_table: usize::try_from(parse_number(acts_like)?)?,
                output: PathBuf::from(&args[6]),
            })))
        }
        [
            command,
            _,
            _,
            mapper,
            page,
            graphics,
            acts_like,
            _,
            checksum,
            start,
            end,
        ] if command == "map16-import" => {
            Ok(Some(Command::Map16Transfer(Map16TransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                page: usize::try_from(parse_number(page)?)?,
                graphics_table: usize::try_from(parse_number(graphics)?)?,
                acts_like_table: usize::try_from(parse_number(acts_like)?)?,
                page_file: PathBuf::from(&args[7]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: None,
            })))
        }
        [
            command,
            _,
            _,
            mapper,
            page,
            graphics,
            acts_like,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "map16-import-owned" => {
            Ok(Some(Command::Map16Transfer(Map16TransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                page: usize::try_from(parse_number(page)?)?,
                graphics_table: usize::try_from(parse_number(graphics)?)?,
                acts_like_table: usize::try_from(parse_number(acts_like)?)?,
                page_file: PathBuf::from(&args[7]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: Some(PathBuf::from(&args[11])),
            })))
        }
        _ => Ok(None),
    }
}
