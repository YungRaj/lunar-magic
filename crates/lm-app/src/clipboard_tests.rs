use super::*;
use lm_graphics::{Bgr555, ExAnimationFrame};
use lm_level::{Map16Tile, Subtile};
use lm_overworld::{OverworldSprite, SpriteAppearancePart, Submap};

#[test]
fn framing_round_trips_and_rejects_trailing_bytes() {
    let payload =
        ClipboardPayload::new(ClipboardKind::LevelSprites, vec![vec![1, 2, 3], vec![4, 5]])
            .unwrap();
    let bytes = payload.encode().unwrap();
    assert_eq!(ClipboardPayload::decode(&bytes).unwrap(), payload);
    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(
        ClipboardPayload::decode(&trailing),
        Err(ClipboardError::TrailingBytes(1))
    );
    let mut flagged = payload.encode().unwrap();
    flagged[9] = 1;
    assert_eq!(
        ClipboardPayload::decode(&flagged),
        Err(ClipboardError::UnknownFlags(1))
    );
    assert_eq!(
        ClipboardPayload::decode_with_limit(&flagged, flagged.len() - 1),
        Err(ClipboardError::PayloadTooLarge(flagged.len()))
    );
}

#[test]
fn aggregate_envelope_limit_is_checked_before_encoding_or_record_copying() {
    let records = vec![vec![1; 3], vec![2; 4]];
    assert_eq!(validate_records(&records, 29), Ok(29));
    assert_eq!(
        validate_records(&records, 28),
        Err(ClipboardError::PayloadTooLarge(29))
    );
    let oversized_record = vec![0; ClipboardPayload::MAX_RECORD_LEN + 1];
    assert_eq!(
        ClipboardPayload::new(ClipboardKind::LevelObjects, vec![oversized_record]),
        Err(ClipboardError::RecordTooLarge(
            ClipboardPayload::MAX_RECORD_LEN + 1
        ))
    );
}

#[test]
fn map16_and_palette_records_are_semantic() {
    let tiles = [Map16Tile {
        top_left: Subtile(1),
        top_right: Subtile(2),
        bottom_left: Subtile(3),
        bottom_right: Subtile(4),
        acts_like: 0x123,
    }];
    let payload = ClipboardPayload::from_map16_tiles(&tiles);
    assert_eq!(payload.to_map16_tiles().unwrap(), tiles);
    let colors = [Bgr555(1), Bgr555(0x7fff)];
    assert_eq!(
        ClipboardPayload::from_palette_colors(&colors)
            .to_palette_colors()
            .unwrap(),
        colors
    );
}

#[test]
fn invalid_graphics_index_and_wrong_kind_are_rejected() {
    let payload = ClipboardPayload::new(ClipboardKind::GraphicsTiles, vec![vec![16; 64]]).unwrap();
    assert!(matches!(
        payload.to_graphics_tiles(),
        Err(ClipboardError::InvalidPixel { pixel: 0, .. })
    ));
    assert!(matches!(
        payload.to_palette_colors(),
        Err(ClipboardError::WrongKind { .. })
    ));
}

#[test]
fn variable_extension_sprites_round_trip() {
    let sprites = [
        OverworldSprite {
            id: 1,
            x: 2,
            y: 3,
            submap: Submap::Main,
            extra: vec![],
        },
        OverworldSprite {
            id: 4,
            x: 5,
            y: 6,
            submap: Submap::StarWorld,
            extra: vec![0xaa, 0xbb],
        },
    ];
    let payload = ClipboardPayload::from_overworld_sprites(&sprites).unwrap();
    assert_eq!(payload.to_overworld_sprites().unwrap(), sprites);
    assert_eq!(
        ClipboardPayload::decode(&payload.encode().unwrap()).unwrap(),
        payload
    );
}

#[test]
fn overworld_appearance_parts_round_trip_every_semantic_field() {
    let parts = [
        SpriteAppearancePart {
            tile_index: 0x1234,
            palette_index: 7,
            x_offset: i16::MIN,
            y_offset: i16::MAX,
            x_flip: true,
            y_flip: false,
        },
        SpriteAppearancePart {
            tile_index: 0xabcd,
            palette_index: 0,
            x_offset: -17,
            y_offset: 29,
            x_flip: false,
            y_flip: true,
        },
    ];
    let payload = ClipboardPayload::from_overworld_appearance_parts(&parts).unwrap();
    assert_eq!(payload.to_overworld_appearance_parts().unwrap(), parts);
    assert_eq!(
        ClipboardPayload::decode(&payload.encode().unwrap())
            .unwrap()
            .to_overworld_appearance_parts()
            .unwrap(),
        parts
    );
    assert!(matches!(
        payload.to_overworld_sprites(),
        Err(ClipboardError::WrongKind { .. })
    ));
}

#[test]
fn malformed_overworld_appearance_parts_are_rejected() {
    let invalid_palette = SpriteAppearancePart {
        tile_index: 0,
        palette_index: 8,
        x_offset: 0,
        y_offset: 0,
        x_flip: false,
        y_flip: false,
    };
    assert!(matches!(
        ClipboardPayload::from_overworld_appearance_parts(&[invalid_palette]),
        Err(ClipboardError::InvalidRecord { index: 0, .. })
    ));
    for record in [
        vec![0; 7],
        vec![0, 0, 8, 0, 0, 0, 0, 0],
        vec![0, 0, 0, 0, 0, 0, 0, 4],
    ] {
        let payload =
            ClipboardPayload::new(ClipboardKind::OverworldAppearanceParts, vec![record]).unwrap();
        assert!(matches!(
            payload.to_overworld_appearance_parts(),
            Err(ClipboardError::InvalidRecord { index: 0, .. })
        ));
    }
}

#[test]
fn layer_three_raw_domains_are_distinct_and_lossless() {
    let bytes = [0, 0x80, 0xff, 7];
    let tilemap = ClipboardPayload::from_layer3_tilemap_bytes(&bytes);
    assert_eq!(
        ClipboardPayload::decode(&tilemap.encode().unwrap())
            .unwrap()
            .to_layer3_tilemap_bytes()
            .unwrap(),
        bytes
    );
    let remap = ClipboardPayload::from_layer3_remap_bytes(&bytes);
    assert_eq!(remap.to_layer3_remap_bytes().unwrap(), bytes);
    assert!(matches!(
        remap.to_layer3_tilemap_bytes(),
        Err(ClipboardError::WrongKind { .. })
    ));
    let malformed =
        ClipboardPayload::new(ClipboardKind::Layer3RemapBytes, vec![vec![1, 2]]).unwrap();
    assert!(matches!(
        malformed.to_layer3_remap_bytes(),
        Err(ClipboardError::InvalidRecord {
            index: 0,
            length: 2
        })
    ));
}

#[test]
fn exanimation_frames_round_trip_with_explicit_widths() {
    let frames = [
        ExAnimationFrame {
            source_words: vec![0x1234],
        },
        ExAnimationFrame {
            source_words: vec![0xabcd, 0x5678],
        },
    ];
    let payload = ClipboardPayload::from_exanimation_frames(&frames).unwrap();
    assert_eq!(
        ClipboardPayload::decode(&payload.encode().unwrap())
            .unwrap()
            .to_exanimation_frames()
            .unwrap(),
        frames
    );
    assert!(matches!(
        payload.to_exanimation_records(),
        Err(ClipboardError::WrongKind { .. })
    ));
}

#[test]
fn malformed_or_unsupported_frame_widths_are_rejected() {
    assert!(matches!(
        ClipboardPayload::from_exanimation_frames(&[ExAnimationFrame {
            source_words: vec![1, 2, 3]
        }]),
        Err(ClipboardError::InvalidRecord { index: 0, .. })
    ));
    for record in [
        vec![],
        vec![0],
        vec![1, 1],
        vec![2, 1, 0],
        vec![3, 1, 0, 2, 0, 3, 0],
    ] {
        let payload =
            ClipboardPayload::new(ClipboardKind::ExAnimationFrames, vec![record]).unwrap();
        assert!(matches!(
            payload.to_exanimation_frames(),
            Err(ClipboardError::InvalidRecord { index: 0, .. })
        ));
    }
}

#[test]
fn layer_two_rectangles_round_trip_dimensions_and_visual_word_order() {
    let words = [0x1234, 0xabcd, 0x5678, 0xbeef, 0, 0xffff];
    let payload = ClipboardPayload::from_layer2_tilemap_selection(3, 2, &words).unwrap();
    assert_eq!(
        ClipboardPayload::decode(&payload.encode().unwrap())
            .unwrap()
            .to_layer2_tilemap_selection()
            .unwrap(),
        (3, 2, words.to_vec())
    );
    assert!(matches!(
        payload.to_layer3_tilemap_bytes(),
        Err(ClipboardError::WrongKind { .. })
    ));
}

#[test]
fn malformed_layer_two_rectangles_are_rejected() {
    for (width, height, words) in [
        (0, 1, vec![]),
        (1, 0, vec![]),
        (33, 1, vec![0; 33]),
        (1, 33, vec![0; 33]),
        (2, 2, vec![0; 3]),
    ] {
        assert!(matches!(
            ClipboardPayload::from_layer2_tilemap_selection(width, height, &words),
            Err(ClipboardError::InvalidRecord { .. })
        ));
    }
    for record in [
        vec![],
        vec![1],
        vec![0, 1],
        vec![1, 0],
        vec![2, 2, 0, 0],
        vec![1, 1, 0],
        vec![1, 1, 0, 0, 0],
    ] {
        let payload =
            ClipboardPayload::new(ClipboardKind::Layer2TilemapSelection, vec![record]).unwrap();
        assert!(matches!(
            payload.to_layer2_tilemap_selection(),
            Err(ClipboardError::InvalidRecord { .. })
        ));
    }
}
