use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_profile::{
    SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
    smw_us_v1_expanded_settings_installation_plan_for_rom,
};
use lm_project::{ExpandedLevelSettingsLayout, Project};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("expanded-settings output must differ from input".into());
    }
    let mut project = Project::open_supported(RomImage::from_bytes(read_rom(input_rom)?)?)?;
    let identity = project
        .identity
        .as_ref()
        .ok_or("opened project has no detected identity")?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err("expanded-settings installer requires SMW US revision 0 LoROM".into());
    }
    let plan = smw_us_v1_expanded_settings_installation_plan_for_rom(&project.rom)?;
    let result = project.install_relocatable_patch_with_expansion_retry(
        &plan,
        SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN,
    )?;
    let block = result
        .blocks
        .first()
        .ok_or("expanded-settings installation produced no table allocation")?;
    let layout = ExpandedLevelSettingsLayout {
        mapper: Mapper::LoRom,
        table_offset: block.payload.start + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
        entries: SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
        stride: 0x20,
    };
    project.load_expanded_level_settings(0x207, layout)?;
    write_new(output_rom, project.save_snapshot())?;
    println!(
        "installed-expanded-settings: table={:#x} records={} writes={}",
        layout.table_offset,
        layout.entries,
        plan.writes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let same = Path::new("same");
        assert!(execute(same, same).is_err());
    }
}
