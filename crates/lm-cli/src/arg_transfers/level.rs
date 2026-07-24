use crate::arg_values::{parse_mapper, parse_number, parse_sprite_format};
use crate::command_types::{Command, LevelTransferCommand};
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_level_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [
            command,
            _,
            mapper,
            level,
            layer1,
            sprites,
            format,
            lengths,
            _,
        ] if command == "level-export" => {
            Ok(Some(Command::LevelTransfer(LevelTransferCommand::Export {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                level: usize::try_from(parse_number(level)?)?,
                layer1_table: usize::try_from(parse_number(layer1)?)?,
                sprite_table: usize::try_from(parse_number(sprites)?)?,
                expanded_sprites: parse_sprite_format(format)?,
                sprite_lengths: optional_length_table(&args[7], lengths),
                output: PathBuf::from(&args[8]),
            })))
        }
        [
            command,
            _,
            _,
            mapper,
            level,
            layer1,
            sprites,
            format,
            lengths,
            _,
            checksum,
            start,
            end,
        ] if command == "level-import" => {
            Ok(Some(Command::LevelTransfer(LevelTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                level: usize::try_from(parse_number(level)?)?,
                layer1_table: usize::try_from(parse_number(layer1)?)?,
                sprite_table: usize::try_from(parse_number(sprites)?)?,
                expanded_sprites: parse_sprite_format(format)?,
                sprite_lengths: optional_length_table(&args[8], lengths),
                level_file: PathBuf::from(&args[9]),
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
            level,
            layer1,
            sprites,
            format,
            lengths,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "level-import-owned" => {
            Ok(Some(Command::LevelTransfer(LevelTransferCommand::Import {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                level: usize::try_from(parse_number(level)?)?,
                layer1_table: usize::try_from(parse_number(layer1)?)?,
                sprite_table: usize::try_from(parse_number(sprites)?)?,
                expanded_sprites: parse_sprite_format(format)?,
                sprite_lengths: optional_length_table(&args[8], lengths),
                level_file: PathBuf::from(&args[9]),
                checksum_field: usize::try_from(parse_number(checksum)?)?,
                search_start: usize::try_from(parse_number(start)?)?,
                search_end: usize::try_from(parse_number(end)?)?,
                ownership_manifest: Some(PathBuf::from(&args[13])),
            })))
        }
        _ => Ok(None),
    }
}

fn optional_length_table(value: &OsString, text: &str) -> Option<PathBuf> {
    (text != "standard").then(|| PathBuf::from(value))
}
