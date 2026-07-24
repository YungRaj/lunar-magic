use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::OverworldWarpLinkTable;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_warp_installation_plan,
    smw_us_v1_overworld_warp_link_layout, smw_us_v1_overworld_warp_patch_locator,
    smw_us_v1_overworld_warp_runtime_template, smw_us_v1_overworld_warp_update_policy,
};
use lm_project::{OverworldWarpLinkStorage, OverworldWarpPatchMigrationOptions, Project};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

const MAX_LINK_FILE_BYTES: usize = 12 + OverworldWarpLinkTable::MAX_LINKS * 8;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldWarpExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldWarpImport {
            input_rom,
            links,
            output_rom,
        } => import(input_rom, links, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

pub(crate) fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld warp output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let table = project
        .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?
        .table;
    write_new(output, table.encode_native_warp_file()?)?;
    println!(
        "exported-native-overworld-warp-links: {}",
        table.links.len()
    );
    Ok(())
}

pub(crate) fn import(
    input_rom: &Path,
    links: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || links == output_rom {
        return Err("native overworld warp output must differ from every input".into());
    }
    let table = OverworldWarpLinkTable::decode_native_warp_file(&read_bounded(
        links,
        MAX_LINK_FILE_BYTES,
    )?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let loaded =
        project.load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?;
    match loaded.storage {
        OverworldWarpLinkStorage::Fixed if table.links.len() == 27 => {
            project.save_overworld_warp_links(
                &table,
                smw_us_v1_overworld_warp_link_layout(),
                SMW_US_V1_CHECKSUM_FIELD,
            )?;
        }
        OverworldWarpLinkStorage::Fixed => {
            project
                .install_relocatable_patch(&smw_us_v1_overworld_warp_installation_plan(&table)?)?;
        }
        storage @ OverworldWarpLinkStorage::CurrentPatch { .. } => {
            let allocation = smw_us_v1_overworld_warp_update_policy(project.rom.logical_len());
            project.save_installed_overworld_warp_links(
                &table,
                storage,
                &allocation,
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )?;
        }
        storage @ OverworldWarpLinkStorage::LegacyPatch { .. } => {
            let allocation = smw_us_v1_overworld_warp_update_policy(project.rom.logical_len());
            let runtime = smw_us_v1_overworld_warp_runtime_template();
            project.migrate_legacy_overworld_warp_patch(
                &table,
                storage,
                OverworldWarpPatchMigrationOptions {
                    locator: smw_us_v1_overworld_warp_patch_locator(),
                    current_runtime: &runtime,
                    allocation: &allocation,
                    checksum_field: SMW_US_V1_CHECKSUM_FIELD,
                    fill: 0xff,
                },
            )?;
        }
    }
    let reopened = project
        .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?
        .table;
    if reopened != table {
        return Err("native overworld warp semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!(
        "imported-native-overworld-warp-links: {}",
        table.links.len()
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
        return Err("native overworld warp links require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let same = Path::new("same");
        assert!(export(same, same).is_err());
        assert!(import(same, Path::new("links"), same).is_err());
        assert!(import(Path::new("rom"), same, same).is_err());
    }
}
