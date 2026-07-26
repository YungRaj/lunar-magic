use super::*;
use lm_level::{
    LegacyHeaderEdit, LevelObjectData, NativeSpriteStream, ObjectEdit, ObjectRecord, SpriteToken,
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
