use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::{NativeOverworldLevelNameTable, OverworldMetadata};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_level_name_allocation_policy,
    smw_us_v1_overworld_level_name_installation_plan, smw_us_v1_overworld_level_name_locator,
    smw_us_v1_overworld_level_name_runtime,
};
use lm_project::{OverworldLevelNameStorage, Project};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldNameExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldNameImport {
            input_rom,
            names,
            output_rom,
        } => import(input_rom, names, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld name output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let names = project
        .load_overworld_level_names_detected(
            smw_us_v1_overworld_level_name_locator(),
            smw_us_v1_overworld_level_name_runtime(),
        )?
        .table
        .names;
    let count = names.len();
    write_new(
        output,
        OverworldMetadata {
            level_names: names,
            ..OverworldMetadata::default()
        }
        .encode_file()?,
    )?;
    println!("exported-native-overworld-level-names: {count}");
    Ok(())
}

fn import(
    input_rom: &Path,
    names: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || names == output_rom {
        return Err("native overworld name output must differ from every input".into());
    }
    let metadata =
        OverworldMetadata::decode_file(&read_bounded(names, OverworldMetadata::MAX_FILE_LEN)?)?;
    if !metadata.player_starts.is_empty() || !metadata.submap_settings.is_empty() {
        return Err(
            "native level-name import requires an LMOWMETA file containing names only".into(),
        );
    }
    let table = NativeOverworldLevelNameTable {
        names: metadata.level_names,
    };
    table.encode()?;
    if table.names.is_empty() {
        return Err("native overworld name table cannot be empty".into());
    }
    let mut project = open_smw_us_v1(input_rom)?;
    let loaded = project.load_overworld_level_names_detected(
        smw_us_v1_overworld_level_name_locator(),
        smw_us_v1_overworld_level_name_runtime(),
    )?;
    match loaded.storage {
        OverworldLevelNameStorage::Vanilla => {
            project.install_relocatable_patch(
                &smw_us_v1_overworld_level_name_installation_plan(&table)?,
            )?;
        }
        storage @ OverworldLevelNameStorage::Expanded { .. } => {
            project.save_installed_overworld_level_names(
                &table,
                storage,
                Mapper::LoRom,
                &smw_us_v1_overworld_level_name_allocation_policy(),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )?;
        }
    }
    let reopened = project
        .load_overworld_level_names_detected(
            smw_us_v1_overworld_level_name_locator(),
            smw_us_v1_overworld_level_name_runtime(),
        )?
        .table;
    if reopened != table {
        return Err("native overworld name semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!(
        "imported-native-overworld-level-names: {}",
        table.names.len()
    );
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
        return Err("native overworld level names require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
