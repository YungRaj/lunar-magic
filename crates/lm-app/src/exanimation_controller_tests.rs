use super::*;
use crate::{AppError, AppState, Command, FrontendEffect};
use lm_project::{
    ExAnimationSaveOptions, GatedLayout, InstallationMarker, InstalledExAnimationRomLayout,
    InstalledLayout, LevelPointerTable, Project, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::RomImage;

const GLOBAL_RUNTIME: usize = 0x6000;

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

fn global_installation() -> InstalledLayout<InstalledExAnimationRomLayout> {
    InstalledLayout::Alternatives {
        primary: GatedLayout {
            marker: InstallationMarker {
                offset: 0x80,
                expected: 0x22,
            },
            layout: InstalledExAnimationRomLayout {
                payload: layout(),
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: Some(lm_project::ChainedSnesPointerLocator {
                    mapper: Mapper::LoRom,
                    first_operand_offset: 0x81,
                    final_operand_displacement: -0x20,
                }),
            },
        },
        fallback: None,
    }
}

fn global_options(search: std::ops::Range<usize>) -> ExAnimationSaveOptions {
    ExAnimationSaveOptions {
        allocation: AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![
                ProtectedRange(0x80..0x84),
                ProtectedRange(GLOBAL_RUNTIME + 0x5c..GLOBAL_RUNTIME + 0x5f),
                ProtectedRange(0x7fdc..0x7fe0),
            ],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn global_test_rom() -> Vec<u8> {
    let mut project = Project::new(RomImage::from_bytes(test_rom()).unwrap());
    project.rom.write(0x80, &[0x22]).unwrap();
    let pointer = lm_rom::pc_to_snes(Mapper::LoRom, GLOBAL_RUNTIME)
        .unwrap()
        .to_le_bytes();
    project.rom.write(0x81, &pointer[..3]).unwrap();
    project
        .rom
        .write(GLOBAL_RUNTIME + 0x5c, &[0, 0, 0])
        .unwrap();
    project
        .save_installed_global_exanimation_with_checksum(
            &animation(),
            global_installation(),
            &MODES,
            0x7fdc,
            &global_options(0x4000..0x5800),
        )
        .unwrap();
    project.save_snapshot()
}

fn empty_global_test_rom() -> Vec<u8> {
    let mut project = Project::new(RomImage::from_bytes(test_rom()).unwrap());
    project.rom.write(0x80, &[0x22]).unwrap();
    let pointer = lm_rom::pc_to_snes(Mapper::LoRom, GLOBAL_RUNTIME)
        .unwrap()
        .to_le_bytes();
    project.rom.write(0x81, &pointer[..3]).unwrap();
    project
        .rom
        .write(GLOBAL_RUNTIME + 0x5c, &[0, 0, 0])
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    project.save_snapshot()
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
fn installed_global_controller_edits_commits_reopens_and_undoes() {
    let original = global_test_rom();
    let mut app = AppState::default();
    app.load_rom(original.clone()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        ExAnimationController::decode_global(&snapshot, global_installation(), &MODES).unwrap();
    assert_eq!(controller.animation(), &animation());
    assert!(controller.previous_block.is_some());
    controller
        .apply_edits(&[
            ExAnimationControllerEdit::SetHeaderValue(0x1234_abcd),
            ExAnimationControllerEdit::ReplaceRecord {
                index: 0,
                record: record(3, 0x20),
            },
        ])
        .unwrap();

    let prepared = controller
        .prepare_commit("Edit global ExAnimation", &global_options(0x8000..0x10000))
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    assert_eq!(
        app.project()
            .unwrap()
            .load_installed_global_exanimation(global_installation(), &MODES)
            .unwrap(),
        lm_project::InstalledAsset::Present(controller.animation().clone())
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), original);
}

#[test]
fn installed_empty_global_controller_allocates_the_first_record_transactionally() {
    let original = empty_global_test_rom();
    let mut app = AppState::default();
    app.load_rom(original.clone()).unwrap();
    app.dispatch(Command::ShowExAnimation(1)).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    let mut controller =
        ExAnimationController::decode_global(&snapshot, global_installation(), &MODES).unwrap();
    assert!(controller.animation().records.is_empty());
    assert_eq!(controller.previous_block, None);
    controller
        .apply_edits(&[
            ExAnimationControllerEdit::SetSetting(2),
            ExAnimationControllerEdit::InsertRecord {
                index: 0,
                record: record(1, 0x30),
            },
        ])
        .unwrap();

    let prepared = controller
        .prepare_commit(
            "Create global ExAnimation",
            &global_options(0x8000..0x10000),
        )
        .unwrap();
    app.dispatch(prepared.into_command()).unwrap();
    assert_eq!(
        app.project()
            .unwrap()
            .load_installed_global_exanimation(global_installation(), &MODES)
            .unwrap(),
        lm_project::InstalledAsset::Present(controller.animation().clone())
    );
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().save_snapshot(), original);
}

#[test]
fn installed_global_controller_matches_across_copier_header_variants() {
    fn edit(mut physical: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
        let original = physical.clone();
        let mut app = AppState::default();
        app.load_rom(std::mem::take(&mut physical)).unwrap();
        app.dispatch(Command::ShowExAnimation(1)).unwrap();
        let snapshot = app.controller_snapshot().unwrap();
        let mut controller =
            ExAnimationController::decode_global(&snapshot, global_installation(), &MODES).unwrap();
        controller
            .apply_edits(&[
                ExAnimationControllerEdit::SetSetting(7),
                ExAnimationControllerEdit::SetTrigger {
                    trigger: 5,
                    value: Some(0x44),
                },
            ])
            .unwrap();
        let prepared = controller
            .prepare_commit(
                "Variant global ExAnimation",
                &global_options(0x8000..0x10000),
            )
            .unwrap();
        app.dispatch(prepared.into_command()).unwrap();
        let changed = app.project().unwrap().save_snapshot();
        app.dispatch(Command::Undo).unwrap();
        assert_eq!(app.project().unwrap().save_snapshot(), original);
        (changed, controller.animation().encode(&MODES).unwrap())
    }

    let headerless = global_test_rom();
    let mut headered = vec![0x5a; 0x200];
    headered.extend_from_slice(&headerless);
    let (headerless_changed, expected_animation) = edit(headerless);
    let (headered_changed, headered_animation) = edit(headered);

    assert_eq!(&headered_changed[..0x200], &[0x5a; 0x200]);
    assert_eq!(headered_animation, expected_animation);
    assert_eq!(
        RomImage::from_bytes(headered_changed)
            .unwrap()
            .logical_bytes(),
        RomImage::from_bytes(headerless_changed)
            .unwrap()
            .logical_bytes()
    );
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
