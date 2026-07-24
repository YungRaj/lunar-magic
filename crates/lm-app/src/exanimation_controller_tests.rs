use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_project::{ExAnimationSaveOptions, LevelPointerTable, Project, RatsOwnershipManifest};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

const MODES: [bool; 256] = [false; 256];

fn layout() -> ExAnimationRomLayout {
    ExAnimationRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x200,
            entries: 2,
            stride: 3,
        },
        maximum_records: 32,
        maximum_encoded_len: 0x4000,
    }
}

fn record(kind: u8, value: u8) -> ExAnimationRecord {
    ExAnimationRecord::new(kind, 0, 0, 0x1234, true, &[value, value + 1], false).unwrap()
}

fn animation() -> CompactExAnimation {
    let mut trigger_values = [0; 16];
    trigger_values[2] = 9;
    CompactExAnimation {
        setting: 3,
        header_value: 0x9234_5678,
        trigger_mask: 4,
        trigger_values,
        records: vec![record(1, 4), record(2, 8)],
    }
}

fn test_rom() -> Vec<u8> {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    project
        .save_exanimation(
            1,
            &animation(),
            layout(),
            &MODES,
            &ExAnimationSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x4000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x200..0x206)],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    project.save_snapshot()
}

fn options() -> ExAnimationSaveOptions {
    ExAnimationSaveOptions {
        allocation: AllocationPolicy {
            search: 0x8000..0x10000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x200..0x206), ProtectedRange(0x7fdc..0x7fe0)],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

#[test]
fn edit_expand_dispatch_reload_and_undo_compact_semantics() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();
    controller
        .apply_edits(&[
            ExAnimationControllerEdit::SetHeaderValue(0xdead_beef),
            ExAnimationControllerEdit::SetTrigger {
                trigger: 15,
                value: Some(0xaa),
            },
            ExAnimationControllerEdit::ReplaceRecord {
                index: 1,
                record: record(3, 12),
            },
            ExAnimationControllerEdit::EditRecordFrames {
                record: 1,
                edits: vec![ExAnimationFrameEdit::Insert {
                    index: 1,
                    frame: lm_graphics::ExAnimationFrame {
                        source_words: vec![0x2222],
                    },
                }],
            },
            ExAnimationControllerEdit::MoveRecordBefore { from: 1, before: 0 },
        ])
        .unwrap();
    assert_eq!(
        lm_graphics::exanimation_frames(&controller.animation().records[0], false).unwrap(),
        [
            lm_graphics::ExAnimationFrame {
                source_words: vec![0x0d0c]
            },
            lm_graphics::ExAnimationFrame {
                source_words: vec![0x2222]
            }
        ]
    );
    let prepared = controller
        .prepare_commit("Edit ExAnimation 001", &options())
        .unwrap();
    assert_eq!(prepared.mutation.appended.len(), 0x8000);
    assert_eq!(
        app.dispatch(prepared.into_command()).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Edit ExAnimation 001".into(),
            mode: EditorMode::ExAnimation(1),
            revision: 1,
        }]
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_exanimation(1, layout(), &MODES)
            .unwrap(),
        *controller.animation()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    app.dispatch(Command::Redo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
}

#[test]
fn owned_exanimation_commit_reclaims_snapshot_block_and_undo_restores_it() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();
    let previous = controller.previous_block.clone().unwrap();
    controller
        .apply_edits(&[ExAnimationControllerEdit::SetHeaderValue(0x1122_3344)])
        .unwrap();
    let prepared = controller
        .prepare_commit_with_reclamation(
            "Owned ExAnimation edit",
            &options(),
            &RatsOwnershipManifest {
                owned: vec![previous.clone()],
                retained: Vec::new(),
            },
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    assert!(
        app.project().unwrap().rom.logical_bytes()[previous.full_range()]
            .iter()
            .all(|byte| *byte == 0xff)
    );
    assert_eq!(
        app.project()
            .unwrap()
            .load_exanimation(1, layout(), &MODES)
            .unwrap(),
        *controller.animation()
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(
        lm_rats::parse_at(
            app.project().unwrap().rom.logical_bytes(),
            previous.header_offset
        )
        .unwrap(),
        previous
    );
}

#[test]
fn clipboard_copy_cut_and_paste_use_controller_frame_transactions() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();

    let copied = crate::copy_exanimation_frames(
        &controller,
        0,
        crate::ClipboardKind::ExAnimationFrames,
        &[0],
    )
    .unwrap();
    crate::paste_exanimation_frames(&mut controller, 0, 1, &copied).unwrap();
    assert_eq!(controller.record_frames(0).unwrap().len(), 2);
    let cut = crate::cut_exanimation_frames(
        &mut controller,
        0,
        crate::ClipboardKind::ExAnimationFrames,
        &[0],
    )
    .unwrap();
    assert_eq!(cut.to_exanimation_frames().unwrap().len(), 1);
    assert_eq!(controller.record_frames(0).unwrap().len(), 1);

    let before = controller.animation().clone();
    let double =
        crate::ClipboardPayload::from_exanimation_frames(&[lm_graphics::ExAnimationFrame {
            source_words: vec![1, 2],
        }])
        .unwrap();
    assert!(crate::paste_exanimation_frames(&mut controller, 0, 1, &double).is_err());
    assert_eq!(controller.animation(), &before);
    assert!(matches!(
        crate::copy_exanimation_frames(
            &controller,
            0,
            crate::ClipboardKind::ExAnimationFrames,
            &[0, 0]
        ),
        Err(crate::ExAnimationClipboardError::DuplicateFrame(0))
    ));
}

#[test]
fn unrepresented_record_bytes_roll_back_earlier_animation_edits() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();
    let original = controller.animation().clone();
    let mut bytes = [0; ExAnimationRecord::ENCODED_LEN];
    bytes[0] = 2;
    bytes[3] = 0xaa;
    let invalid = ExAnimationRecord::decode(&bytes).unwrap();
    assert!(matches!(
        controller.apply_edits(&[
            ExAnimationControllerEdit::SetHeaderValue(7),
            ExAnimationControllerEdit::ReplaceRecord {
                index: 0,
                record: invalid,
            },
        ]),
        Err(ExAnimationControllerError::Edit {
            command: 1,
            error: ExAnimationControllerEditFailure::Encoding(
                ExAnimationError::UnrepresentedRecordByte {
                    record: 0,
                    offset: 3,
                    value: 0xaa
                }
            )
        })
    ));
    assert_eq!(controller.animation(), &original);
    assert!(!controller.is_modified());
}

#[test]
fn edits_trimmed_by_compact_encoding_are_rejected_atomically() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();
    let original = controller.animation().clone();
    assert!(matches!(
        controller.apply_edits(&[
            ExAnimationControllerEdit::SetSetting(9),
            ExAnimationControllerEdit::InsertRecord {
                index: original.records.len(),
                record: ExAnimationRecord::inactive(),
            },
        ]),
        Err(ExAnimationControllerError::Edit {
            command: 1,
            error: ExAnimationControllerEditFailure::NonCanonicalEncoding,
        })
    ));
    assert_eq!(controller.animation(), &original);
    assert!(!controller.is_modified());
}

#[test]
fn late_edit_failure_rolls_back_and_stale_commit_is_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller = ExAnimationController::decode(&snapshot, layout(), &MODES).unwrap();
    assert!(!controller.is_modified());
    assert!(
        controller
            .prepare_commit("unchanged", &options())
            .unwrap()
            .mutation
            .is_empty()
    );
    let original = controller.animation().clone();
    assert!(matches!(
        controller.apply_edits(&[
            ExAnimationControllerEdit::SetSetting(9),
            ExAnimationControllerEdit::SetTrigger {
                trigger: 16,
                value: Some(1),
            },
        ]),
        Err(ExAnimationControllerError::Edit { command: 1, .. })
    ));
    assert_eq!(controller.animation(), &original);
    let prepared = controller.prepare_commit("stale", &options()).unwrap();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "newer".into(),
        writes: vec![lm_project::RomWrite {
            offset: 1,
            bytes: vec![7],
        }],
    })
    .unwrap();
    assert!(matches!(
        app.dispatch(prepared.into_command()),
        Err(AppError::StaleProjectRevision { .. })
    ));
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
}

#[test]
fn wrong_mode_mapper_and_size_mode_count_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let level_snapshot = app.controller_snapshot().unwrap();
    assert!(matches!(
        ExAnimationController::decode(&level_snapshot, layout(), &MODES),
        Err(ExAnimationControllerError::WrongMode(EditorMode::Level(
            0x105
        )))
    ));
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut wrong_mapper = layout();
    wrong_mapper.mapper = Mapper::Sa1;
    assert!(matches!(
        ExAnimationController::decode(&snapshot, wrong_mapper, &MODES),
        Err(ExAnimationControllerError::MapperMismatch { .. })
    ));
    assert!(matches!(
        ExAnimationController::decode(&snapshot, layout(), &[false; 255]),
        Err(ExAnimationControllerError::SizeModeCount(255))
    ));
}
