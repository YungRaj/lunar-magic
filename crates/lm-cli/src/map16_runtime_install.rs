use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_profile::{
    detect_smw_us_v1_current_map16_runtime, load_smw_us_v1_secondary_map16,
    smw_us_v1_map16_runtime_installation_plan,
};
use lm_project::Project;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute(
    input_rom: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("Map16 runtime output must differ from input".into());
    }
    let mut project = Project::open_supported(RomImage::from_bytes(read_rom(input_rom)?)?)?;
    let identity = project
        .identity
        .clone()
        .ok_or("opened project has no detected identity")?;
    if identity.game != SupportedGame::SuperMarioWorld
        || identity.region != Region::NorthAmerica
        || identity.revision != 0
        || identity.mapper != Mapper::LoRom
    {
        return Err("Map16 runtime installer requires SMW US revision 0 LoROM".into());
    }
    let checksum_field = identity.internal_header_offset + 0x1c;
    let plan = smw_us_v1_map16_runtime_installation_plan(
        project.rom.logical_bytes(),
        AllocationPolicy {
            search: 0x80_000..0x10_0000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0, 0xff],
            protected: vec![ProtectedRange(
                identity.internal_header_offset..identity.internal_header_offset + 0x40,
            )],
        },
        checksum_field,
    )?;
    let result = project.install_relocatable_patch(&plan)?;
    let secondary = load_smw_us_v1_secondary_map16(&project)?;
    if !secondary.installed {
        return Err("installed Map16 runtime did not reopen".into());
    }
    let authenticated = detect_smw_us_v1_current_map16_runtime(project.rom.logical_bytes())?
        .ok_or("installed Map16 runtime did not authenticate")?;
    if authenticated != result.blocks[0] {
        return Err("authenticated Map16 auxiliary allocation does not match installation".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!(
        "installed-map16-runtime: auxiliary={:#x}..{:#x} writes={}",
        result.blocks[0].payload.start,
        result.blocks[0].payload.end,
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
