use super::*;
use lm_level::{
    LegacyHeaderEdit, LevelObjectData, NativeObjectRecordFields, NativeSpriteRecordFields,
    NativeSpriteStream, ObjectEdit, ObjectRecord, SpriteRecord, SpriteToken,
};

fn file() -> NativeLevelFile {
    NativeLevelFile {
        source_level: 0x105,
        layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
        sprites: NativeSpriteStream::parse(
            &[0x10, 0x00, 0x20, 0x01, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap(),
    }
}

fn controller() -> NativeLevelDocumentController {
    let file = file();
    NativeLevelDocumentController::decode(
        "level.lmlvl".into(),
        &file.encode().unwrap(),
        SpriteLengthTable::standard(),
    )
    .unwrap()
}

#[test]
fn expanded_sprite_relocation_is_one_revision_and_reopens_canonically() {
    let mut value = file();
    value.sprites.expanded = true;
    value.sprites.header |= 0x20;
    value.sprites.tokens.insert(0, SpriteToken::Screen(2));
    let mut controller = NativeLevelDocumentController::decode(
        "expanded.lmlvl".into(),
        &value.encode().unwrap(),
        SpriteLengthTable::standard(),
    )
    .unwrap();
    controller
        .apply_edits(
            0,
            &[NativeLevelEdit::RelocateExpandedSprite {
                selected: 1,
                screen: 4,
                x: 3,
                y: 5 * 32 + 7,
            }],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    let placement = controller.value().sprites.native_placements()[0];
    assert_eq!(
        (placement.screen, placement.major, placement.minor),
        (4, 67, 167)
    );
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        NativeLevelFile::decode(&snapshot.bytes, controller.sprite_lengths()).unwrap(),
        *controller.value()
    );
    controller.cancel_save(snapshot.request_id).unwrap();
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &value);
}

#[test]
fn semantic_sprite_position_edits_cover_legacy_and_expanded_documents() {
    let mut legacy = controller();
    legacy
        .apply_edits(
            0,
            &[NativeLevelEdit::PlaceSpriteAtPosition {
                record: SpriteRecord {
                    encoded: vec![0x08, 0x00, 0x47],
                },
                screen: 0x1f,
                x: 0x0c,
                y: 0x1a,
            }],
        )
        .unwrap();
    let placed = legacy
        .value()
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| placement.sprite_number == 0x47)
        .unwrap();
    assert_eq!(
        (placed.screen, placed.major, placed.minor),
        (0x1f, 0x1fc, 0x1a)
    );
    legacy
        .apply_edits(
            1,
            &[NativeLevelEdit::RelocateSpritePosition {
                selected: placed.token_index,
                screen: 0,
                x: 3,
                y: 9,
            }],
        )
        .unwrap();
    let relocated = legacy
        .value()
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| placement.sprite_number == 0x47)
        .unwrap();
    assert_eq!(
        (relocated.screen, relocated.major, relocated.minor),
        (0, 3, 9)
    );
    legacy
        .apply_edits(
            2,
            &[NativeLevelEdit::SetSpriteFields {
                index: relocated.token_index,
                fields: NativeSpriteRecordFields {
                    y_low: 0x1c,
                    extra_bits: 2,
                    screen: 0x1d,
                    x: 0x0b,
                    sprite_number: 0x47,
                },
            }],
        )
        .unwrap();
    let field_edited = legacy
        .value()
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| placement.sprite_number == 0x47)
        .unwrap();
    assert_eq!(
        (
            field_edited.screen,
            field_edited.major,
            field_edited.minor,
            field_edited.extra_bits,
        ),
        (0x1d, 0x1db, 0x1c, 2)
    );

    let mut value = file();
    value.sprites.expanded = true;
    value.sprites.header |= NativeSpriteStream::EXPANDED_HEADER_FLAG;
    value.sprites.tokens.insert(0, SpriteToken::Screen(2));
    let mut expanded = NativeLevelDocumentController::decode(
        "semantic-expanded.lmlvl".into(),
        &value.encode().unwrap(),
        SpriteLengthTable::standard(),
    )
    .unwrap();
    assert!(expanded.value().sprites.expanded);
    expanded
        .apply_edits(
            0,
            &[NativeLevelEdit::PlaceSpriteAtPosition {
                record: SpriteRecord {
                    encoded: vec![0x04, 0x00, 0x47],
                },
                screen: 0x1e,
                x: 0x0a,
                y: 4 * 32 + 0x1d,
            }],
        )
        .unwrap();
    let placed = expanded
        .value()
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| placement.sprite_number == 0x47)
        .unwrap();
    assert_eq!(
        (placed.screen, placed.major, placed.minor),
        (0x1e, 0x1ea, 0x9d)
    );
    expanded
        .apply_edits(
            1,
            &[NativeLevelEdit::SetSpriteFields {
                index: placed.token_index,
                fields: NativeSpriteRecordFields {
                    y_low: 0x1b,
                    extra_bits: 2,
                    screen: 1,
                    x: 2,
                    sprite_number: 0x47,
                },
            }],
        )
        .unwrap();
    let field_edited = expanded
        .value()
        .sprites
        .native_placements()
        .into_iter()
        .find(|placement| placement.sprite_number == 0x47)
        .unwrap();
    assert_eq!(
        (
            field_edited.screen,
            field_edited.major,
            field_edited.minor,
            field_edited.extra_bits,
        ),
        (1, 0x12, 0x9b, 2)
    );
    assert!(
        expanded
            .value()
            .sprites
            .tokens
            .iter()
            .any(|token| matches!(token, SpriteToken::Screen(4)))
    );
}

#[test]
fn semantic_ordinary_fields_reorder_and_reopen_through_the_document_controller() {
    let mut controller = controller();
    let placement = controller.value().layer1.objects.native_placements()[0];
    let record = &controller.value().layer1.objects.records[placement.record_index];
    let fields = NativeObjectRecordFields {
        command_id: record.command_id(),
        parameter: record.parameter(),
        screen: 0x1f,
        coordinates: lm_level::ObjectCoordinateNibbles {
            first: 0x0c,
            second: 0x0b,
        },
        perpendicular_high: true,
    };
    controller
        .apply_edits(
            0,
            &[NativeLevelEdit::Objects(vec![
                ObjectEdit::SetOrdinaryFields {
                    index: placement.record_index,
                    fields,
                },
            ])],
        )
        .unwrap();
    let edited = controller
        .value()
        .layer1
        .objects
        .native_placements()
        .into_iter()
        .find(|placement| {
            placement.screen == 0x1f && placement.major == 0x1fb && placement.minor == 0x1c
        })
        .unwrap();
    let record = &controller.value().layer1.objects.records[edited.record_index];
    assert_eq!(
        (record.command_id(), record.parameter()),
        (fields.command_id, fields.parameter)
    );
    assert!(record.perpendicular_high_coordinate());
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        NativeLevelFile::decode(&snapshot.bytes, controller.sprite_lengths()).unwrap(),
        *controller.value()
    );
}

#[test]
fn vertical_expanded_relocation_uses_level_mode_for_canonical_ordering() {
    let mut value = file();
    value.sprites = NativeSpriteStream {
        header: NativeSpriteStream::EXPANDED_HEADER_FLAG,
        expanded: true,
        tokens: vec![
            SpriteToken::Screen(2),
            SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![0xa0, 0x05, 0x10],
            }),
            SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![0x20, 0x05, 0x20],
            }),
            SpriteToken::Record(lm_level::SpriteRecord {
                encoded: vec![0x80, 0x05, 0x30],
            }),
        ],
    };
    let mut controller = NativeLevelDocumentController::decode(
        "vertical-expanded.lmlvl".into(),
        &value.encode().unwrap(),
        SpriteLengthTable::standard(),
    )
    .unwrap();

    controller
        .apply_edits(
            0,
            &[
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                NativeLevelEdit::RelocateExpandedSprite {
                    selected: 1,
                    screen: 5,
                    x: 0,
                    y: 2 * 32 + 10,
                },
            ],
        )
        .unwrap();
    assert_eq!(controller.value().layer1.header.level_mode(), 3);
    assert_eq!(
        controller
            .value()
            .sprites
            .tokens
            .iter()
            .filter_map(|token| match token {
                SpriteToken::Record(record) => Some(record.encoded[2]),
                SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
            })
            .collect::<Vec<_>>(),
        [0x20, 0x30, 0x10]
    );
}

#[test]
fn mixed_native_edits_are_shared_atomic_and_canonical() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(0x1f)),
                NativeLevelEdit::Objects(vec![ObjectEdit::Insert {
                    index: 1,
                    record: ObjectRecord::new(vec![3, 4, 5]).unwrap(),
                }]),
                NativeLevelEdit::SetSpriteHeader(0x44),
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().layer1.header.level_mode(), 0x1f);
    assert_eq!(controller.value().layer1.objects.records.len(), 2);
    assert_eq!(controller.value().sprites.header, 0x44);
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        NativeLevelFile::decode(&snapshot.bytes, controller.sprite_lengths()).unwrap(),
        *controller.value()
    );
}

#[test]
fn late_failure_and_stale_revision_preserve_both_streams() {
    let mut controller = controller();
    let original = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                NativeLevelEdit::SetSpriteHeader(9),
                NativeLevelEdit::Objects(vec![ObjectEdit::Remove { index: 99 }]),
            ]
        ),
        Err(NativeLevelDocumentControllerError::Edit(
            LevelControllerError::ObjectEdit { command: 1, .. }
        ))
    ));
    assert_eq!(controller.value(), &original);
    assert!(matches!(
        controller.apply_edits(1, &[]),
        Err(NativeLevelDocumentControllerError::StaleRevision { .. })
    ));
}

#[test]
fn save_acknowledgement_retains_newer_edits_and_cancel_is_retryable() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[NativeLevelEdit::SetSpriteHeader(1)])
        .unwrap();
    let first = controller.begin_save().unwrap();
    controller
        .apply_edits(1, &[NativeLevelEdit::SetSpriteHeader(2)])
        .unwrap();
    assert!(matches!(
        controller.begin_save(),
        Err(NativeLevelDocumentControllerError::SavePending)
    ));
    assert!(controller.acknowledge_save(first.request_id + 1).is_err());
    controller.acknowledge_save(first.request_id).unwrap();
    assert!(controller.is_modified());
    let second = controller.begin_save().unwrap();
    controller.cancel_save(second.request_id).unwrap();
    assert!(!controller.save_pending());
}

#[test]
fn explicit_length_table_is_retained_for_custom_sprite_records() {
    let mut lengths = SpriteLengthTable::standard();
    lengths.set(0, 0x20, 4).unwrap();
    let value = NativeLevelFile {
        sprites: NativeSpriteStream::parse(&[0, 0, 0, 0x20, 0xaa, 0xff], false, &lengths).unwrap(),
        ..file()
    };
    let mut controller = NativeLevelDocumentController::decode(
        "custom.lmlvl".into(),
        &value.encode().unwrap(),
        lengths,
    )
    .unwrap();
    controller
        .apply_edits(0, &[NativeLevelEdit::SetSpriteHeader(3)])
        .unwrap();
    assert_eq!(
        NativeLevelFile::decode(
            &controller.begin_save().unwrap().bytes,
            controller.sprite_lengths()
        )
        .unwrap()
        .sprites
        .tokens,
        value.sprites.tokens
    );
}

#[test]
fn history_restores_saved_native_level_and_invalidates_divergent_redo() {
    let mut controller = controller();
    controller
        .apply_edits(0, &[NativeLevelEdit::SetSpriteHeader(1)])
        .unwrap();
    let snapshot = controller.begin_save().unwrap();
    controller.acknowledge_save(snapshot.request_id).unwrap();
    controller
        .apply_edits(1, &[NativeLevelEdit::SetSpriteHeader(2)])
        .unwrap();
    assert!(controller.undo(2).unwrap());
    assert!(!controller.is_modified());
    assert!(controller.redo(3).unwrap());
    assert!(controller.undo(4).unwrap());
    controller
        .apply_edits(5, &[NativeLevelEdit::SetSpriteHeader(3)])
        .unwrap();
    assert!(!controller.can_redo());
    assert!(matches!(
        controller.undo(5),
        Err(NativeLevelDocumentControllerError::StaleRevision { .. })
    ));
}
