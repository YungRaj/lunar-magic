use crate::{
    atomic_output::write_new,
    oracle_input::{read_exact, read_rom},
};
use lm_oracle::observe_custom_overworld_sprites;
use lm_overworld::CUSTOM_OVERWORLD_SPRITE_ID_COUNT;
use lm_project::{NativeCustomOverworldSpriteRomLayout, Project};
use lm_rom::{Mapper, RomImage};
use std::path::Path;

pub(crate) fn observe(
    rom: &Path,
    location: (Mapper, usize),
    record_sizes: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output || record_sizes == output {
        return Err("native overworld sprite observation output must be a new path".into());
    }
    let sizes = read_exact(
        record_sizes,
        CUSTOM_OVERWORLD_SPRITE_ID_COUNT,
        "custom overworld sprite record-size table",
    )?;
    let sizes: [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT] = sizes
        .try_into()
        .map_err(|_| "record-size table has the wrong length")?;
    let project = Project::new(RomImage::from_bytes(read_rom(rom)?)?);
    let loaded = project.load_native_custom_overworld_sprites(
        NativeCustomOverworldSpriteRomLayout {
            mapper: location.0,
            pointer_offset: location.1,
            maximum_payload_len: 0x10000,
        },
        &sizes,
    )?;
    let observation = observe_custom_overworld_sprites(&loaded.table)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!(
        "observed-native-custom-overworld-sprites: {}",
        loaded.table.maps.iter().map(Vec::len).sum::<usize>()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        assert!(
            observe(
                Path::new("same"),
                (Mapper::LoRom, 0),
                Path::new("sizes"),
                Path::new("same")
            )
            .is_err()
        );
    }
}
