use super::*;
use lm_level::{CustomSpriteEntry, SpriteRecord};

fn controller() -> CustomSpriteLibraryController {
    CustomSpriteLibraryController::decode(
        "sprites.mw2".into(),
        "sprites.mwt".into(),
        &[0x5a, 1, 2, 3, 5, 4, 5, 0xff],
        b"First\nSecond\n",
        SpriteLengthTable::standard(),
    )
    .unwrap()
}

fn entry(bytes: &[&[u8]], description: &str) -> CustomSpriteEntry {
    CustomSpriteEntry::new(
        bytes
            .iter()
            .map(|bytes| SpriteRecord {
                encoded: bytes.to_vec(),
            })
            .collect(),
        description.into(),
    )
    .unwrap()
}

#[test]
fn mixed_batch_is_atomic_revisioned_and_saveable() {
    let mut controller = controller();
    controller
        .apply_edits(
            0,
            &[
                CustomSpriteLibraryEdit::Replace {
                    index: 0,
                    entry: entry(&[&[1, 8, 9], &[0, 10, 11]], "Pair"),
                },
                CustomSpriteLibraryEdit::SetHeader(0x44),
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert!(controller.is_modified());
    assert_eq!(controller.library().header(), 0x44);
    assert_eq!(controller.library().entries()[0].sprites.len(), 2);
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.data, [0x44, 1, 8, 9, 0, 10, 11, 5, 4, 5, 0xff]);
    controller.acknowledge_save(snapshot.request_id).unwrap();
    assert!(!controller.is_modified());
}

#[test]
fn late_edit_and_length_failure_preserve_revision_and_library() {
    let mut controller = controller();
    let before = controller.library().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                CustomSpriteLibraryEdit::Remove { index: 0 },
                CustomSpriteLibraryEdit::Remove { index: 9 }
            ]
        ),
        Err(CustomSpriteControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.library(), &before);
    let long = entry(&[&[1, 2, 3, 4]], "bad width");
    assert!(matches!(
        controller.apply_edits(
            0,
            &[CustomSpriteLibraryEdit::Replace {
                index: 0,
                entry: long
            }]
        ),
        Err(CustomSpriteControllerError::Edit {
            command: 1,
            error: CustomSpriteLibraryError::SpriteLengthMismatch { .. }
        })
    ));
    assert_eq!(controller.library(), &before);
    assert_eq!(controller.revision(), 0);
}

#[test]
fn pending_snapshot_acknowledges_exact_state_and_failed_save_is_retryable() {
    let mut controller = controller();
    let snapshot = controller.begin_save().unwrap();
    controller
        .apply_edits(0, &[CustomSpriteLibraryEdit::SetHeader(0x77)])
        .unwrap();
    assert!(matches!(
        controller.acknowledge_save(snapshot.request_id + 1),
        Err(CustomSpriteControllerError::StaleSave { .. })
    ));
    assert!(controller.save_pending());
    controller.cancel_save(snapshot.request_id).unwrap();
    let retry = controller.begin_save().unwrap();
    controller.acknowledge_save(retry.request_id).unwrap();
    assert!(!controller.is_modified());
}

#[test]
fn stale_revision_aliases_and_overlapping_saves_are_rejected() {
    assert!(matches!(
        CustomSpriteLibraryController::decode(
            "same".into(),
            "same".into(),
            &[0, 0xff],
            b"",
            SpriteLengthTable::standard()
        ),
        Err(CustomSpriteControllerError::AliasedPaths)
    ));
    let mut controller = controller();
    assert!(matches!(
        controller.apply_edits(4, &[]),
        Err(CustomSpriteControllerError::StaleRevision { .. })
    ));
    let _snapshot = controller.begin_save().unwrap();
    assert_eq!(
        controller.begin_save(),
        Err(CustomSpriteControllerError::SavePending)
    );
}

#[test]
fn history_restores_pair_under_immutable_length_interpretation() {
    let mut controller = controller();
    let original = controller.library().clone();
    let lengths = controller.sprite_lengths().clone();
    controller
        .apply_edits(
            0,
            &[
                CustomSpriteLibraryEdit::Replace {
                    index: 0,
                    entry: entry(&[&[1, 8, 9], &[0, 10, 11]], "Pair"),
                },
                CustomSpriteLibraryEdit::SetHeader(0x44),
            ],
        )
        .unwrap();
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.library(), &original);
    assert_eq!(controller.sprite_lengths(), &lengths);
    assert!(!controller.is_modified());
    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.library().header(), 0x44);
    assert_eq!(controller.library().entries()[0].sprites.len(), 2);
    assert!(controller.undo(3).unwrap());
    controller
        .apply_edits(4, &[CustomSpriteLibraryEdit::SetHeader(0x66)])
        .unwrap();
    assert!(!controller.can_redo());
    assert!(matches!(
        controller.undo(4),
        Err(CustomSpriteControllerError::StaleRevision { .. })
    ));
}
