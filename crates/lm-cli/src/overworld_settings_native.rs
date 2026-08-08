use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_level::ExpandedOverworldSettings;
use lm_oracle::observe_overworld_layer3_settings;
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
    load_smw_us_v1_overworld_layer3_settings, load_smw_us_v1_overworld_settings,
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings,
    smw_us_v1_installed_expanded_settings_layout,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldSettingsExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldSettingsImport {
            input_rom,
            settings,
            output_rom,
        } => import(input_rom, settings, output_rom)?,
        crate::command_types::Command::SmwOverworldLayer3SettingsObserve { rom, output } => {
            observe_layer3(rom, output)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn observe_layer3(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("overworld Layer 3 observation output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let loaded = load_smw_us_v1_overworld_layer3_settings(&project)?;
    let observation = observe_overworld_layer3_settings(&loaded.settings)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!(
        "observed-native-overworld-layer3-settings: 7 installed={}",
        loaded.installed
    );
    Ok(())
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld settings output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let settings = load_or_default(&project)?;
    write_new(output, settings.encode_file())?;
    println!("exported-native-overworld-settings: 7");
    Ok(())
}

fn import(
    input_rom: &Path,
    settings_path: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || settings_path == output_rom {
        return Err("native overworld settings output must differ from every input".into());
    }
    let settings = ExpandedOverworldSettings::decode_file(&read_bounded(
        settings_path,
        ExpandedOverworldSettings::ENCODED_LEN,
    )?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    if let Some(layout) = smw_us_v1_installed_expanded_settings_layout(&project)? {
        project.save_expanded_overworld_settings(
            SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
            &settings,
            layout,
            SMW_US_V1_CHECKSUM_FIELD,
        )?;
    } else {
        project.install_relocatable_patch(
            &smw_us_v1_expanded_settings_installation_plan_with_overworld_settings(Some(
                &settings,
            ))?,
        )?;
    }
    if load_or_default(&project)? != settings {
        return Err("native overworld settings semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-overworld-settings: 7");
    Ok(())
}

fn load_or_default(
    project: &Project,
) -> Result<ExpandedOverworldSettings, Box<dyn std::error::Error>> {
    Ok(load_smw_us_v1_overworld_settings(project)?.settings)
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
        return Err("native overworld settings require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
