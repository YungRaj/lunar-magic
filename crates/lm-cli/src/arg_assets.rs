use crate::arg_values::{parse_graphics_compression, parse_mapper, parse_number};
use crate::command_types::{AssetCommand, Command};
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

#[allow(clippy::too_many_lines)] // One exhaustive positional grammar keeps ambiguity visible.
pub(crate) fn parse_asset_command(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [command, _, mapper, pointer, _] if command == "exanimation-slot-options" => {
            Ok(Some(Command::ExAnimationSlotOptionsObserve {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                pointer: usize::try_from(parse_number(pointer)?)?,
                output: PathBuf::from(&args[4]),
            }))
        }
        [
            command,
            _,
            mapper,
            file,
            pointer_table,
            compressed,
            decompressed,
        ] if command == "graphics" => Ok(Some(Command::Asset(AssetCommand::Graphics {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            file: usize::try_from(parse_number(file)?)?,
            pointer_table: usize::try_from(parse_number(pointer_table)?)?,
            maximum_compressed_len: usize::try_from(parse_number(compressed)?)?,
            maximum_decompressed_len: usize::try_from(parse_number(decompressed)?)?,
            compression: lm_project::GraphicsCompression::Lz2,
            observation: None,
        }))),
        [
            command,
            _,
            mapper,
            file,
            pointer_table,
            compressed,
            decompressed,
            last,
        ] if command == "graphics" && !matches!(last.as_ref(), "lz2" | "lz3") => {
            Ok(Some(Command::Asset(AssetCommand::Graphics {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                file: usize::try_from(parse_number(file)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                maximum_compressed_len: usize::try_from(parse_number(compressed)?)?,
                maximum_decompressed_len: usize::try_from(parse_number(decompressed)?)?,
                compression: lm_project::GraphicsCompression::Lz2,
                observation: Some(PathBuf::from(&args[7])),
            })))
        }
        [
            command,
            _,
            mapper,
            file,
            pointer_table,
            compressed,
            decompressed,
            compression,
        ] if command == "graphics" => Ok(Some(Command::Asset(AssetCommand::Graphics {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            file: usize::try_from(parse_number(file)?)?,
            pointer_table: usize::try_from(parse_number(pointer_table)?)?,
            maximum_compressed_len: usize::try_from(parse_number(compressed)?)?,
            maximum_decompressed_len: usize::try_from(parse_number(decompressed)?)?,
            compression: parse_graphics_compression(compression)?,
            observation: None,
        }))),
        [
            command,
            _,
            mapper,
            file,
            pointer_table,
            compressed,
            decompressed,
            compression,
            _,
        ] if command == "graphics" => Ok(Some(Command::Asset(AssetCommand::Graphics {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            file: usize::try_from(parse_number(file)?)?,
            pointer_table: usize::try_from(parse_number(pointer_table)?)?,
            maximum_compressed_len: usize::try_from(parse_number(compressed)?)?,
            maximum_decompressed_len: usize::try_from(parse_number(decompressed)?)?,
            compression: parse_graphics_compression(compression)?,
            observation: Some(PathBuf::from(&args[8])),
        }))),
        [command, _, mapper, index, pointer_table, colors] if command == "palette" => {
            Ok(Some(Command::Asset(AssetCommand::Palette {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                index: usize::try_from(parse_number(index)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                colors: usize::try_from(parse_number(colors)?)?,
                observation: None,
            })))
        }
        [command, _, mapper, index, pointer_table, colors, _] if command == "palette" => {
            Ok(Some(Command::Asset(AssetCommand::Palette {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                index: usize::try_from(parse_number(index)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                colors: usize::try_from(parse_number(colors)?)?,
                observation: Some(PathBuf::from(&args[6])),
            })))
        }
        [
            command,
            _,
            mapper,
            slot,
            pointer_table,
            records,
            encoded_len,
            _,
        ] if command == "exanimation" => Ok(Some(Command::Asset(AssetCommand::ExAnimation {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            slot: usize::try_from(parse_number(slot)?)?,
            pointer_table: usize::try_from(parse_number(pointer_table)?)?,
            maximum_records: usize::try_from(parse_number(records)?)?,
            maximum_encoded_len: usize::try_from(parse_number(encoded_len)?)?,
            size_modes: PathBuf::from(&args[7]),
            observation: None,
        }))),
        [
            command,
            _,
            mapper,
            slot,
            pointer_table,
            records,
            encoded_len,
            _,
            _,
        ] if command == "exanimation" => Ok(Some(Command::Asset(AssetCommand::ExAnimation {
            rom: PathBuf::from(&args[1]),
            mapper: parse_mapper(mapper)?,
            slot: usize::try_from(parse_number(slot)?)?,
            pointer_table: usize::try_from(parse_number(pointer_table)?)?,
            maximum_records: usize::try_from(parse_number(records)?)?,
            maximum_encoded_len: usize::try_from(parse_number(encoded_len)?)?,
            size_modes: PathBuf::from(&args[7]),
            observation: Some(PathBuf::from(&args[8])),
        }))),
        [command, _, mapper, slot, pointer_table, count] if command == "overworld-messages" => {
            Ok(Some(Command::Asset(AssetCommand::OverworldMessages {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                count: usize::try_from(parse_number(count)?)?,
                observation: None,
            })))
        }
        [command, _, mapper, slot, pointer_table, count, _] if command == "overworld-messages" => {
            Ok(Some(Command::Asset(AssetCommand::OverworldMessages {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                count: usize::try_from(parse_number(count)?)?,
                observation: Some(PathBuf::from(&args[6])),
            })))
        }
        [command, _, mapper, slot, pointer_table, count, record_len]
            if command == "overworld-sprites" =>
        {
            Ok(Some(Command::Asset(AssetCommand::OverworldSprites {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                count: usize::try_from(parse_number(count)?)?,
                record_len: usize::try_from(parse_number(record_len)?)?,
                observation: None,
            })))
        }
        [
            command,
            _,
            mapper,
            slot,
            pointer_table,
            count,
            record_len,
            _,
        ] if command == "overworld-sprites" => {
            Ok(Some(Command::Asset(AssetCommand::OverworldSprites {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                slot: usize::try_from(parse_number(slot)?)?,
                pointer_table: usize::try_from(parse_number(pointer_table)?)?,
                count: usize::try_from(parse_number(count)?)?,
                record_len: usize::try_from(parse_number(record_len)?)?,
                observation: Some(PathBuf::from(&args[7])),
            })))
        }
        [command, _, mapper, pointer, _, _] if command == "native-overworld-sprites" => Ok(Some(
            Command::Asset(AssetCommand::NativeCustomOverworldSprites {
                rom: PathBuf::from(&args[1]),
                mapper: parse_mapper(mapper)?,
                pointer: usize::try_from(parse_number(pointer)?)?,
                record_sizes: PathBuf::from(&args[4]),
                observation: PathBuf::from(&args[5]),
            }),
        )),
        _ => Ok(None),
    }
}
