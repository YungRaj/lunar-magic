use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::BossSequenceMessageTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_boss_sequence_locator,
    smw_us_v1_boss_sequence_update_policy,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldBossSequenceExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldBossSequenceImport {
            input_rom,
            messages,
            output_rom,
        } => import(input_rom, messages, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("boss-sequence output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let table = project
        .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())?
        .table;
    write_new(output, table.encode_native_file())?;
    println!("exported-native-boss-sequence-glyphs: 1344");
    Ok(())
}

fn import(
    input_rom: &Path,
    messages: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || messages == output_rom {
        return Err("boss-sequence output must differ from every input".into());
    }
    let table = BossSequenceMessageTable::decode_native_file(&read_bounded(
        messages,
        BossSequenceMessageTable::FILE_LEN,
    )?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let update = smw_us_v1_boss_sequence_update_policy(project.rom.logical_len());
    project.save_boss_sequence_messages_detected(
        &table,
        smw_us_v1_boss_sequence_locator(),
        &update,
        SMW_US_V1_CHECKSUM_FIELD,
        0xff,
    )?;
    if project
        .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())?
        .table
        != table
    {
        return Err("boss-sequence semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-boss-sequence-glyphs: 1344");
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
        return Err("boss-sequence messages require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
