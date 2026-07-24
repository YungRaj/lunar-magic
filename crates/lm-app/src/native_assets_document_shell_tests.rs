use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream};
use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets};
use std::time::{SystemTime, UNIX_EPOCH};

fn directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lm-native-assets-shell-{}-{nonce}",
        std::process::id()
    ))
}

fn file(colors: usize) -> NativeLevelAssetsFile {
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
fn open_edit_save_close_round_trips_all_aggregate_domains() {
    let directory = directory();
    fs::create_dir(&directory).unwrap();
    let profile = lm_profile::test_support::profile();
    let document = directory.join("Aggregate 日本語.lmnat");
    fs::write(
        &document,
        file(profile.palette.colors_per_palette)
            .encode(&profile.exanimation_double_size_modes)
            .unwrap(),
    )
    .unwrap();
    fs::write(directory.join("profile.txt"), profile.encode()).unwrap();
    fs::write(
        directory.join("open.txt"),
        "LMNADOC1\ndocument Aggregate 日本語.lmnat\nprofile profile.txt\n",
    )
    .unwrap();
    fs::write(directory.join("level.txt"), "LMLEDIT1\nheader mode 03\n").unwrap();
    fs::write(
        directory.join("palette.txt"),
        format!(
            "LMPALED1\nowners {:x} editable\nset 1 1234\n",
            profile.palette.colors_per_palette
        ),
    )
    .unwrap();
    fs::write(directory.join("animation.txt"), "LMEXAED1\nsetting 07\n").unwrap();
    fs::write(directory.join("settings.txt"), "LMXSETED1\nword 2 abcd\n").unwrap();
    fs::write(
        directory.join("edits.txt"),
        "LMNATED1\nlevel=level.txt\npalette=palette.txt\nexanimation=animation.txt\nexpanded-settings=settings.txt\n",
    )
    .unwrap();

    let mut session = None;
    open(&mut session, &directory.join("open.txt")).unwrap();
    edit(&mut session, &directory.join("edits.txt")).unwrap();
    assert!(close(&mut session, false).is_err());
    save(&mut session).unwrap();
    close(&mut session, false).unwrap();
    let saved = NativeLevelAssetsFile::decode(
        &fs::read(&document).unwrap(),
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
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_open_and_dirty_discard_preserve_the_document() {
    let directory = directory();
    fs::create_dir(&directory).unwrap();
    let profile = lm_profile::test_support::profile();
    let document = directory.join("aggregate.lmnat");
    let original = file(profile.palette.colors_per_palette)
        .encode(&profile.exanimation_double_size_modes)
        .unwrap();
    fs::write(&document, &original).unwrap();
    fs::write(directory.join("profile.txt"), "invalid").unwrap();
    fs::write(
        directory.join("open.txt"),
        "LMNADOC1\ndocument aggregate.lmnat\nprofile profile.txt\n",
    )
    .unwrap();
    let mut session = None;
    assert!(open(&mut session, &directory.join("open.txt")).is_err());
    assert!(session.is_none());
    fs::write(directory.join("profile.txt"), profile.encode()).unwrap();
    open(&mut session, &directory.join("open.txt")).unwrap();
    session
        .as_mut()
        .unwrap()
        .apply_edits(
            0,
            &[lm_app::NativeLevelAssetsControllerEdit::ExAnimation(vec![
                lm_app::ExAnimationControllerEdit::SetSetting(9),
            ])],
            &PaletteOwnership::editable(profile.palette.colors_per_palette),
        )
        .unwrap();
    close(&mut session, true).unwrap();
    assert_eq!(fs::read(&document).unwrap(), original);
    fs::remove_dir_all(directory).unwrap();
}
