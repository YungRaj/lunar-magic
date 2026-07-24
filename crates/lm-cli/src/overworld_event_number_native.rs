use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::EventNumberMap;
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_event_number_map_locator};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

const MAX_EVENT_MAP_FILE_BYTES: usize = 10 + EventNumberMap::ENTRY_COUNT;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldEventMapExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldEventMapImport {
            input_rom,
            event_map,
            output_rom,
        } => import(input_rom, event_map, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld event-map output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let map = project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())?
        .map;
    write_new(output, map.encode_native_file()?)?;
    println!("exported-native-overworld-event-map: {}", map.stored_len());
    Ok(())
}

fn import(
    input_rom: &Path,
    event_map: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || event_map == output_rom {
        return Err("native overworld event-map output must differ from every input".into());
    }
    let map =
        EventNumberMap::decode_native_file(&read_bounded(event_map, MAX_EVENT_MAP_FILE_BYTES)?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    project.save_overworld_event_number_map_detected(
        &map,
        smw_us_v1_overworld_event_number_map_locator(),
        SMW_US_V1_CHECKSUM_FIELD,
    )?;
    if project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())?
        .map
        != map
    {
        return Err("native overworld event-map semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-overworld-event-map: {}", map.stored_len());
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
        return Err("native overworld event maps require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
