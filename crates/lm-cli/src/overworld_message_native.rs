use crate::{
    atomic_output::write_new,
    oracle_input::{read_bounded, read_rom},
};
use lm_overworld::{
    OverworldMessage, decode_native_overworld_message_file, encode_native_overworld_message_file,
};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED,
    SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET, load_smw_us_v1_overworld_messages,
    smw_us_v1_overworld_message_allocation_policy, smw_us_v1_overworld_message_installation_plan,
    smw_us_v1_overworld_message_patch_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

const MAX_MESSAGE_FILE_BYTES: usize = 10 + 512 * OverworldMessage::ENCODED_LEN;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldMessageExport { rom, output } => {
            export(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldMessageInstall {
            input_rom,
            messages,
            output_rom,
        } => install(input_rom, messages, output_rom)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn export(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("native overworld-message output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let loaded = load_smw_us_v1_overworld_messages(&project)?;
    write_new(
        output,
        encode_native_overworld_message_file(&loaded.messages)?,
    )?;
    println!(
        "exported-native-overworld-messages: {}",
        loaded.messages.len()
    );
    Ok(())
}

fn install(
    input_rom: &Path,
    messages: &Path,
    output_rom: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input_rom == output_rom || messages == output_rom {
        return Err("native overworld-message output must differ from every input".into());
    }
    let messages =
        decode_native_overworld_message_file(&read_bounded(messages, MAX_MESSAGE_FILE_BYTES)?)?;
    let mut project = open_smw_us_v1(input_rom)?;
    let hook = project
        .rom
        .read(
            SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
            SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED.len(),
        )?
        .to_vec();
    if hook == SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED {
        project.install_relocatable_patch(&smw_us_v1_overworld_message_installation_plan(
            &messages,
        )?)?;
    } else if hook.first() == Some(&0x22) {
        let loaded = project.load_expanded_overworld_messages_detected(
            smw_us_v1_overworld_message_patch_locator(),
        )?;
        project.save_installed_overworld_messages(
            &messages,
            &loaded.storage,
            smw_us_v1_overworld_message_patch_locator(),
            &smw_us_v1_overworld_message_allocation_policy(),
            SMW_US_V1_CHECKSUM_FIELD,
            0xff,
        )?;
    } else {
        return Err("native overworld-message hook is neither pristine nor recognized".into());
    }
    let reopened = project
        .load_expanded_overworld_messages_detected(smw_us_v1_overworld_message_patch_locator())?;
    if reopened.messages != messages {
        return Err("native overworld-message semantic reopen mismatch".into());
    }
    write_new(output_rom, project.save_snapshot())?;
    println!("installed-native-overworld-messages: {}", messages.len());
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
        return Err("native overworld messages require SMW US revision 0 LoROM".into());
    }
    Ok(project)
}
