use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_native_level_assets;
use lm_profile::RevisionProfile;
use lm_project::NativeLevelAssetsFile;
use std::fs;
use std::path::Path;

pub fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    let crate::command_types::Command::NativeAssetsFile {
        input,
        profile,
        normalized_output,
        observation,
    } = command
    else {
        return Ok(false);
    };
    execute(
        input,
        profile,
        normalized_output.as_deref(),
        observation.as_deref(),
    )?;
    Ok(true)
}

pub fn execute(
    input: &Path,
    profile_path: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let outputs = [normalized_output, observation]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if outputs
        .iter()
        .any(|output| [input, profile_path].contains(output))
    {
        return Err("native-assets outputs must differ from both inputs".into());
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized native-assets and observation outputs must differ".into());
    }
    let profile = RevisionProfile::read_from(fs::File::open(profile_path)?)?;
    let file = NativeLevelAssetsFile::decode(
        &read_bounded(input, NativeLevelAssetsFile::MAX_FILE_LEN)?,
        &profile.sprite_lengths,
        profile.exanimation.maximum_records,
        &profile.exanimation_double_size_modes,
    )?;
    let normalized = normalized_output
        .map(|_| file.encode(&profile.exanimation_double_size_modes))
        .transpose()?;
    let observed = observation.map(|_| observe_native_level_assets(&file).to_text());
    let mut publications: Vec<(&Path, &[u8])> = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized_output, normalized.as_ref()) {
        publications.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        publications.push((path, text.as_bytes()));
    }
    write_new_batch(&publications)?;
    println!("native-assets-slot: {:#05x}", file.source_slot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_level::{
        ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream, SpriteLengthTable,
    };
    use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lm-native-assets-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn normalization_and_field_complete_observation_publish_together() {
        let input = temporary("input.lmna");
        let profile_path = temporary("profile.lmrev");
        let normalized = temporary("normalized.lmna");
        let observation = temporary("assets.obs");
        let profile = lm_profile::test_support::profile();
        let file = NativeLevelAssetsFile {
            source_slot: 0x105,
            assets: LoadedNativeLevelAssets {
                level: LoadedLevelSlot {
                    number: 0x105,
                    layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
                    sprites: NativeSpriteStream::parse(
                        &[0x10, 0, 1, 2, 0xff, 0xfe],
                        true,
                        &SpriteLengthTable::standard(),
                    )
                    .unwrap(),
                },
                palette: Palette {
                    colors: vec![Bgr555(3); profile.palette.colors_per_palette],
                },
                exanimation: CompactExAnimation {
                    setting: 4,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: Vec::new(),
                },
                expanded_settings: Some(ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap()),
            },
        };
        let encoded = file.encode(&profile.exanimation_double_size_modes).unwrap();
        fs::write(&input, &encoded).unwrap();
        fs::write(&profile_path, profile.encode()).unwrap();
        execute(&input, &profile_path, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(fs::read(&normalized).unwrap(), encoded);
        let observed = lm_oracle::Observation::from_text(
            std::str::from_utf8(&fs::read(&observation).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(observed.get("native-assets/source-slot"), Some("261"));
        assert_eq!(
            observed.get("native-assets/palette/colors/00ff/bgr555"),
            Some("3")
        );
        assert!(execute(&input, &profile_path, Some(&normalized), None).is_err());
        for path in [input, profile_path, normalized, observation] {
            fs::remove_file(path).unwrap();
        }
    }
}
