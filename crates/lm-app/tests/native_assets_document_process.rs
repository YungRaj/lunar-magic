use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream};
use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets, NativeLevelAssetsFile};
use std::{fs, process::Command};

fn aggregate(colors: usize) -> NativeLevelAssetsFile {
    NativeLevelAssetsFile {
        source_slot: 0x105,
        assets: LoadedNativeLevelAssets {
            level: LoadedLevelSlot {
                number: 0x105,
                layer1: LevelObjectData::default(),
                sprites: NativeSpriteStream::default(),
            },
            palette: Palette {
                colors: vec![Bgr555(0); colors],
            },
            exanimation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            expanded_settings: Some(ExpandedLevelSettingsRecord::decode(&[0; 32]).unwrap()),
        },
    }
}

#[test]
#[allow(clippy::too_many_lines)] // One persisted snapshot anchors both real-process lifecycle runs.
fn real_binary_edits_saves_and_dirty_guards_portable_aggregate() {
    let directory = std::env::temp_dir().join(format!(
        "lm-app-native-assets-document-日本語-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let profile = lm_profile::test_support::profile();
    let document = directory.join("Aggregate 日本語.lmnat");
    let original = aggregate(profile.palette.colors_per_palette)
        .encode(&profile.exanimation_double_size_modes)
        .unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(directory.join("Profile 日本語.txt"), profile.encode()).unwrap();
    fs::write(
        directory.join("Open Aggregate.txt"),
        "LMNADOC1\ndocument Aggregate 日本語.lmnat\nprofile Profile 日本語.txt\n",
    )
    .unwrap();
    fs::write(
        directory.join("Level edits.txt"),
        "LMLEDIT1\nheader mode 03\n",
    )
    .unwrap();
    fs::write(
        directory.join("Palette edits.txt"),
        format!(
            "LMPALED1\nowners {:x} editable\nset 1 1234\n",
            profile.palette.colors_per_palette
        ),
    )
    .unwrap();
    fs::write(
        directory.join("Animation edits.txt"),
        "LMEXAED1\nsetting 07\n",
    )
    .unwrap();
    fs::write(
        directory.join("Settings edits.txt"),
        "LMXSETED1\nword 2 abcd\n",
    )
    .unwrap();
    fs::write(
        directory.join("Aggregate edits.txt"),
        "LMNATED1\nlevel=Level edits.txt\npalette=Palette edits.txt\nexanimation=Animation edits.txt\nexpanded-settings=Settings edits.txt\n",
    )
    .unwrap();
    let preview = directory.join("Edited Palette.png");
    fs::write(
        directory.join("Render Palette.txt"),
        "LMPALDR1\ncolumns 16\ncell-size 2\noutput Edited Palette.png\n",
    )
    .unwrap();
    let commands = directory.join("Commands.txt");
    fs::write(
        &commands,
        format!(
            "native-assets-open-file {}\nnative-assets-edit-file {}\nnative-assets-undo\nnative-assets-redo\nnative-assets-render-file {}\nnative-assets-status\nnative-assets-save\nnative-assets-close\nquit\n",
            directory.join("Open Aggregate.txt").display(),
            directory.join("Aggregate edits.txt").display(),
            directory.join("Render Palette.txt").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-assets document saved"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-assets undo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-assets redo: applied"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("native-assets palette rendered"));
    let png = fs::read(&preview).unwrap();
    assert_eq!(png.get(..8), Some(b"\x89PNG\r\n\x1a\n".as_slice()));
    assert_eq!(png.get(16..20), Some(32_u32.to_be_bytes().as_slice()));
    let expected_rows = profile.palette.colors_per_palette.div_ceil(16) * 2;
    assert_eq!(
        png.get(20..24),
        Some(
            u32::try_from(expected_rows)
                .unwrap()
                .to_be_bytes()
                .as_slice()
        )
    );
    let saved_bytes = fs::read(&document).unwrap();
    let saved = NativeLevelAssetsFile::decode(
        &saved_bytes,
        &profile.sprite_lengths,
        profile.exanimation.maximum_records,
        &profile.exanimation_double_size_modes,
    )
    .unwrap();
    assert_eq!(saved.assets.level.layer1.header.level_mode(), 3);
    assert_eq!(saved.assets.palette.colors[1], Bgr555(0x1234));
    assert_eq!(saved.assets.exanimation.setting, 7);
    assert_eq!(
        saved.assets.expanded_settings.unwrap().word(2).unwrap(),
        0xabcd
    );

    fs::write(
        directory.join("Animation edits.txt"),
        "LMEXAED1\nsetting 09\n",
    )
    .unwrap();
    fs::write(
        &commands,
        format!(
            "native-assets-open-file {}\nnative-assets-edit-file {}\n",
            directory.join("Open Aggregate.txt").display(),
            directory.join("Aggregate edits.txt").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lm-app"))
        .arg("--script")
        .arg(&commands)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("native level assets"));
    assert_eq!(fs::read(&document).unwrap(), saved_bytes);
    fs::remove_dir_all(directory).unwrap();
}
