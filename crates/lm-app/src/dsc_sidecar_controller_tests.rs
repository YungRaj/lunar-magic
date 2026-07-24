use super::*;
use lm_level::MAX_DSC_SOURCE_LEN;

#[test]
fn replacement_is_atomic_revisioned_and_semantically_reparsed() {
    let mut controller = DscSidecarController::decode("test.dsc".into(), b"10\t0\told\n").unwrap();
    controller
        .replace_source(0, b"10\t0\tnew\n11\t2\t1234\n")
        .unwrap();
    assert_eq!(controller.revision(), 1);
    assert_eq!(controller.value().entries().len(), 2);
    assert!(controller.is_modified());

    let before = controller.value().clone();
    assert!(matches!(
        controller.replace_source(0, b"20\t0\tstale\n"),
        Err(DscSidecarControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    let oversized = vec![0; MAX_DSC_SOURCE_LEN + 1];
    assert!(controller.replace_source(1, &oversized).is_err());
    assert_eq!(controller.value(), &before);
}

#[test]
fn save_acknowledges_the_snapshot_not_later_edits() {
    let mut controller = DscSidecarController::decode("test.dsc".into(), b"10\t0\tone\n").unwrap();
    controller.replace_source(0, b"10\t0\ttwo\n").unwrap();
    let save = controller.begin_save().unwrap();
    controller.replace_source(1, b"10\t0\tthree\n").unwrap();
    assert_eq!(save.bytes, b"10\t0\ttwo\n");
    assert!(controller.acknowledge_save(save.request_id + 1).is_err());
    assert!(controller.save_pending());
    controller.acknowledge_save(save.request_id).unwrap();
    assert!(controller.is_modified());

    let latest = controller.begin_save().unwrap();
    controller.acknowledge_save(latest.request_id).unwrap();
    assert!(!controller.is_modified());
}

#[test]
fn cancelled_save_is_retryable() {
    let mut controller = DscSidecarController::decode("test.dsc".into(), b"").unwrap();
    let save = controller.begin_save().unwrap();
    assert!(controller.cancel_save(save.request_id + 1).is_err());
    assert!(controller.save_pending());
    controller.cancel_save(save.request_id).unwrap();
    assert!(!controller.save_pending());
    assert_eq!(controller.begin_save().unwrap().request_id, 1);
}

#[test]
fn history_preserves_saved_baseline_and_uses_monotonic_revisions() {
    let original = b"10\t0\tone\n";
    let replacement = b"10\t0\ttwo\n";
    let mut controller = DscSidecarController::decode("test.dsc".into(), original).unwrap();
    controller.replace_source(0, replacement).unwrap();
    assert!(controller.can_undo());

    assert!(controller.undo(1).unwrap());
    assert_eq!(controller.revision(), 2);
    assert_eq!(controller.value().encode_lossless(), original);
    assert!(!controller.is_modified());
    assert!(controller.can_redo());

    assert!(controller.redo(2).unwrap());
    assert_eq!(controller.revision(), 3);
    assert_eq!(controller.value().encode_lossless(), replacement);
    assert!(controller.is_modified());
}

#[test]
fn history_rejects_stale_tokens_and_divergent_edits_clear_redo() {
    let mut controller = DscSidecarController::decode("test.dsc".into(), b"10\t0\tone\n").unwrap();
    controller.replace_source(0, b"10\t0\ttwo\n").unwrap();
    let before = controller.value().clone();
    assert!(matches!(
        controller.undo(0),
        Err(DscSidecarControllerError::StaleRevision { .. })
    ));
    assert_eq!(controller.value(), &before);
    assert_eq!(controller.revision(), 1);

    assert!(controller.undo(1).unwrap());
    controller.replace_source(2, b"10\t0\tdiverged\n").unwrap();
    assert!(!controller.can_redo());
    assert!(!controller.redo(3).unwrap());
    assert_eq!(controller.revision(), 3);
}
