use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::NativeOverworldPlayerStarts;
use lm_profile::{SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_overworld_player_start_layout};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldStartExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldStartImport {
            input_rom,
            starts,
            output_rom,
        } => import(input_rom, starts, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld player-start output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let starts = project.load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())?;
    write_new(output, starts.encode_file()?)?;
    println!("exported-native-overworld-player-starts: 2");
    Ok(())
}

fn import(
    input_rom: &Path,
    starts_path: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || starts_path == output_rom {
        return Err("native overworld player-start output must differ from every input".into());
    }
    let starts = NativeOverworldPlayerStarts::decode_file(&read_bounded(
        starts_path,
        NativeOverworldPlayerStarts::FILE_LEN,
    )?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    project.save_overworld_player_starts(
        &starts,
        smw_us_v1_overworld_player_start_layout(),
        SMW_US_V1_CHECKSUM_FIELD,
    )?;
    if project.load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())? != starts {
        return Err("native overworld player-start semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("imported-native-overworld-player-starts: 2");
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
        return Err("native overworld player starts require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
