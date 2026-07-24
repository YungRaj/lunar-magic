use super::*;
use crate::{Entrance, EntranceKind, ObjectStream, Subtile};

fn complete_level() -> CompleteLevelFile {
    let object = || ObjectRecord::new(vec![1, 2, 3]).unwrap();
    CompleteLevelFile(Level {
        number: 0x105,
        header: LevelHeader {
            legacy: LegacyLevelHeader::decode(&[1, 2, 3, 4, 5]).unwrap(),
            expanded: Some(ExpandedLevelHeader {
                fields: std::array::from_fn(|index| u16::try_from(index).unwrap() * 17),
            }),
        },
        layer1: LayerData {
            objects: ObjectStream {
                records: vec![object()],
            },
            raw_tilemap: vec![0x1234, 0xabcd],
        },
        layer2: LayerData {
            objects: ObjectStream {
                records: vec![object(), object()],
            },
            raw_tilemap: vec![0x5678],
        },
        layer3: Some(crate::Layer3Data {
            settings: crate::Layer3Settings {
                start_position: 2,
                tilemap_size: 1,
                liquid_type: 3,
                flags: 0x80,
                graphics_files: [0x28, 0x29, 0x2a, 0x2b],
                reserved: [0x55; 16],
            },
            tilemap: vec![1, 2, 3, 4],
            remap_commands: vec![0x80, 7],
        }),
        sprites: SpriteStream {
            header: 0x77,
            records: vec![SpriteRecord {
                encoded: vec![9, 8, 7, 6, 5],
            }],
        },
        entrances: vec![
            Entrance {
                kind: EntranceKind::Main,
                x: 1,
                y: 2,
                screen: 3,
                action: 4,
                raw_flags: 0x8050,
            },
            Entrance {
                kind: EntranceKind::Secondary,
                x: 5,
                y: 6,
                screen: 7,
                action: 8,
                raw_flags: 0x1234,
            },
        ],
        screen_exits: vec![ScreenExit {
            encoded: 0x1234_5678,
        }],
        secondary_exits: vec![SecondaryExit {
            destination_level: 0x1ff,
            position_and_method: 1,
            screen: 2,
            x: 3,
            y: 4,
            destination_flags: 5,
            x_and_overworld_flags: 6,
            additional_flags: 7,
        }],
        map16_overrides: vec![(
            0x12345,
            Map16Tile {
                top_left: Subtile(1),
                top_right: Subtile(2),
                bottom_left: Subtile(3),
                bottom_right: Subtile(4),
                acts_like: 5,
            },
        )],
        unknown_extensions: vec![vec![0, 0xff, 3], vec![]],
    })
}

#[test]
fn version_one_bundles_decode_with_no_layer_three_state() {
    let mut expected = complete_level();
    expected.0.layer3 = None;
    let mut bytes = expected.encode().unwrap();
    let mut input = ByteCursor::new(&bytes);
    input
        .take(MAGIC.len() + 2 + 2 + LegacyLevelHeader::ENCODED_LEN)
        .unwrap();
    assert_eq!(input.u8().unwrap(), 1);
    input.take(ExpandedLevelHeader::ENCODED_LEN).unwrap();
    decode_layer(
        &mut input,
        LevelCollection::Layer1Objects,
        LevelCollection::Layer1Tiles,
    )
    .unwrap();
    decode_layer(
        &mut input,
        LevelCollection::Layer2Objects,
        LevelCollection::Layer2Tiles,
    )
    .unwrap();
    let layer3_flag = bytes.len() - input.remaining();
    assert_eq!(bytes.remove(layer3_flag), 0);
    bytes[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&1u16.to_le_bytes());
    assert_eq!(CompleteLevelFile::decode(&bytes).unwrap(), expected);
}

#[test]
fn all_level_domains_round_trip_deterministically() {
    let expected = complete_level();
    let bytes = expected.encode().unwrap();
    let layer3 = expected
        .0
        .layer3
        .as_ref()
        .map(|value| Layer3File(value.clone()).encode().unwrap());
    assert_eq!(
        encoded_file_len(&expected.0, layer3.as_deref()).unwrap(),
        bytes.len()
    );
    assert_eq!(CompleteLevelFile::decode(&bytes).unwrap(), expected);
    assert_eq!(
        CompleteLevelFile::decode(&bytes).unwrap().encode().unwrap(),
        bytes
    );
}

#[test]
fn every_truncation_trailing_data_and_invalid_flags_are_rejected() {
    let bytes = complete_level().encode().unwrap();
    for end in 0..bytes.len() {
        assert!(CompleteLevelFile::decode(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        CompleteLevelFile::decode(&trailing),
        Err(CompleteLevelFileError::TrailingBytes(1))
    ));
    let mut invalid_flag = bytes;
    invalid_flag[17] = 2;
    assert!(matches!(
        CompleteLevelFile::decode(&invalid_flag),
        Err(CompleteLevelFileError::InvalidExpandedFlag(2))
    ));
}

#[test]
fn excessive_variable_records_fail_before_output() {
    let mut file = complete_level();
    file.0.sprites.records[0].encoded = vec![0; CompleteLevelFile::MAX_RECORD_LEN + 1];
    assert!(matches!(
        file.encode(),
        Err(CompleteLevelFileError::RecordTooLarge {
            collection: LevelCollection::Sprites,
            ..
        })
    ));
}

#[test]
fn duplicate_map16_override_keys_fail_on_encode_and_decode() {
    let mut duplicate_model = complete_level();
    duplicate_model
        .0
        .map16_overrides
        .push(duplicate_model.0.map16_overrides[0]);
    assert!(matches!(
        duplicate_model.encode(),
        Err(CompleteLevelFileError::DuplicateMap16Override(0x12345))
    ));

    let first_key = 0x0bad_c0de_u32;
    let second_key = 0xfeed_face_u32;
    let mut malformed = complete_level();
    malformed.0.map16_overrides[0].0 = first_key;
    malformed
        .0
        .map16_overrides
        .push((second_key, malformed.0.map16_overrides[0].1));
    let mut bytes = malformed.encode().unwrap();
    let second = second_key.to_le_bytes();
    let positions = bytes
        .windows(second.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == second).then_some(offset))
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 1);
    bytes[positions[0]..positions[0] + 4].copy_from_slice(&first_key.to_le_bytes());
    assert!(matches!(
        CompleteLevelFile::decode(&bytes),
        Err(CompleteLevelFileError::DuplicateMap16Override(value)) if value == first_key
    ));
}

#[test]
fn aggregate_file_limit_is_checked_before_capacity_allocation() {
    assert_eq!(
        checked_file_add(CompleteLevelFile::MAX_FILE_LEN - 1, 1).unwrap(),
        CompleteLevelFile::MAX_FILE_LEN
    );
    assert!(matches!(
        checked_file_add(CompleteLevelFile::MAX_FILE_LEN, 1),
        Err(CompleteLevelFileError::FileTooLarge(size))
            if size == CompleteLevelFile::MAX_FILE_LEN + 1
    ));
    assert!(matches!(
        checked_file_add(usize::MAX, 1),
        Err(CompleteLevelFileError::Overflow)
    ));
}
