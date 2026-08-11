use super::*;

#[test]
fn controller_writes_commit_atomically_and_invalidate_the_active_view() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let effects = app
        .dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "Paint Map16 tiles".into(),
            writes: vec![
                lm_project::RomWrite {
                    offset: 1,
                    bytes: vec![4, 5],
                },
                lm_project::RomWrite {
                    offset: 20,
                    bytes: vec![6],
                },
            ],
        })
        .unwrap();
    assert_eq!(
        effects,
        [FrontendEffect::ProjectChanged {
            description: "Paint Map16 tiles".into(),
            mode: EditorMode::Map16,
            revision: 1,
        }]
    );
    assert_eq!(
        app.project.as_ref().unwrap().rom.read(1, 2).unwrap(),
        [4, 5]
    );
    assert_eq!(
        app.dispatch(Command::Undo).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Undo".into(),
            mode: EditorMode::Map16,
            revision: 2,
        }]
    );
    assert_eq!(
        app.project.as_ref().unwrap().rom.read(1, 2).unwrap(),
        [0, 0]
    );
    assert_eq!(
        app.dispatch(Command::Redo).unwrap(),
        [FrontendEffect::ProjectChanged {
            description: "Redo".into(),
            mode: EditorMode::Map16,
            revision: 3,
        }]
    );
}

#[test]
fn maintain_checksum_policy_excludes_only_the_internal_header_checksum() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let original_checksum = app.project().unwrap().rom.read(0x7fdc, 4).unwrap().to_vec();

    assert!(app.maintain_checksum());
    app.set_maintain_checksum(false);
    app.dispatch(Command::CommitRomMutation {
        expected_revision: 0,
        description: "Edit without maintaining checksum".into(),
        mutation: lm_project::RomMutation {
            mapper: lm_rom::Mapper::LoRom,
            expected_len: 0x8000,
            appended: Vec::new(),
            writes: vec![lm_project::RomWrite {
                offset: 0x7fda,
                bytes: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            }],
        },
    })
    .unwrap();

    assert_eq!(
        app.project().unwrap().rom.read(0x7fda, 2).unwrap(),
        [0x11, 0x22]
    );
    assert_eq!(
        app.project().unwrap().rom.read(0x7fdc, 4).unwrap(),
        original_checksum
    );
    assert_eq!(
        app.project().unwrap().rom.read(0x7fe0, 2).unwrap(),
        [0x77, 0x88]
    );
}

#[test]
fn controller_mutation_can_expand_and_undo_as_one_revision() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.dispatch(Command::ShowMap16).unwrap();
    let effects = app
        .dispatch(Command::CommitRomMutation {
            expected_revision: 0,
            description: "Allocate Map16 payload".into(),
            mutation: lm_project::RomMutation {
                mapper: lm_rom::Mapper::LoRom,
                expected_len: 0x8000,
                appended: vec![0xff; 0x8000],
                writes: vec![lm_project::RomWrite {
                    offset: 0x9000,
                    bytes: vec![4, 5],
                }],
            },
        })
        .unwrap();
    assert_eq!(
        effects,
        [FrontendEffect::ProjectChanged {
            description: "Allocate Map16 payload".into(),
            mode: EditorMode::Map16,
            revision: 1,
        }]
    );
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
    assert_eq!(app.project().unwrap().rom.read(0x9000, 2).unwrap(), [4, 5]);
    app.dispatch(Command::Undo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    app.dispatch(Command::Redo).unwrap();
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x10000);
    assert_eq!(app.project_revision(), 3);
}

#[test]
fn unchanged_controller_mutation_has_no_application_side_effects() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let effects = app
        .dispatch(Command::CommitRomMutation {
            expected_revision: 0,
            description: "Semantically unchanged".into(),
            mutation: lm_project::RomMutation::unchanged(lm_rom::Mapper::LoRom, 0x8000),
        })
        .unwrap();
    assert!(effects.is_empty());
    assert_eq!(app.project_revision(), 0);
    assert!(!app.project().unwrap().history.can_undo());
    assert!(!app.project().unwrap().is_modified());
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
}

#[test]
fn stale_or_wrong_length_controller_mutation_cannot_expand() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let mutation = lm_project::RomMutation {
        mapper: lm_rom::Mapper::LoRom,
        expected_len: 0x10000,
        appended: vec![0xff; 0x8000],
        writes: vec![],
    };
    assert!(matches!(
        app.dispatch(Command::CommitRomMutation {
            expected_revision: 0,
            description: "wrong image".into(),
            mutation: mutation.clone(),
        }),
        Err(AppError::Transaction(
            TransactionError::UnexpectedLogicalLength { .. }
        ))
    ));
    app.dispatch(Command::CommitRomWrites {
        expected_revision: 0,
        description: "newer edit".into(),
        writes: vec![lm_project::RomWrite {
            offset: 1,
            bytes: vec![7],
        }],
    })
    .unwrap();
    assert!(matches!(
        app.dispatch(Command::CommitRomMutation {
            expected_revision: 0,
            description: "stale growth".into(),
            mutation,
        }),
        Err(AppError::StaleProjectRevision { .. })
    ));
    assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
    assert_eq!(app.project_revision(), 1);
}

#[test]
fn unaligned_unrepresentable_and_wrong_mapper_growth_are_rejected() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    for (mapper, appended, expected) in [
        (
            lm_rom::Mapper::LoRom,
            vec![0xff],
            TransactionError::InvalidMutationExpansionSize(0x8001),
        ),
        (
            lm_rom::Mapper::LoRom,
            vec![0xff; 0x0040_0000],
            TransactionError::InvalidMutationExpansionSize(0x0040_8000),
        ),
        (
            lm_rom::Mapper::Sa1,
            vec![0xff; 0x8000],
            TransactionError::MutationMapperMismatch {
                expected: lm_rom::Mapper::LoRom,
                actual: lm_rom::Mapper::Sa1,
            },
        ),
    ] {
        let result = app.dispatch(Command::CommitRomMutation {
            expected_revision: 0,
            description: "invalid growth".into(),
            mutation: lm_project::RomMutation {
                mapper,
                expected_len: 0x8000,
                appended,
                writes: vec![],
            },
        });
        assert!(
            matches!(result, Err(AppError::Transaction(error)) if std::mem::discriminant(&error) == std::mem::discriminant(&expected))
        );
        assert_eq!(app.project().unwrap().rom.logical_len(), 0x8000);
        assert_eq!(app.project_revision(), 0);
        assert!(!app.project().unwrap().history.can_undo());
        assert!(!app.project().unwrap().is_modified());
    }
}

#[test]
fn invalid_controller_batch_and_empty_description_preserve_project() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    assert!(matches!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: " ".into(),
            writes: vec![],
        }),
        Err(AppError::EmptyEditDescription)
    ));
    assert!(matches!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "invalid".into(),
            writes: vec![
                lm_project::RomWrite {
                    offset: 1,
                    bytes: vec![9]
                },
                lm_project::RomWrite {
                    offset: 0x8000,
                    bytes: vec![8]
                },
            ],
        }),
        Err(AppError::Transaction(_))
    ));
    assert_eq!(app.project.as_ref().unwrap().rom.read(1, 1).unwrap(), [0]);
    assert!(!app.project.as_ref().unwrap().history.can_undo());
}

#[test]
fn stale_async_controller_result_cannot_overwrite_a_newer_revision() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    let decoded_revision = app.project_revision();
    app.dispatch(Command::CommitRomWrites {
        expected_revision: decoded_revision,
        description: "newer edit".into(),
        writes: vec![lm_project::RomWrite {
            offset: 1,
            bytes: vec![7],
        }],
    })
    .unwrap();
    assert_eq!(app.project_revision(), 1);
    assert!(matches!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: decoded_revision,
            description: "stale edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 1,
                bytes: vec![9],
            }],
        }),
        Err(AppError::StaleProjectRevision {
            expected: 0,
            actual: 1
        })
    ));
    assert_eq!(app.project.as_ref().unwrap().rom.read(1, 1).unwrap(), [7]);
    assert_eq!(app.project_revision(), 1);
}

#[test]
fn controller_snapshot_is_immutable_and_revision_bound() {
    let mut app = AppState::default();
    app.load_rom_at(test_rom(), Some("project.smc".into()))
        .unwrap();
    app.dispatch(Command::ShowOverworld).unwrap();
    let snapshot = app.controller_snapshot().unwrap();
    assert_eq!(snapshot.revision, 0);
    assert_eq!(snapshot.mode, EditorMode::Overworld);
    assert_eq!(snapshot.document_path, Some("project.smc".into()));
    assert_eq!(snapshot.rom_bytes[1], 0);
    app.dispatch(Command::CommitRomWrites {
        expected_revision: snapshot.revision,
        description: "edit".into(),
        writes: vec![lm_project::RomWrite {
            offset: 1,
            bytes: vec![7],
        }],
    })
    .unwrap();
    assert_eq!(snapshot.rom_bytes[1], 0);
    let newer = app.controller_snapshot().unwrap();
    assert_eq!(newer.revision, 1);
    assert_eq!(newer.rom_bytes[1], 7);
}

#[test]
fn revision_overflow_is_rejected_before_mutation_or_history_change() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    app.project_revision = u64::MAX;
    assert!(matches!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: u64::MAX,
            description: "edit".into(),
            writes: vec![lm_project::RomWrite {
                offset: 1,
                bytes: vec![9],
            }],
        }),
        Err(AppError::ProjectRevisionOverflow)
    ));
    assert_eq!(app.project.as_ref().unwrap().rom.read(1, 1).unwrap(), [0]);
    assert!(!app.project.as_ref().unwrap().history.can_undo());
}

#[test]
fn byte_identical_controller_commit_does_not_advance_revision() {
    let mut app = AppState::default();
    app.load_rom(test_rom()).unwrap();
    assert!(
        app.dispatch(Command::CommitRomWrites {
            expected_revision: 0,
            description: "no change".into(),
            writes: vec![lm_project::RomWrite {
                offset: 1,
                bytes: vec![0],
            }],
        })
        .unwrap()
        .is_empty()
    );
    assert_eq!(app.project_revision(), 0);
    assert!(!app.project().unwrap().history.can_undo());
}
