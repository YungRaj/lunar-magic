use super::*;
use lm_level::{
    Entrance, EntranceKind, Layer3Data, Layer3Edit, LegacyHeaderEdit, LevelLayer,
    LevelPropertyEdit, Map16OverrideEdit, Map16Tile, ObjectEdit, ObjectRecord, SequenceEdit,
    SpriteEdit, SpriteRecord,
};

fn controller() -> CompleteLevelDocumentController {
    let file = CompleteLevelFile(lm_level::Level::default());
    CompleteLevelDocumentController::decode("level.lmlevel".into(), &file.encode().unwrap())
        .unwrap()
}

const fn entrance() -> Entrance {
    Entrance {
        kind: EntranceKind::Main,
        x: 0,
        y: 0,
        screen: 0,
        action: 0,
        raw_flags: 0,
    }
}

#[test]
fn edits_are_revisioned_and_round_trip_canonically() {
    let mut controller = controller();
    let edits = [
        LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
            index: 0,
            value: entrance(),
        }),
        LevelAuxiliaryEdit::Map16Override(Map16OverrideEdit::Upsert {
            index: 42,
            tile: Map16Tile::default(),
        }),
    ];
    controller.apply_auxiliary_edits(0, &edits).unwrap();
    assert_eq!(controller.revision(), 1);
    assert!(controller.is_modified());
    let bytes = controller.begin_save().unwrap().bytes;
    assert_eq!(
        CompleteLevelFile::decode(&bytes).unwrap(),
        *controller.value()
    );
}

#[test]
fn stale_and_late_invalid_batches_are_atomic_and_no_ops_keep_revision() {
    let mut controller = controller();
    assert!(matches!(
        controller.apply_auxiliary_edits(1, &[]),
        Err(CompleteLevelDocumentControllerError::StaleRevision { .. })
    ));
    let before = controller.value().clone();
    let edits = [
        LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
            index: 0,
            value: entrance(),
        }),
        LevelAuxiliaryEdit::Entrance(SequenceEdit::Remove { index: 9 }),
    ];
    assert!(matches!(
        controller.apply_auxiliary_edits(0, &edits),
        Err(CompleteLevelDocumentControllerError::Edit(_))
    ));
    assert_eq!(controller.value(), &before);
    controller.apply_auxiliary_edits(0, &[]).unwrap();
    assert_eq!(controller.revision(), 0);
}

#[test]
fn mixed_domain_batch_is_one_canonical_history_revision() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                CompleteLevelDocumentEdit::Property(LevelPropertyEdit::LegacyHeader(
                    LegacyHeaderEdit::LevelMode(3),
                )),
                CompleteLevelDocumentEdit::LayerObject {
                    layer: LevelLayer::Layer1,
                    edit: ObjectEdit::Insert {
                        index: 0,
                        record: ObjectRecord::new(vec![1, 2, 3]).unwrap(),
                    },
                },
                CompleteLevelDocumentEdit::Sprite(SpriteEdit::Insert {
                    index: 0,
                    record: SpriteRecord {
                        encoded: vec![4, 5, 6],
                    },
                }),
                CompleteLevelDocumentEdit::Layer3(Layer3Edit::Enable(Layer3Data::default())),
                CompleteLevelDocumentEdit::Auxiliary(LevelAuxiliaryEdit::Entrance(
                    SequenceEdit::Insert {
                        index: 0,
                        value: entrance(),
                    },
                )),
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().0.header.legacy.level_mode(), 3);
    assert_eq!(controller.value().0.layer1.objects.records.len(), 1);
    assert_eq!(controller.value().0.sprites.records.len(), 1);
    assert!(controller.value().0.layer3.is_some());
    assert_eq!(controller.value().0.entrances.len(), 1);
    assert!(controller.undo(1).unwrap());
    assert!(controller.value().0.layer1.objects.records.is_empty());
    assert!(controller.value().0.sprites.records.is_empty());
    assert!(controller.value().0.layer3.is_none());
    assert!(controller.value().0.entrances.is_empty());
}

#[test]
fn invalid_late_domain_edit_rolls_back_earlier_domains() {
    let mut controller = controller();
    let before = controller.value().clone();
    let result = controller.apply_edits(
        0,
        &[
            CompleteLevelDocumentEdit::Property(LevelPropertyEdit::LegacyHeader(
                LegacyHeaderEdit::LevelMode(2),
            )),
            CompleteLevelDocumentEdit::Sprite(SpriteEdit::Remove { index: 0 }),
        ],
    );
    assert!(matches!(
        result,
        Err(CompleteLevelDocumentControllerError::Domain { command: 1, .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 0);
}

#[test]
fn saves_are_correlated_and_older_snapshots_leave_newer_edits_dirty() {
    let mut controller = controller();
    controller
        .apply_auxiliary_edits(
            0,
            &[LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
                index: 0,
                value: entrance(),
            })],
        )
        .unwrap();
    let save = controller.begin_save().unwrap();
    assert!(matches!(
        controller.begin_save(),
        Err(CompleteLevelDocumentControllerError::SavePending)
    ));
    assert!(controller.cancel_save(save.request_id + 1).is_err());
    assert!(controller.save_pending());
    controller
        .apply_auxiliary_edits(
            1,
            &[LevelAuxiliaryEdit::Map16Override(
                Map16OverrideEdit::Upsert {
                    index: 7,
                    tile: Map16Tile::default(),
                },
            )],
        )
        .unwrap();
    controller.acknowledge_save(save.request_id).unwrap();
    assert!(controller.is_modified());
    assert!(!controller.save_pending());
}

#[test]
fn counter_overflows_do_not_mutate_the_document() {
    let mut revision_controller = controller();
    revision_controller.revision = u64::MAX;
    let before = revision_controller.value().clone();
    assert!(matches!(
        revision_controller.apply_auxiliary_edits(
            u64::MAX,
            &[LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
                index: 0,
                value: entrance(),
            })]
        ),
        Err(CompleteLevelDocumentControllerError::RevisionOverflow)
    ));
    assert_eq!(revision_controller.value(), &before);

    let mut controller = controller();
    controller.next_save_request = u64::MAX;
    assert!(matches!(
        controller.begin_save(),
        Err(CompleteLevelDocumentControllerError::SaveRequestOverflow)
    ));
    assert!(!controller.save_pending());
}

#[test]
fn history_is_monotonic_tracks_save_baseline_and_invalidates_redo() {
    let mut controller = controller();
    controller
        .apply_auxiliary_edits(
            0,
            &[LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
                index: 0,
                value: entrance(),
            })],
        )
        .unwrap();
    let saved = controller.value().clone();
    let snapshot = controller.begin_save().unwrap();
    controller.acknowledge_save(snapshot.request_id).unwrap();
    controller
        .apply_auxiliary_edits(
            1,
            &[LevelAuxiliaryEdit::Map16Override(
                Map16OverrideEdit::Upsert {
                    index: 7,
                    tile: Map16Tile::default(),
                },
            )],
        )
        .unwrap();
    assert!(controller.undo(2).unwrap());
    assert_eq!(controller.revision(), 3);
    assert_eq!(controller.value(), &saved);
    assert!(!controller.is_modified());
    assert!(controller.redo(3).unwrap());
    assert_eq!(controller.revision(), 4);
    assert!(controller.is_modified());
    assert!(controller.undo(4).unwrap());
    controller
        .apply_auxiliary_edits(
            5,
            &[LevelAuxiliaryEdit::Map16Override(
                Map16OverrideEdit::Upsert {
                    index: 9,
                    tile: Map16Tile::default(),
                },
            )],
        )
        .unwrap();
    assert!(!controller.can_redo());
    assert!(matches!(
        controller.undo(5),
        Err(CompleteLevelDocumentControllerError::StaleRevision { .. })
    ));
}
