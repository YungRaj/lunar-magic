use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::EventRevealTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_event_allocation_policy,
    smw_us_v1_overworld_event_reveal_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

const MAX_EVENT_FILE_BYTES: usize = 10 + EventRevealTable::MAX_ENTRIES * 4;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldEventExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldEventImport {
            input_rom,
            events,
            output_rom,
        } => import(input_rom, events, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld-event output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let table = project
        .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
        .table;
    write_new(output, table.encode_native_event_file()?)?;
    println!("exported-native-overworld-events: {}", table.entries.len());
    Ok(())
}

fn import(
    input_rom: &Path,
    events: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || events == output_rom {
        return Err("native overworld-event output must differ from every input".into());
    }
    let table =
        EventRevealTable::decode_native_event_file(&read_bounded(events, MAX_EVENT_FILE_BYTES)?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    project.save_overworld_event_reveals_detected(
        &table,
        smw_us_v1_overworld_event_reveal_locator(),
        &smw_us_v1_overworld_event_allocation_policy(),
        SMW_US_V1_CHECKSUM_FIELD,
        0xff,
    )?;
    if project
        .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
        .table
        != table
    {
        return Err("native overworld-event semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-overworld-events: {}", table.entries.len());
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
        return Err("native overworld events require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
