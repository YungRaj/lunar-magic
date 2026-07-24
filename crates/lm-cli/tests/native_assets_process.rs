use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{
    ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream, SpriteLengthTable,
};
use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets, NativeLevelAssetsFile};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn fixture(profile: &lm_profile::RevisionProfile) -> NativeLevelAssetsFile {
    NativeLevelAssetsFile {
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
                colors: (0..profile.palette.colors_per_palette)
                    .map(|index| Bgr555(u16::try_from(index).unwrap()))
                    .collect(),
            },
            exanimation: CompactExAnimation {
                setting: 6,
                header_value: 0x1234,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            expanded_settings: Some(ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap()),
        },
    }
}

#[test]
fn built_cli_normalizes_and_observes_aggregate_through_unicode_paths() {
    let directory = std::env::temp_dir().join(format!(
        "lm-native-assets-process-{}-{}-日本語",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("source level assets.lmna");
    let profile_path = directory.join("revision profile.lmrev");
    let normalized = directory.join("normalized level assets.lmna");
    let observation = directory.join("semantic level assets.obs");
    let profile = lm_profile::test_support::profile();
    let encoded = fixture(&profile)
        .encode(&profile.exanimation_double_size_modes)
        .unwrap();
    fs::write(&input, &encoded).unwrap();
    fs::write(&profile_path, profile.encode()).unwrap();

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args([
                "native-assets-file",
                input.to_str().unwrap(),
                profile_path.to_str().unwrap(),
                normalized.to_str().unwrap(),
                observation.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&normalized).unwrap(), encoded);
    let observed = lm_oracle::Observation::from_text(
        std::str::from_utf8(&fs::read(&observation).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(observed.get("native-assets/source-slot"), Some("261"));
    assert_eq!(
        observed.get("native-assets/palette/colors/00ff/bgr555"),
        Some("255")
    );
    let before_observation = fs::read(&observation).unwrap();

    let second = run();
    assert!(!second.status.success());
    assert_eq!(fs::read(&normalized).unwrap(), encoded);
    assert_eq!(fs::read(&observation).unwrap(), before_observation);
    fs::remove_dir_all(directory).unwrap();
}
