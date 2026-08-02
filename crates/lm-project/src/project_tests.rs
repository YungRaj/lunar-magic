use super::*;
use lm_rom::{Mapper, RomImage, compute_snes_checksum};

fn supported_project() -> Project {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap()
}

#[test]
fn cached_checksum_identity_tracks_commit_undo_and_redo() {
    let mut project = supported_project();
    assert!(project.identity.as_ref().unwrap().checksum_matches());
    let mut staged = project.rom.logical_bytes().to_vec();
    staged[0x100] = 7;
    let checksum = compute_snes_checksum(&staged, 0x7fdc).unwrap();
    staged[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    project
        .apply_writes(
            "checksum-valid edit",
            &[
                RomWrite {
                    offset: 0x100,
                    bytes: vec![7],
                },
                RomWrite {
                    offset: 0x7fdc,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )
        .unwrap();
    assert_eq!(project.identity.as_ref().unwrap().stored_checksum, checksum);
    assert!(project.identity.as_ref().unwrap().checksum_matches());
    assert!(project.undo().unwrap());
    assert!(project.identity.as_ref().unwrap().checksum_matches());
    assert!(project.redo().unwrap());
    assert_eq!(project.identity.as_ref().unwrap().stored_checksum, checksum);
    assert!(project.identity.as_ref().unwrap().checksum_matches());
}

#[test]
fn unqualified_project_has_no_checksum_identity_to_synchronize() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    assert!(!project.synchronize_identity_checksums());
}

#[test]
fn bounded_multi_write_is_atomic_and_undoable() {
    let rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
    let mut project = Project::new(rom);
    project
        .apply_writes(
            "level save",
            &[
                RomWrite {
                    offset: 2,
                    bytes: vec![1, 2],
                },
                RomWrite {
                    offset: 20,
                    bytes: vec![3],
                },
            ],
        )
        .unwrap();
    assert_eq!(project.rom.read(2, 2).unwrap(), &[1, 2]);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.rom.read(2, 2).unwrap(), &[0, 0]);
    assert_eq!(project.rom.read(20, 1).unwrap(), &[0]);
}

#[test]
fn invalid_group_never_mutates_the_rom() {
    let rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
    let mut project = Project::new(rom);
    let result = project.apply_writes(
        "bad save",
        &[
            RomWrite {
                offset: 1,
                bytes: vec![9],
            },
            RomWrite {
                offset: 0x8000,
                bytes: vec![8],
            },
        ],
    );
    assert!(result.is_err());
    assert_eq!(project.rom.read(1, 1).unwrap(), &[0]);
    assert!(!project.history.can_undo());
}

#[test]
fn complete_logical_replacement_grows_shrinks_and_preserves_header_through_history() {
    let mut image = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
    image.set_copier_header(lm_rom::CopierHeader::Present, 0xa5);
    let mut project = Project::new(image);
    let before = project.save_snapshot();
    let mut expanded = vec![0; 0x1_0000];
    expanded[0x123] = 7;
    assert!(
        project
            .apply_logical_replacement("IPS growth", Mapper::LoRom, &expanded)
            .unwrap()
    );
    assert_eq!(
        &project.rom.as_file_bytes()[..lm_rom::COPIER_HEADER_LEN],
        &[0xa5; lm_rom::COPIER_HEADER_LEN]
    );
    let grown = project.save_snapshot();
    assert!(project.undo().unwrap());
    assert_eq!(project.save_snapshot(), before);
    assert!(project.redo().unwrap());
    assert_eq!(project.save_snapshot(), grown);
    assert!(
        project
            .apply_logical_replacement("IPS shrink", Mapper::LoRom, &vec![3; 0x8000])
            .unwrap()
    );
    assert_eq!(project.rom.logical_len(), 0x8000);
    assert!(project.undo().unwrap());
    assert_eq!(project.save_snapshot(), grown);
}

#[test]
fn invalid_or_identical_logical_replacement_is_nonmutating() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    assert!(
        !project
            .apply_logical_replacement("same", Mapper::LoRom, &vec![0; 0x8000])
            .unwrap()
    );
    assert!(
        project
            .apply_logical_replacement("partial bank", Mapper::LoRom, &vec![0; 0x8001])
            .is_err()
    );
    assert_eq!(project.rom.logical_len(), 0x8000);
    assert_eq!(project.history.undo_len(), 0);
}

#[test]
fn copier_header_conversion_is_dirty_reversible_and_retains_removed_bytes() {
    let logical = supported_project().rom.logical_bytes().to_vec();
    let mut image = RomImage::from_bytes(logical.clone()).unwrap();
    image.set_copier_header(lm_rom::CopierHeader::Present, 0);
    let mut varied = vec![0; lm_rom::COPIER_HEADER_LEN];
    for (index, byte) in varied.iter_mut().enumerate() {
        *byte = index.to_le_bytes()[0];
    }
    image
        .replace_copier_header_exact(Some(&[0; lm_rom::COPIER_HEADER_LEN]), Some(&varied))
        .unwrap();
    image.accept_changes();
    let mut project = Project::open_supported(image).unwrap();
    assert!(!project.is_modified());
    assert!(
        project
            .set_copier_header("remove header", lm_rom::CopierHeader::Absent, 0)
            .unwrap()
    );
    assert!(project.is_modified());
    assert_eq!(project.rom.logical_bytes(), logical);
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.copier_header_bytes().unwrap(), varied);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.copier_header(), lm_rom::CopierHeader::Absent);
    assert!(project.undo().unwrap());
    assert!(!project.is_modified());

    assert!(project.redo().unwrap());
    assert!(
        project
            .set_copier_header("add header", lm_rom::CopierHeader::Present, 0x5a)
            .unwrap()
    );
    assert_eq!(
        project.rom.copier_header_bytes().unwrap(),
        &[0x5a; lm_rom::COPIER_HEADER_LEN]
    );
}

#[test]
fn exact_copier_header_install_adds_replaces_rejects_bad_shapes_and_undoes() {
    let logical = supported_project().rom.logical_bytes().to_vec();
    let mut project = supported_project();
    let mut canonical = vec![0; lm_rom::COPIER_HEADER_LEN];
    canonical[..4].copy_from_slice(&[0x40, 0xaa, 0xbb, 4]);
    assert!(
        project
            .set_copier_header_exact("canonical header", &canonical)
            .unwrap()
    );
    assert_eq!(
        project.rom.copier_header_bytes(),
        Some(canonical.as_slice())
    );
    assert_eq!(project.rom.logical_bytes(), logical);
    assert!(
        !project
            .set_copier_header_exact("no op", &canonical)
            .unwrap()
    );
    assert_eq!(project.history.undo_len(), 1);

    let replacement = vec![0x7e; lm_rom::COPIER_HEADER_LEN];
    assert!(
        project
            .set_copier_header_exact("replace header", &replacement)
            .unwrap()
    );
    assert_eq!(
        project.rom.copier_header_bytes(),
        Some(replacement.as_slice())
    );
    assert!(project.undo().unwrap());
    assert_eq!(
        project.rom.copier_header_bytes(),
        Some(canonical.as_slice())
    );
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.copier_header(), lm_rom::CopierHeader::Absent);
    assert_eq!(project.rom.logical_bytes(), logical);

    assert!(
        project
            .set_copier_header_exact("bad", &[0; lm_rom::COPIER_HEADER_LEN - 1])
            .is_err()
    );
    assert_eq!(project.history.undo_len(), 0);
    assert_eq!(project.rom.logical_bytes(), logical);
}

#[test]
fn checksum_update_participates_in_history() {
    let mut bytes = vec![0x11; 0x8000];
    bytes[0x7fdc..0x7fe0].fill(0);
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let checksum = project.refresh_checksum(0x7fdc).unwrap();
    assert!(checksum.is_complementary());
    assert_eq!(project.rom.read(0x7fdc, 4).unwrap(), checksum.encoded());
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.rom.read(0x7fdc, 4).unwrap(), &[0; 4]);
}

#[test]
fn entirely_identical_batch_is_not_an_undoable_operation() {
    let mut bytes = vec![0; 0x8000];
    bytes[10] = 7;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    assert!(
        !project
            .apply_writes(
                "no change",
                &[RomWrite {
                    offset: 10,
                    bytes: vec![7],
                }],
            )
            .unwrap()
    );
    assert!(!project.history.can_undo());
    assert!(!project.is_modified());
}

#[test]
fn overlapping_batch_is_rejected_before_any_mutation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    assert!(matches!(
        project.apply_writes(
            "ambiguous",
            &[
                RomWrite {
                    offset: 5,
                    bytes: vec![1, 2]
                },
                RomWrite {
                    offset: 6,
                    bytes: vec![3]
                },
            ],
        ),
        Err(TransactionError::OverlappingWrites {
            first: 0,
            second: 1
        })
    ));
    assert_eq!(project.rom.read(5, 2).unwrap(), [0, 0]);
    assert!(!project.history.can_undo());
}

#[test]
fn prepared_growth_and_writes_are_one_reversible_operation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    let mutation = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: 0x8000,
        appended: vec![0xff; 0x8000],
        writes: vec![
            RomWrite {
                offset: 4,
                bytes: vec![1, 2],
            },
            RomWrite {
                offset: 0x9000,
                bytes: vec![3, 4],
            },
        ],
    };
    assert!(project.apply_mutation("grow and edit", &mutation).unwrap());
    assert_eq!(project.rom.logical_len(), 0x10000);
    assert_eq!(project.rom.read(4, 2).unwrap(), [1, 2]);
    assert_eq!(project.rom.read(0x9000, 2).unwrap(), [3, 4]);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.rom.logical_len(), 0x8000);
    assert_eq!(project.rom.read(4, 2).unwrap(), [0, 0]);
    assert!(project.history.redo(&mut project.rom).unwrap());
    assert_eq!(project.rom.logical_len(), 0x10000);
    assert_eq!(project.rom.read(0x9000, 2).unwrap(), [3, 4]);
}

#[test]
fn stale_invalid_and_overlapping_mutations_do_not_append() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    let original = project.save_snapshot();
    let stale = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: 0x10000,
        appended: vec![0; 0x8000],
        writes: vec![],
    };
    assert!(matches!(
        project.apply_mutation("stale", &stale),
        Err(TransactionError::UnexpectedLogicalLength { .. })
    ));
    let invalid = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: 0x8000,
        appended: vec![0; 2],
        writes: vec![RomWrite {
            offset: 0x8001,
            bytes: vec![1, 2],
        }],
    };
    assert!(project.apply_mutation("invalid", &invalid).is_err());
    let overlap = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: 0x8000,
        appended: vec![0; 0x8000],
        writes: vec![
            RomWrite {
                offset: 0x7fff,
                bytes: vec![1, 2, 3],
            },
            RomWrite {
                offset: 0x8000,
                bytes: vec![4],
            },
        ],
    };
    assert!(matches!(
        project.apply_mutation("overlap", &overlap),
        Err(TransactionError::OverlappingWrites { .. })
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn mutation_between_coalesces_changed_runs_and_preserves_tail() {
    let before = [0, 1, 2, 3, 4, 5];
    let after = [0, 9, 8, 3, 7, 5, 0xaa, 0xbb];
    assert_eq!(
        RomMutation::between(Mapper::LoRom, &before, &after).unwrap(),
        RomMutation {
            mapper: Mapper::LoRom,
            expected_len: 6,
            appended: vec![0xaa, 0xbb],
            writes: vec![
                RomWrite {
                    offset: 1,
                    bytes: vec![9, 8],
                },
                RomWrite {
                    offset: 4,
                    bytes: vec![7],
                },
            ],
        }
    );
    assert!(matches!(
        RomMutation::between(Mapper::LoRom, &after, &before),
        Err(TransactionError::CannotPrepareShrink {
            before: 8,
            after: 6,
        })
    ));
}

#[test]
fn explicit_unchanged_mutation_is_a_valid_no_op() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    let mutation = RomMutation::unchanged(Mapper::LoRom, 0x8000);
    assert!(mutation.is_empty());
    assert!(!project.mutation_would_change(&mutation).unwrap());
    assert!(!project.apply_mutation("nothing", &mutation).unwrap());
    assert!(!project.history.can_undo());
    assert!(!project.is_modified());
}

#[test]
fn write_only_mutation_requires_mapper_to_address_whole_image() {
    let logical_len = 0x0040_0001;
    let mut project = Project::new(RomImage::from_bytes(vec![0; logical_len]).unwrap());
    let original = project.save_snapshot();
    let mutation = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: logical_len,
        appended: Vec::new(),
        writes: vec![RomWrite {
            offset: 0,
            bytes: vec![1],
        }],
    };
    assert!(matches!(
        project.apply_mutation("invalid mapper extent", &mutation),
        Err(TransactionError::MutationMapperCannotAddressImage {
            mapper: Mapper::LoRom,
            image_len,
        })
        if image_len == logical_len
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
    assert!(!project.is_modified());
}

#[test]
fn write_only_mutation_rejects_an_addressable_partial_bank() {
    let logical_len = 0x8001;
    let mut project = Project::new(RomImage::from_bytes(vec![0; logical_len]).unwrap());
    let original = project.save_snapshot();
    let mutation = RomMutation {
        mapper: Mapper::LoRom,
        expected_len: logical_len,
        appended: Vec::new(),
        writes: vec![RomWrite {
            offset: 0,
            bytes: vec![1],
        }],
    };
    assert!(matches!(
        project.apply_mutation("partial bank", &mutation),
        Err(TransactionError::MutationMapperCannotAddressImage {
            mapper: Mapper::LoRom,
            image_len: 0x8001,
        })
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}
