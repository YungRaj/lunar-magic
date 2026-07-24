use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::EventTilemapBuffers;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SmwUsV1EventTilemapStorage, load_smw_us_v1_event_tilemaps,
    smw_us_v1_event_tilemap_installation_plan, smw_us_v1_event_tilemap_locator,
    smw_us_v1_event_tilemap_update_policy,
};
use lm_project::{EventTilemapCompression, Project};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldEventTilemapExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldEventTilemapImport {
            input_rom,
            tilemaps,
            output_rom,
        } => import(input_rom, tilemaps, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("event-tilemap output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let loaded = load_smw_us_v1_event_tilemaps(&project)?;
    write_new(output, loaded.buffers.encode_native_file())?;
    println!("exported-native-event-tilemap-bytes: 6144");
    Ok(())
}

fn import(
    input_rom: &Path,
    tilemaps: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || tilemaps == output_rom {
        return Err("event-tilemap output must differ from every input".into());
    }
    let buffers = EventTilemapBuffers::decode_native_file(&read_bounded(
        tilemaps,
        EventTilemapBuffers::FILE_LEN,
    )?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let locator = smw_us_v1_event_tilemap_locator();
    match load_smw_us_v1_event_tilemaps(&project)?.storage {
        SmwUsV1EventTilemapStorage::Installed(compression) => {
            let update = smw_us_v1_event_tilemap_update_policy(project.rom.logical_len());
            project.save_event_tilemap_buffers_detected(
                &buffers,
                locator,
                compression,
                &update,
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )?;
        }
        SmwUsV1EventTilemapStorage::Pristine => {
            let compression = EventTilemapCompression::Lz2;
            let plan = smw_us_v1_event_tilemap_installation_plan(&buffers, compression);
            project.install_event_tilemap_buffers(&buffers, locator, compression, &plan)?;
        }
    }
    let reopened = load_smw_us_v1_event_tilemaps(&project)?;
    if reopened.buffers != buffers {
        return Err("event-tilemap semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-event-tilemap-bytes: 6144");
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
        return Err("event tilemaps require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
