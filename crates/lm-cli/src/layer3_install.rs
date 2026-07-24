use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("Layer 3 output must differ from input".into());
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
        return Err("Layer 3 installer requires SMW US revision 0 LoROM".into());
    }
    let plans = lm_profile::smw_us_v1_complete_layer3_feature_plans()?;
    let results = project
        .install_relocatable_patch_group("install complete SMW US Layer 3 feature", &plans)?;
    write_new(output_rom, project.save_snapshot())?;
    let allocations = results
        .iter()
        .map(|result| result.blocks.len())
        .sum::<usize>();
    let writes = plans.iter().map(|plan| plan.writes.len()).sum::<usize>();
    println!("installed-layer3-runtime: allocations={allocations} writes={writes}");
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
