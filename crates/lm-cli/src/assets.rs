use crate::args::AssetCommand;
use crate::native_custom_overworld_sprites as native_sprites;
use crate::oracle_input::{read_exact, read_rom};
pub fn execute(command: AssetCommand) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        AssetCommand::Graphics {
            rom,
            mapper,
            file,
            pointer_table,
            maximum_compressed_len,
            maximum_decompressed_len,
            compression,
            observation,
        } => crate::graphics::inspect(
            read_rom(rom)?,
            crate::graphics::GraphicsInspectOptions {
                mapper,
                file,
                pointer_table,
                maximum_compressed_len,
                maximum_decompressed_len,
                compression,
                observation: observation.as_deref(),
            },
        ),
        AssetCommand::Palette {
            rom,
            mapper,
            index,
            pointer_table,
            colors,
            observation,
        } => crate::palette::inspect(
            read_rom(rom)?,
            mapper,
            index,
            pointer_table,
            colors,
            observation.as_deref(),
        ),
        AssetCommand::ExAnimation {
            rom,
            mapper,
            slot,
            pointer_table,
            maximum_records,
            maximum_encoded_len,
            size_modes,
            observation,
        } => crate::exanimation::inspect(
            read_rom(rom)?,
            mapper,
            slot,
            pointer_table,
            crate::exanimation::InspectOptions {
                maximum_records,
                maximum_encoded_len,
                size_mode_bytes: &read_exact(size_modes, 256, "ExAnimation size-mode table")?,
                observation: observation.as_deref(),
            },
        ),
        AssetCommand::OverworldMessages {
            rom,
            mapper,
            slot,
            pointer_table,
            count,
            observation,
        } => crate::overworld::inspect_messages(
            read_rom(rom)?,
            mapper,
            slot,
            pointer_table,
            count,
            observation.as_deref(),
        ),
        AssetCommand::OverworldSprites {
            rom,
            mapper,
            slot,
            pointer_table,
            count,
            record_len,
            observation,
        } => inspect_sprites(
            &rom,
            mapper,
            (slot, pointer_table, count, record_len),
            observation.as_deref(),
        ),
        AssetCommand::NativeCustomOverworldSprites {
            rom,
            mapper,
            pointer,
            record_sizes,
            observation,
        } => native_sprites::observe(&rom, (mapper, pointer), &record_sizes, &observation),
    }
}

fn inspect_sprites(
    rom: &std::path::Path,
    mapper: lm_rom::Mapper,
    layout: (usize, usize, usize, usize),
    observation: Option<&std::path::Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    crate::overworld::inspect_sprites(
        read_rom(rom)?,
        mapper,
        layout.0,
        layout.1,
        layout.2,
        layout.3,
        observation,
    )
}
