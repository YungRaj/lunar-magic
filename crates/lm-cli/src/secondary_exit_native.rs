use crate::{
    atomic_output::write_new,
    oracle_input::{read_exact, read_rom},
};
use lm_level::SecondaryExitTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_builtin_secondary_exit_installation_plan_from_source,
    smw_us_v1_secondary_exit_allocation_policy, smw_us_v1_secondary_exit_locator,
};
use lm_project::{Project, SecondaryExitStorage};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwSecondaryExitExport { rom, output } => {
            let loaded = open_smw_us_v1(rom)?
                .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())?;
            write_new(output, loaded.table.encode_native_file()?)?;
            println!("exported-native-secondary-exits: 8192");
        }
        crate::command_types::Command::SmwSecondaryExitImport {
            input_rom,
            table,
            output_rom,
        } => {
            if input_rom == output_rom || table == output_rom {
                return Err("secondary-exit output must differ from every input".into());
            }
            let table = SecondaryExitTable::decode_native_file(&read_exact(
                table,
                SecondaryExitTable::FILE_LEN,
                "secondary-exit table",
            )?)?;
            let mut project = open_smw_us_v1(input_rom)?;
            let locator = smw_us_v1_secondary_exit_locator();
            let loaded = project.load_secondary_exit_table_detected(locator)?;
            match loaded.storage {
                SecondaryExitStorage::Pristine => {
                    project.install_relocatable_patch(
                        &smw_us_v1_builtin_secondary_exit_installation_plan_from_source(
                            &loaded.table,
                            &table,
                        )?,
                    )?;
                }
                SecondaryExitStorage::Installed { .. } => {
                    project.save_installed_secondary_exit_table(
                        &table,
                        locator,
                        &smw_us_v1_secondary_exit_allocation_policy(project.rom.logical_len()),
                        SMW_US_V1_CHECKSUM_FIELD,
                        0xff,
                    )?;
                }
            }
            if project.load_secondary_exit_table_detected(locator)?.table != table {
                return Err("secondary-exit semantic reopen mismatch".into());
            }
            write_new(output_rom, project.save_snapshot())?;
            println!("imported-native-secondary-exits: 8192");
        }
        _ => return Ok(false),
    }
    Ok(true)
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
        return Err("secondary exits require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
