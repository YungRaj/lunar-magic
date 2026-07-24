use crate::{
    atomic_output::write_new,
    oracle_input::{read_exact, read_rom},
};
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_lunar_magic_metadata_layout};
use lm_project::Project;
use lm_rom::{LunarMagicRomMetadata, Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwLunarMagicMetadataExport { rom, output } => {
            let metadata = load(rom)?;
            write_new(output, metadata.encode_file())?;
            println!(
                "exported-lunar-magic-metadata-bytes: {}",
                LunarMagicRomMetadata::FILE_LEN
            );
        }
        crate::command_types::Command::SmwLunarMagicMetadataImport {
            input_rom,
            metadata,
            output_rom,
        } => {
            if input_rom == output_rom {
                return Err("metadata output must differ from ROM input".into());
            }
            let metadata = LunarMagicRomMetadata::decode_file(&read_exact(
                metadata,
                LunarMagicRomMetadata::FILE_LEN,
                "Lunar Magic metadata file",
            )?)?;
            let mut project = open_smw_us_v1(input_rom)?;
            let layout = smw_us_v1_lunar_magic_metadata_layout();
            project.save_lunar_magic_rom_metadata(&metadata, layout, SMW_US_V1_CHECKSUM_FIELD)?;
            if project.load_lunar_magic_rom_metadata(layout)?.as_ref() != Some(&metadata) {
                return Err("Lunar Magic metadata semantic reopen mismatch".into());
            }
            write_new(output_rom, project.save_snapshot())?;
            println!(
                "imported-lunar-magic-metadata-bytes: {}",
                LunarMagicRomMetadata::FILE_LEN
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn load(path: &Path) -> Result<LunarMagicRomMetadata, Box<dyn std::error::Error>> {
    open_smw_us_v1(path)?
        .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())?
        .ok_or_else(|| "ROM has no installed Lunar Magic metadata".into())
}

fn open_smw_us_v1(path: &Path) -> Result<Project, Box<dyn std::error::Error>> {
    let project = Project::open_supported(RomImage::from_bytes(read_rom(path)?)?)?;
    let identity = project
        .identity
        .as_ref()
        .ok_or("opened project has no detected identity")?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err("Lunar Magic metadata requires SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
