use crate::{atomic_output::write_new, oracle_input::read_rom};
use lm_oracle::{
    TransferOverworldDomains, observe_map16_remaps, observe_transfer_overworld,
    observe_transfer_overworld_events, observe_transferred_map16,
};
use lm_profile::{
    load_smw_us_v1_event_tilemaps, load_smw_us_v1_installed_map16_remaps,
    load_smw_us_v1_overworld_messages, load_smw_us_v1_overworld_settings,
    load_smw_us_v1_transferred_map16, smw_us_v1_boss_sequence_locator,
    smw_us_v1_overworld_event_number_map_locator, smw_us_v1_overworld_event_reveal_locator,
    smw_us_v1_overworld_level_name_locator, smw_us_v1_overworld_level_name_runtime,
    smw_us_v1_overworld_path_patch_locator, smw_us_v1_overworld_player_start_layout,
    smw_us_v1_overworld_warp_patch_locator, smw_us_v1_special_event_reveal_locator,
};
use lm_project::Project;
use lm_rom::{Mapper, Region, RomImage, SupportedGame};
use std::path::Path;

pub(crate) fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::SmwOverworldTransferObserve { rom, output } => {
            observe_events(rom, output)?;
        }
        crate::command_types::Command::SmwOverworldTransferFullObserve { rom, output } => {
            observe_full(rom, output)?;
        }
        crate::command_types::Command::SmwTransferredMap16Observe { rom, output } => {
            observe_map16(rom, output)?;
        }
        crate::command_types::Command::SmwInstalledMap16RemapsObserve { rom, output } => {
            observe_map16_remaps_installed(rom, output)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn observe_map16_remaps_installed(
    rom: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("installed Map16 remap observation output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let remaps = load_smw_us_v1_installed_map16_remaps(&project)?;
    let observation = observe_map16_remaps(&remaps.range_groups, &remaps.record_groups)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!(
        "observed-installed-map16-remaps: {} range records, {} grouped records",
        remaps.range_groups.iter().map(Vec::len).sum::<usize>(),
        remaps.record_groups.iter().map(Vec::len).sum::<usize>()
    );
    Ok(())
}

fn observe_map16(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("transferred Map16 observation output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let map16 = load_smw_us_v1_transferred_map16(&project)?;
    let observation = observe_transferred_map16(&map16.definitions, &map16.acts_like)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!(
        "observed-transferred-map16: {} definition words, {} acts-like entries",
        map16.definitions.len(),
        map16.acts_like.len()
    );
    Ok(())
}

fn observe_events(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err("overworld transfer observation output must differ from ROM input".into());
    }
    let project = open_smw_us_v1(rom)?;
    let reveals = project
        .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
        .table;
    let event_numbers = project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())?
        .map;
    let special = project
        .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())?
        .table;
    let tilemaps = load_smw_us_v1_event_tilemaps(&project)?.buffers;
    let observation =
        observe_transfer_overworld_events(&reveals, &event_numbers, &special, &tilemaps)?;
    write_new(output, observation.to_text().as_bytes())?;
    println!("observed-native-overworld-transfer-events: 4");
    Ok(())
}

fn observe_full(rom: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if rom == output {
        return Err(
            "complete overworld transfer observation output must differ from ROM input".into(),
        );
    }
    let project = open_smw_us_v1(rom)?;
    let map16 = load_smw_us_v1_transferred_map16(&project)?;
    let reveals = project
        .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())?
        .table;
    let event_numbers = project
        .load_overworld_event_number_map_detected(smw_us_v1_overworld_event_number_map_locator())?
        .map;
    let special = project
        .load_special_event_reveals_detected(smw_us_v1_special_event_reveal_locator())?
        .table;
    let tilemaps = load_smw_us_v1_event_tilemaps(&project)?.buffers;
    let paths = project
        .load_overworld_path_links_detected(smw_us_v1_overworld_path_patch_locator())?
        .table;
    let warps = project
        .load_overworld_warp_links_detected(smw_us_v1_overworld_warp_patch_locator())?
        .table;
    let level_names = project
        .load_overworld_level_names_detected(
            smw_us_v1_overworld_level_name_locator(),
            smw_us_v1_overworld_level_name_runtime(),
        )?
        .table;
    let player_starts =
        project.load_overworld_player_starts(smw_us_v1_overworld_player_start_layout())?;
    let settings = load_smw_us_v1_overworld_settings(&project)?.settings;
    let messages = load_smw_us_v1_overworld_messages(&project)?.messages;
    let boss_sequence = project
        .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())?
        .table;
    let observation = observe_transfer_overworld(TransferOverworldDomains {
        map16_definitions: &map16.definitions,
        map16_acts_like: &map16.acts_like,
        reveals: &reveals,
        event_numbers: &event_numbers,
        special: &special,
        tilemaps: &tilemaps,
        paths: &paths,
        warps: &warps,
        level_names: &level_names,
        player_starts: &player_starts,
        settings: &settings,
        messages: &messages,
        boss_sequence: &boss_sequence,
    })?;
    write_new(output, observation.to_text().as_bytes())?;
    println!("observed-native-overworld-transfer-domains: 13");
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
        return Err("overworld transfer observation requires SMW US revision 0 LoROM".into());
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        assert!(observe_events(Path::new("same"), Path::new("same")).is_err());
        assert!(observe_full(Path::new("same"), Path::new("same")).is_err());
    }

    #[test]
    fn wine_fixture_observes_all_four_recovered_event_domains() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = std::env::temp_dir().join(format!(
            "lm-overworld-transfer-observation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let output = directory.join("transfer.obs");
        observe_events(
            &root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"),
            &output,
        )
        .unwrap();
        let observation =
            lm_oracle::Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
        assert_eq!(
            observation.get("overworld/event-reveals/count"),
            Some("120")
        );
        assert_eq!(
            observation.get("overworld/event-number-map/count"),
            Some("96")
        );
        assert_eq!(
            observation.get("overworld/special-event-reveals/count"),
            Some("24")
        );
        assert_eq!(
            observation.get("overworld/event-tilemap/primary-bytes"),
            Some("4096")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn wine_fixture_observes_all_recovered_native_domains() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = std::env::temp_dir().join(format!(
            "lm-overworld-transfer-full-observation-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let output = directory.join("transfer-full.obs");
        observe_full(
            &root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"),
            &output,
        )
        .unwrap();
        let observation =
            lm_oracle::Observation::from_text(&fs::read_to_string(output).unwrap()).unwrap();
        for path in [
            "overworld/native-path-links/count",
            "overworld/native-warp-links/count",
            "overworld/native-level-names/count",
            "overworld/native-player-starts/count",
            "overworld/expanded-settings/count",
            "overworld/messages/count",
            "overworld/boss-sequence/message-count",
        ] {
            assert!(observation.get(path).is_some(), "missing {path}");
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
