use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_profile::{
    SmwUsV1Sprite19FixState, detect_smw_us_v1_sprite19_fix,
    smw_us_v1_sprite19_fix_installation_plan,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("sprite 19 fix output must differ from input".into());
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
        return Err("sprite 19 fix installer requires SMW US revision 0 LoROM".into());
    }
    let plan = smw_us_v1_sprite19_fix_installation_plan(project.rom.logical_bytes())?;
    project.install_relocatable_patch(&plan)?;
    if detect_smw_us_v1_sprite19_fix(project.rom.logical_bytes())?
        != SmwUsV1Sprite19FixState::Installed
    {
        return Err("installed sprite 19 fix did not authenticate".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("installed-sprite19-fix: writes={}", plan.writes.len());
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
