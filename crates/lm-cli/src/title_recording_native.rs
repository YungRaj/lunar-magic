use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_title_recording_allocation_policy,
    smw_us_v1_title_recording_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use lm_title::{
    TitleScreenRecording, decode_snes9x_title_recording, decode_zsnes_title_recording,
    encode_zsnes_title_recording,
};
use std::path::Path;

const ZSNES_STATE_LEN: usize = 0x20c13;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwTitleRecordingExport { rom, output } => {
            let recording = load(rom)?;
            write_new(output, recording.encode_native_file())?;
            println!(
                "exported-title-recording-bytes: {}",
                recording.bytes().len()
            );
        }
        crate::command_types::Command::SmwTitleRecordingImport {
            input_rom,
            recording,
            output_rom,
        } => {
            let recording = TitleScreenRecording::decode_native_file(&read_bounded(
                recording,
                TitleScreenRecording::MAX_FILE_LEN,
            )?)?;
            install(input_rom, output_rom, &recording)?;
        }
        crate::command_types::Command::SmwTitleRecordingZsnesExport { rom, output } => {
            let recording = load(rom)?;
            write_new(output, encode_zsnes_title_recording(&recording))?;
            println!(
                "exported-title-recording-zst-bytes: {}",
                recording.bytes().len()
            );
        }
        crate::command_types::Command::SmwTitleRecordingZsnesImport {
            input_rom,
            state,
            output_rom,
        } => {
            let recording = decode_zsnes_title_recording(&read_bounded(state, ZSNES_STATE_LEN)?)?;
            install(input_rom, output_rom, &recording)?;
        }
        crate::command_types::Command::SmwTitleRecordingSnes9xImport {
            input_rom,
            state,
            output_rom,
        } => {
            let recording = decode_snes9x_title_recording(&read_bounded(state, 64 * 1024 * 1024)?)?;
            install(input_rom, output_rom, &recording)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn load(rom: &Path) -> Result<TitleScreenRecording, Box<dyn std::error::Error>> {
    let project = open_smw_us_v1(rom)?;
    project
        .load_title_recording_detected(&smw_us_v1_title_recording_locator())?
        .recording
        .ok_or_else(|| "ROM has no installed title-screen recording".into())
}

fn install(
    input_rom: &Path,
    output_rom: &Path,
    recording: &TitleScreenRecording,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom {
        return Err("title recording output must differ from ROM input".into());
    }
    let mut project = open_smw_us_v1(input_rom)?;
    let locator = smw_us_v1_title_recording_locator();
    let allocation = smw_us_v1_title_recording_allocation_policy(project.rom.logical_len());
    project.save_title_recording_detected(
        recording,
        &locator,
        &allocation,
        SMW_US_V1_CHECKSUM_FIELD,
        0xff,
    )?;
    if project
        .load_title_recording_detected(&locator)?
        .recording
        .as_ref()
        != Some(recording)
    {
        return Err("title recording semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!(
        "imported-title-recording-bytes: {}",
        recording.bytes().len()
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
        return Err("title recordings require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
