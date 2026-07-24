use super::*;
use lm_level::AppearanceSource;

fn record(id: u32, x: i32) -> EntityAppearanceRecord {
    EntityAppearanceRecord {
        source: AppearanceSource::Sprite(id),
        tile_index: u16::try_from(id).unwrap(),
        palette_index: 3,
        x,
        y: -8,
        x_flip: false,
        y_flip: true,
    }
}

fn controller() -> EntityAppearanceDocumentController {
    let file = EntityAppearanceFile {
        appearances: vec![record(1, 10), record(2, 20)],
    };
    EntityAppearanceDocumentController::decode("entities.lmentapp".into(), &file.encode().unwrap())
        .unwrap()
}

#[test]
fn ordered_mixed_edits_are_atomic_canonical_and_preserve_painter_order() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                EntityAppearanceDocumentEdit::Insert {
                    index: 1,
                    value: record(3, 30),
                },
                EntityAppearanceDocumentEdit::Replace {
                    index: 0,
                    value: record(4, 40),
                },
                EntityAppearanceDocumentEdit::MoveBefore { from: 2, before: 0 },
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(
        controller
            .value()
            .appearances
            .iter()
            .map(|value| value.x)
            .collect::<Vec<_>>(),
        [20, 40, 30]
    );
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(
        EntityAppearanceFile::decode(&snapshot.bytes).unwrap(),
        *controller.value()
    );
}

#[test]
fn late_index_and_file_validation_failures_roll_back_every_record() {
    let mut controller = controller();
    let original = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                EntityAppearanceDocumentEdit::Remove { index: 0 },
                EntityAppearanceDocumentEdit::Replace {
                    index: 9,
                    value: record(3, 30),
                },
            ]
        ),
        Err(EntityAppearanceDocumentControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.value(), &original);
    let mut bad = record(3, 30);
    bad.palette_index = 8;
    assert!(matches!(
        controller.apply_edits(
            0,
            &[EntityAppearanceDocumentEdit::Replace {
                index: 0,
                value: bad
            }]
        ),
        Err(EntityAppearanceDocumentControllerError::File(
            EntityAppearanceFileError::PaletteOutOfRange(8)
        ))
    ));
    assert_eq!(controller.value(), &original);
}

#[test]
fn revisions_and_immutable_save_acknowledgements_are_retryable() {
    let mut controller = controller();
    assert!(matches!(
        controller.apply_edits(1, &[]),
        Err(EntityAppearanceDocumentControllerError::StaleRevision { .. })
    ));
    controller
        .apply_edits(0, &[EntityAppearanceDocumentEdit::Remove { index: 0 }])
        .unwrap();
    let first = controller.begin_save().unwrap();
    controller
        .apply_edits(
            1,
            &[EntityAppearanceDocumentEdit::Insert {
                index: 0,
                value: record(5, 50),
            }],
        )
        .unwrap();
    assert!(matches!(
        controller.begin_save(),
        Err(EntityAppearanceDocumentControllerError::SavePending)
    ));
    assert!(controller.acknowledge_save(first.request_id + 1).is_err());
    controller.acknowledge_save(first.request_id).unwrap();
    assert!(controller.is_modified());
    let second = controller.begin_save().unwrap();
    controller.cancel_save(second.request_id).unwrap();
    assert!(!controller.save_pending());
}

#[test]
fn ordered_history_restores_saved_baseline_and_invalidates_divergent_redo() {
    let mut controller = controller();
    let original = controller.value().clone();
    controller
        .apply_edits(
            0,
            &[EntityAppearanceDocumentEdit::MoveBefore { from: 1, before: 0 }],
        )
        .unwrap();
    assert!(controller.can_undo());
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.value(), &original);
    assert!(!controller.is_modified());
    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.value().appearances[0].x, 20);
    assert!(controller.undo(3).unwrap());
    controller
        .apply_edits(
            4,
            &[EntityAppearanceDocumentEdit::Replace {
                index: 0,
                value: record(5, 50),
            }],
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
        .apply_edits(0, &[EntityAppearanceDocumentEdit::Remove { index: 0 }])
        .unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.undo(0),
        Err(EntityAppearanceDocumentControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);
}
