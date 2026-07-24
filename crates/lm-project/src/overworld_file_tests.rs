use super::*;
use lm_graphics::{Bgr555, CompactExAnimation};
use lm_overworld::{EventReveal, OverworldEndpoint, Submap};

fn file() -> CompleteOverworldFile {
    let shape = CompleteOverworldShape {
        width: 2,
        height: 2,
        event_reveals: 2,
        endpoints: 2,
        messages: 2,
        sprites: 2,
        sprite_record_len: 9,
        palette_colors: 16,
    };
    CompleteOverworldFile {
        source_slot: 3,
        shape,
        data: CompleteOverworldData {
            layers: OverworldLayers {
                layer1: OverworldLayer::new(2, 2, vec![1, 2, 3, 4]).unwrap(),
                layer2: OverworldLayer::new(2, 2, vec![5, 6, 7, 8]).unwrap(),
            },
            event_reveals: EventRevealTable {
                entries: vec![
                    EventReveal {
                        source_tile: 1,
                        destination_tile: 2,
                    },
                    EventReveal {
                        source_tile: 3,
                        destination_tile: 4,
                    },
                ],
            },
            endpoints: vec![
                OverworldEndpoint {
                    x: 1,
                    y: 2,
                    submap: 0,
                },
                OverworldEndpoint {
                    x: 3,
                    y: 4,
                    submap: 1,
                },
            ],
            messages: vec![
                OverworldMessage::decode(&[0x11; OverworldMessage::ENCODED_LEN]).unwrap(),
                OverworldMessage::decode(&[0x22; OverworldMessage::ENCODED_LEN]).unwrap(),
            ],
            sprites: vec![
                OverworldSprite {
                    id: 1,
                    x: 2,
                    y: 3,
                    submap: Submap::Main,
                    extra: vec![0xaa, 0xbb],
                },
                OverworldSprite {
                    id: 4,
                    x: 5,
                    y: 6,
                    submap: Submap::StarWorld,
                    extra: vec![0xcc, 0xdd],
                },
            ],
            palette: Palette {
                colors: (0_u16..16).map(Bgr555).collect(),
            },
            animation: CompactExAnimation {
                setting: 1,
                header_value: 2,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: Vec::new(),
            },
        },
    }
}

#[test]
fn all_nine_domains_round_trip() {
    let modes = [false; 256];
    let file = file();
    assert_eq!(
        CompleteOverworldFile::decode(&file.encode(&modes).unwrap(), 32, &modes).unwrap(),
        file
    );
}

#[test]
fn shape_mismatch_reserved_bytes_and_trailing_data_are_rejected() {
    let modes = [false; 256];
    let mut mismatched = file();
    mismatched.shape.messages = 1;
    assert!(matches!(
        mismatched.encode(&modes),
        Err(CompleteOverworldFileError::ShapeMismatch { .. })
    ));
    let bytes = file().encode(&modes).unwrap();
    let mut reserved = bytes.clone();
    reserved[28] = 1;
    assert!(matches!(
        CompleteOverworldFile::decode(&reserved, 32, &modes),
        Err(CompleteOverworldFileError::ReservedBytes)
    ));
    let mut trailing = bytes;
    trailing.push(0);
    assert!(matches!(
        CompleteOverworldFile::decode(&trailing, 32, &modes),
        Err(CompleteOverworldFileError::WrongLength { .. })
    ));
}

#[test]
fn invalid_dimensions_and_sprite_record_shape_are_bounded() {
    let modes = [false; 256];
    let mut empty = file();
    empty.shape.width = 0;
    assert!(matches!(
        empty.encode(&modes),
        Err(CompleteOverworldFileError::EmptyDimensions)
    ));
    let mut short_sprite = file();
    short_sprite.shape.sprite_record_len = 6;
    assert!(matches!(
        short_sprite.encode(&modes),
        Err(CompleteOverworldFileError::Sprites(
            OverworldSpriteError::RecordTooShort(6)
        ))
    ));
}

#[test]
fn event_source_that_cannot_semantically_reopen_is_rejected() {
    let modes = [false; 256];
    let mut invalid = file();
    invalid.data.event_reveals.entries[1].source_tile = EventRevealTable::MAX_TILE + 1;
    assert!(matches!(
        invalid.encode(&modes),
        Err(CompleteOverworldFileError::Events(
            EventTableError::InvalidSourceTile {
                index: 1,
                tile: 0x800
            }
        ))
    ));
}

#[test]
fn animation_state_omitted_by_compact_format_is_rejected() {
    let modes = [false; 256];
    let mut invalid = file();
    invalid.data.animation.trigger_values[6] = 0x44;
    assert!(matches!(
        invalid.encode(&modes),
        Err(CompleteOverworldFileError::Animation(
            ExAnimationError::DisabledTriggerValue {
                trigger: 6,
                value: 0x44
            }
        ))
    ));
}
