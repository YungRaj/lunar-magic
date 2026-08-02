use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_profile::{
    SmwUsV1SupportPatchBState, detect_smw_us_v1_support_patch_b,
    smw_us_v1_support_patch_b_installation_plan,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("support patch B output must differ from input".into());
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
        return Err("support patch B installer requires SMW US revision 0 LoROM".into());
    }
    let plan = smw_us_v1_support_patch_b_installation_plan(project.rom.logical_bytes())?;
    project.install_relocatable_patch(&plan)?;
    if detect_smw_us_v1_support_patch_b(project.rom.logical_bytes())?
        != SmwUsV1SupportPatchBState::Installed
    {
        return Err("installed support patch B did not authenticate".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("installed-support-patch-b: writes={}", plan.writes.len());
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
