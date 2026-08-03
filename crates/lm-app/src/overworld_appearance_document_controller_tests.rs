use super::*;
use lm_overworld::SpriteAppearanceDefinition;

fn part(tile: u16, x: i16) -> SpriteAppearancePart {
    SpriteAppearancePart {
        tile_index: tile,
        palette_index: 3,
        x_offset: x,
        y_offset: -8,
        x_flip: false,
        y_flip: true,
    }
}

fn controller() -> OverworldAppearanceDocumentController {
    let file = SpriteAppearanceFile {
        definitions: vec![
            SpriteAppearanceDefinition {
                sprite_id: 1,
                parts: vec![part(1, 10)],
            },
            SpriteAppearanceDefinition {
                sprite_id: 2,
                parts: vec![part(2, 20)],
            },
        ],
    };
    OverworldAppearanceDocumentController::decode("sprites.lmowapp".into(), &file.encode().unwrap())
        .unwrap()
}

#[test]
fn stable_definition_and_nested_part_edits_preserve_both_orders() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                OverworldAppearanceDocumentEdit::InsertDefinition {
                    index: 1,
                    sprite_id: 3,
                },
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: 3,
                    index: 0,
                    value: part(3, 30),
                },
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: 1,
                    index: 1,
                    value: part(4, 40),
                },
                OverworldAppearanceDocumentEdit::MoveDefinitionBefore {
                    sprite_id: 2,
                    before: Some(1),
                },
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(
        controller
            .value()
            .definitions
            .iter()
            .map(|value| value.sprite_id)
            .collect::<Vec<_>>(),
        [2, 1, 3]
    );
    assert_eq!(
        controller.value().definition(1).unwrap().parts[1].x_offset,
        40
    );
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        SpriteAppearanceFile::decode(&snapshot.bytes).unwrap(),
        *controller.value()
    );
}

#[test]
fn painter_order_move_is_atomic_canonical_and_undoable() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: 1,
                    index: 1,
                    value: part(2, 20),
                },
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: 1,
                    index: 2,
                    value: part(3, 30),
                },
            ],
        )
        .unwrap();
    let before_move = controller.value().clone();
    controller
        .apply_edits(
            1,
            &[OverworldAppearanceDocumentEdit::MovePartBefore {
                sprite_id: 1,
                index: 0,
                before: None,
            }],
        )
        .unwrap();
    assert_eq!(
        controller
            .value()
            .definition(1)
            .unwrap()
            .parts
            .iter()
            .map(|value| value.tile_index)
            .collect::<Vec<_>>(),
        [2, 3, 1]
    );
    let saved = controller.begin_save().unwrap();
    assert_eq!(
        SpriteAppearanceFile::decode(&saved.bytes).unwrap(),
        *controller.value()
    );
    controller.cancel_save(saved.request_id).unwrap();
    assert!(controller.undo(2).unwrap());
    assert_eq!(controller.value(), &before_move);
    assert!(controller.redo(3).unwrap());
    assert_eq!(
        controller.value().definition(1).unwrap().parts[2].tile_index,
        1
    );

    let before_error = controller.value().clone();
    assert!(
        controller
            .apply_edits(
                4,
                &[
                    OverworldAppearanceDocumentEdit::RemovePart {
                        sprite_id: 2,
                        index: 0,
                    },
                    OverworldAppearanceDocumentEdit::MovePartBefore {
                        sprite_id: 1,
                        index: 0,
                        before: Some(99),
                    },
                ],
            )
            .is_err()
    );
    assert_eq!(controller.value(), &before_error);
    assert_eq!(controller.revision(), 4);
}

#[test]
fn complete_part_replacement_is_one_revision_and_failure_atomic() {
    let mut controller = controller();
    let replacement = vec![part(7, -24), part(8, 32), part(9, 0)];
    controller
        .apply_edits(
            0,
            &[OverworldAppearanceDocumentEdit::ReplaceParts {
                sprite_id: 1,
                values: replacement.clone(),
            }],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().definition(1).unwrap().parts, replacement);
    assert!(controller.undo(1).unwrap());
    assert_eq!(
        controller.value().definition(1).unwrap().parts,
        [part(1, 10)]
    );

    let before = controller.value().clone();
    let mut invalid = part(10, 0);
    invalid.palette_index = 8;
    assert!(
        controller
            .apply_edits(
                2,
                &[OverworldAppearanceDocumentEdit::ReplaceParts {
                    sprite_id: 1,
                    values: vec![part(11, 0), invalid],
                }],
            )
            .is_err()
    );
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 2);
}

#[test]
fn complete_part_translation_is_one_revision_and_overflow_atomic() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[OverworldAppearanceDocumentEdit::ReplaceParts {
                sprite_id: 1,
                values: vec![part(1, -20), part(2, 30)],
            }],
        )
        .unwrap();
    controller
        .apply_edits(
            1,
            &[OverworldAppearanceDocumentEdit::TranslateParts {
                sprite_id: 1,
                delta_x: 8,
                delta_y: -1,
            }],
        )
        .unwrap();
    assert_eq!(controller.revision(), 2);
    let parts = &controller.value().definition(1).unwrap().parts;
    assert_eq!((parts[0].x_offset, parts[0].y_offset), (-12, -9));
    assert_eq!((parts[1].x_offset, parts[1].y_offset), (38, -9));

    controller
        .apply_edits(
            2,
            &[OverworldAppearanceDocumentEdit::ReplacePart {
                sprite_id: 1,
                index: 1,
                value: part(3, i16::MAX),
            }],
        )
        .unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            3,
            &[OverworldAppearanceDocumentEdit::TranslateParts {
                sprite_id: 1,
                delta_x: 1,
                delta_y: 0,
            }]
        ),
        Err(OverworldAppearanceDocumentControllerError::Edit {
            error: OverworldAppearanceEditError::PartOffsetOverflow {
                sprite_id: 1,
                index: 1,
                axis: "x",
                offset: i16::MAX,
                delta: 1,
            },
            ..
        })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 3);
}

#[test]
fn duplicate_missing_late_index_and_palette_failures_are_atomic() {
    let mut controller = controller();
    let original = controller.value().clone();
    for edits in [
        vec![OverworldAppearanceDocumentEdit::InsertDefinition {
            index: 0,
            sprite_id: 1,
        }],
        vec![OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id: 99 }],
        vec![
            OverworldAppearanceDocumentEdit::RemovePart {
                sprite_id: 1,
                index: 0,
            },
            OverworldAppearanceDocumentEdit::RemovePart {
                sprite_id: 2,
                index: 9,
            },
        ],
    ] {
        assert!(controller.apply_edits(0, &edits).is_err());
        assert_eq!(controller.value(), &original);
    }
    let mut invalid = part(3, 30);
    invalid.palette_index = 8;
    assert!(matches!(
        controller.apply_edits(
            0,
            &[OverworldAppearanceDocumentEdit::ReplacePart {
                sprite_id: 1,
                index: 0,
                value: invalid
            }]
        ),
        Err(OverworldAppearanceDocumentControllerError::File(
            SpriteAppearanceFileError::PaletteOutOfRange(8)
        ))
    ));
    assert_eq!(controller.value(), &original);
}

#[test]
fn revisions_and_immutable_save_requests_retain_newer_changes() {
    let mut controller = controller();
    assert!(controller.apply_edits(1, &[]).is_err());
    controller
        .apply_edits(
            0,
            &[OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id: 2 }],
        )
        .unwrap();
    let first = controller.begin_save().unwrap();
    controller
        .apply_edits(
            1,
            &[OverworldAppearanceDocumentEdit::InsertDefinition {
                index: 1,
                sprite_id: 4,
            }],
        )
        .unwrap();
    assert!(controller.begin_save().is_err());
    assert!(controller.acknowledge_save(first.request_id + 1).is_err());
    controller.acknowledge_save(first.request_id).unwrap();
    assert!(controller.is_modified());
    let second = controller.begin_save().unwrap();
    controller.cancel_save(second.request_id).unwrap();
    assert!(!controller.save_pending());
}

#[test]
fn nested_history_restores_saved_baseline_and_invalidates_divergent_redo() {
    let mut controller = controller();
    let original = controller.value().clone();
    controller
        .apply_edits(
            0,
            &[
                OverworldAppearanceDocumentEdit::MoveDefinitionBefore {
                    sprite_id: 2,
                    before: Some(1),
                },
                OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: 1,
                    index: 1,
                    value: part(3, 30),
                },
            ],
        )
        .unwrap();
    assert!(controller.can_undo());
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &original);
    assert!(!controller.is_modified());
    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.value().definitions[0].sprite_id, 2);
    assert_eq!(controller.value().definition(1).unwrap().parts.len(), 2);
    assert!(controller.undo(3).unwrap());
    controller
        .apply_edits(
            4,
            &[OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id: 2 }],
        )
        .unwrap();
    assert!(!controller.can_redo());
    assert!(!controller.redo(5).unwrap());
    assert_eq!(controller.revision(), 5);
}

#[test]
fn stale_and_empty_history_navigation_are_atomic() {
    let mut controller = controller();
    assert!(!controller.undo(0).unwrap());
    controller
        .apply_edits(
            0,
            &[OverworldAppearanceDocumentEdit::RemoveDefinition { sprite_id: 2 }],
        )
        .unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.undo(0),
        Err(OverworldAppearanceDocumentControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);
}
