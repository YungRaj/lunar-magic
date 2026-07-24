use crate::arg_values::{parse_graphics_compression, parse_mapper, parse_number};
use crate::command_types::{Command, GraphicsMigrationCommand, GraphicsTransferCommand};
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

#[allow(clippy::too_many_lines)] // Keeps legacy and codec-explicit positional forms side by side.
pub(crate) fn parse_graphics_transfer(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    match text {
        [
            command,
            _,
            _,
            mapper,
            pointers,
            entries,
            compressed,
            decompressed,
            source,
            target,
            checksum,
            start,
            end,
        ] if command == "graphics-recompress" => {
            Ok(Some(Command::GraphicsMigration(GraphicsMigrationCommand {
                input_rom: PathBuf::from(&args[1]),
                output_rom: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                pointer_table: number(pointers)?,
                entries: number(entries)?,
                maximum_compressed_len: number(compressed)?,
                maximum_decompressed_len: number(decompressed)?,
                source_compression: parse_graphics_compression(source)?,
                target_compression: parse_graphics_compression(target)?,
                checksum_field: number(checksum)?,
                search_start: number(start)?,
                search_end: number(end)?,
            })))
        }
        [
            command,
            _,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            _,
        ] if command == "graphics-export" => Ok(Some(export(
            args,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            lm_project::GraphicsCompression::Lz2,
            7,
        )?)),
        [
            command,
            _,
            _,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            _,
            checksum,
            start,
            end,
        ] if command == "graphics-import" => Ok(Some(import(
            args,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            lm_project::GraphicsCompression::Lz2,
            8,
            checksum,
            start,
            end,
            None,
        )?)),
        [
            command,
            _,
            _,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            compression,
            _,
            checksum,
            start,
            end,
            _,
        ] if command == "graphics-import-owned" => Ok(Some(import(
            args,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            parse_graphics_compression(compression)?,
            9,
            checksum,
            start,
            end,
            Some(13),
        )?)),
        [
            command,
            _,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            compression,
            _,
        ] if command == "graphics-export" => Ok(Some(export(
            args,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            parse_graphics_compression(compression)?,
            8,
        )?)),
        [
            command,
            _,
            _,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            compression,
            _,
            checksum,
            start,
            end,
        ] if command == "graphics-import" => Ok(Some(import(
            args,
            mapper,
            slot,
            pointers,
            compressed,
            decompressed,
            parse_graphics_compression(compression)?,
            9,
            checksum,
            start,
            end,
            None,
        )?)),
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn export(
    args: &[OsString],
    mapper: &str,
    slot: &str,
    pointers: &str,
    compressed: &str,
    decompressed: &str,
    compression: lm_project::GraphicsCompression,
    output_index: usize,
) -> Result<Command, Box<dyn std::error::Error>> {
    Ok(Command::GraphicsTransfer(GraphicsTransferCommand::Export {
        rom: PathBuf::from(&args[1]),
        mapper: parse_mapper(mapper)?,
        slot: number(slot)?,
        pointer_table: number(pointers)?,
        maximum_compressed_len: number(compressed)?,
        maximum_decompressed_len: number(decompressed)?,
        compression,
        output: PathBuf::from(&args[output_index]),
    }))
}

#[allow(clippy::too_many_arguments)]
fn import(
    args: &[OsString],
    mapper: &str,
    slot: &str,
    pointers: &str,
    compressed: &str,
    decompressed: &str,
    compression: lm_project::GraphicsCompression,
    graphics_index: usize,
    checksum: &str,
    start: &str,
    end: &str,
    ownership_index: Option<usize>,
) -> Result<Command, Box<dyn std::error::Error>> {
    Ok(Command::GraphicsTransfer(GraphicsTransferCommand::Import {
        input_rom: PathBuf::from(&args[1]),
        output_rom: PathBuf::from(&args[2]),
        mapper: parse_mapper(mapper)?,
        slot: number(slot)?,
        pointer_table: number(pointers)?,
        maximum_compressed_len: number(compressed)?,
        maximum_decompressed_len: number(decompressed)?,
        compression,
        graphics_file: PathBuf::from(&args[graphics_index]),
        checksum_field: number(checksum)?,
        search_start: number(start)?,
        search_end: number(end)?,
        ownership_manifest: ownership_index.map(|index| PathBuf::from(&args[index])),
    }))
}

fn number(value: &str) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(usize::try_from(parse_number(value)?)?)
}
