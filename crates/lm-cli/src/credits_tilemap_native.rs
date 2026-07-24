use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::CreditsTilemap;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_credits_allocation_policy,
    smw_us_v1_credits_tilemap_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwCreditsTilemapExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwCreditsTilemapImport {
            input_rom,
            tilemap,
            output_rom,
        } => import(input_rom, tilemap, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("credits tilemap output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let tilemap = project
        .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())?
        .tilemap;
    write_new(output, tilemap.encode_native_file())?;
    println!("exported-credits-tilemap-words: 8192");
    Ok(())
}

fn import(
    input_rom: &Path,
    tilemap: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || tilemap == output_rom {
        return Err("credits tilemap output must differ from every input".into());
    }
    let tilemap =
        CreditsTilemap::decode_native_file(&read_bounded(tilemap, CreditsTilemap::FILE_LEN)?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let locator = smw_us_v1_credits_tilemap_locator();
    let allocation = smw_us_v1_credits_allocation_policy(project.rom.logical_len());
    project.save_credits_tilemap_detected(
        &tilemap,
        &locator,
        &allocation,
        SMW_US_V1_CHECKSUM_FIELD,
        0xff,
    )?;
    if project.load_credits_tilemap_detected(&locator)?.tilemap != tilemap {
        return Err("credits tilemap semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-credits-tilemap-words: 8192");
    Ok(())
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
        return Err("credits tilemaps require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
