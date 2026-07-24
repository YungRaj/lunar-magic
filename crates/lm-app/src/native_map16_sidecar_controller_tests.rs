use super::*;

fn controller(kind: NativeMap16SidecarDocumentKind) -> NativeMap16SidecarController {
    let bytes = match kind {
        NativeMap16SidecarDocumentKind::M16 => vec![0; M16Sidecar::ENCODED_LEN],
        NativeMap16SidecarDocumentKind::S16 => Vec::new(),
    };
    NativeMap16SidecarController::decode("sidecar".into(), kind, &bytes).unwrap()
}

#[test]
fn batch_is_atomic_revisioned_and_s16_snapshot_is_canonical() {
    let mut controller = controller(NativeMap16SidecarDocumentKind::S16);
    controller
        .apply_edits(
            0,
            &[
                NativeMap16SidecarEdit { entry: 0, value: 1 },
                NativeMap16SidecarEdit {
                    entry: 0x200,
                    value: 2,
                },
            ],
        )
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert!(controller.is_modified());
    let snapshot = controller.begin_save().unwrap();
    assert_eq!(snapshot.bytes.len(), 0x1000);
    controller.acknowledge_save(snapshot.request_id).unwrap();
    assert!(!controller.is_modified());
}

#[test]
fn late_invalid_entry_and_stale_revision_preserve_everything() {
    let mut controller = controller(NativeMap16SidecarDocumentKind::M16);
    let before = controller.value().clone();
    assert!(matches!(
        controller.apply_edits(
            0,
            &[
                NativeMap16SidecarEdit { entry: 0, value: 1 },
                NativeMap16SidecarEdit {
                    entry: M16Sidecar::ENTRY_COUNT,
                    value: 2
                }
            ]
        ),
        Err(NativeMap16SidecarControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.value(), &before);
    assert!(matches!(
        controller.apply_edits(9, &[]),
        Err(NativeMap16SidecarControllerError::StaleRevision { .. })
    ));
}

#[test]
fn pending_snapshot_survives_edits_and_bad_tokens_are_retryable() {
    let mut controller = controller(NativeMap16SidecarDocumentKind::M16);
    let snapshot = controller.begin_save().unwrap();
    controller
        .apply_edits(0, &[NativeMap16SidecarEdit { entry: 1, value: 7 }])
        .unwrap();
    assert!(matches!(
        controller.acknowledge_save(snapshot.request_id + 1),
        Err(NativeMap16SidecarControllerError::StaleSave { .. })
    ));
    assert!(controller.save_pending());
    controller.cancel_save(snapshot.request_id).unwrap();
    let retry = controller.begin_save().unwrap();
    controller.acknowledge_save(retry.request_id).unwrap();
    assert!(!controller.is_modified());
}

#[test]
fn history_preserves_kind_saved_baseline_and_canonical_s16_bytes() {
    let mut controller = controller(NativeMap16SidecarDocumentKind::S16);
    controller
        .apply_edits(
            0,
            &[NativeMap16SidecarEdit {
                entry: 0x200,
                value: 2,
            }],
        )
        .unwrap();
    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.revision(), 2);
    assert_eq!(
        controller.value().kind(),
        NativeMap16SidecarDocumentKind::S16
    );
    assert!(!controller.is_modified());
    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.revision(), 3);
    assert_eq!(
        controller.value().kind(),
        NativeMap16SidecarDocumentKind::S16
    );
    assert_eq!(controller.value().entry(0x200), Some(2));
    assert_eq!(controller.begin_save().unwrap().bytes.len(), 0x1000);
}

#[test]
fn history_rejects_stale_tokens_and_divergent_edits_clear_redo() {
    let mut controller = controller(NativeMap16SidecarDocumentKind::M16);
    controller
        .apply_edits(0, &[NativeMap16SidecarEdit { entry: 1, value: 7 }])
        .unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.undo(0),
        Err(NativeMap16SidecarControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);

    assert!(controller.undo(1).unwrap());
    controller
        .apply_edits(2, &[NativeMap16SidecarEdit { entry: 2, value: 9 }])
        .unwrap();
    assert!(!controller.can_redo());
    assert!(!controller.redo(3).unwrap());
    assert_eq!(controller.revision(), 3);
}
