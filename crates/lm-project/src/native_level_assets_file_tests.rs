use super::*;
use lm_graphics::{Bgr555, CompactExAnimation, Palette};
use lm_level::{LevelObjectData, NativeSpriteStream};

fn file(with_settings: bool) -> NativeLevelAssetsFile {
    NativeLevelAssetsFile {
        source_slot: 0x105,
        assets: LoadedNativeLevelAssets {
            level: crate::LoadedLevelSlot {
                number: 0x105,
                layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
                sprites: NativeSpriteStream::parse(
                    &[0x10, 0, 1, 2, 0xff],
                    false,
                    &SpriteLengthTable::standard(),
                )
                .unwrap(),
            },
            palette: Palette {
                colors: vec![Bgr555(1), Bgr555(2)],
            },
            exanimation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
            expanded_settings: with_settings
                .then(|| ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap()),
        },
    }
}

#[test]
fn aggregate_round_trips_with_and_without_settings() {
    for with_settings in [false, true] {
        let file = file(with_settings);
        let bytes = file.encode(&[false; 256]).unwrap();
        assert_eq!(
            NativeLevelAssetsFile::decode(&bytes, &SpriteLengthTable::standard(), 8, &[false; 256])
                .unwrap(),
            file
        );
    }
}

#[test]
fn rejects_nested_slot_disagreement_and_bad_settings_shape() {
    let bytes = file(true).encode(&[false; 256]).unwrap();
    let mut nested_slot = bytes.clone();
    nested_slot[NativeLevelAssetsFile::HEADER_LEN + 12] ^= 1;
    assert!(matches!(
        NativeLevelAssetsFile::decode(
            &nested_slot,
            &SpriteLengthTable::standard(),
            8,
            &[false; 256]
        ),
        Err(NativeLevelAssetsFileError::SourceSlotMismatch {
            domain: "level",
            ..
        })
    ));

    let mut settings_len = bytes;
    settings_len[28..32].copy_from_slice(&31_u32.to_le_bytes());
    assert!(matches!(
        NativeLevelAssetsFile::decode(
            &settings_len,
            &SpriteLengthTable::standard(),
            8,
            &[false; 256]
        ),
        Err(NativeLevelAssetsFileError::SettingsLength(31))
    ));
}

#[test]
fn rejects_outer_slot_disagreement_before_encoding() {
    let mut file = file(false);
    file.assets.level.number = 4;
    assert!(matches!(
        file.encode(&[false; 256]),
        Err(NativeLevelAssetsFileError::SourceSlotMismatch {
            domain: "assets",
            ..
        })
    ));
}
