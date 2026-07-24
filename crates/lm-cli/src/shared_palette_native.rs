use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_expanded_shared_palette_installation_plan,
    smw_us_v1_shared_palette_layout,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwSharedPaletteExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwSharedPaletteImport {
            input_rom,
            palette,
            output_rom,
        } => import(input_rom, palette, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("shared palette output must differ from ROM input".into());
    }
    let palette = open_smw_us_v1(rom)?.load_shared_palette(smw_us_v1_shared_palette_layout())?;
    write_new(output, palette.encode())?;
    println!("exported-shared-palette: {:?}", palette.backend());
    Ok(())
}

fn import(
    input_rom: &Path,
    palette_path: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || palette_path == output_rom {
        return Err("shared palette ROM output must differ from every input".into());
    }
    let palette =
        SmwPaletteFile::decode(&read_bounded(palette_path, SmwPaletteFile::MAX_FILE_LEN)?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let layout = smw_us_v1_shared_palette_layout();
    let installed = project.load_shared_palette(layout)?.backend();
    if installed == SmwPaletteBackend::Legacy && palette.backend() == SmwPaletteBackend::Expanded {
        let expected = project
            .rom
            .read(layout.table_offset, SmwPaletteFile::EXPANDED_FILE_LEN)?
            .to_vec();
        let plan = smw_us_v1_expanded_shared_palette_installation_plan(&palette, &expected)?;
        project.install_relocatable_patch(&plan)?;
    } else {
        project.save_shared_palette(&palette, layout, SMW_US_V1_CHECKSUM_FIELD)?;
    }
    if project.load_shared_palette(smw_us_v1_shared_palette_layout())? != palette {
        return Err("shared palette semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-shared-palette: {:?}", palette.backend());
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
        return Err("shared palette commands require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
