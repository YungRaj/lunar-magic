use super::*;
use lm_rats::{ProtectedRange, make_header, parse_at};
use lm_rom::{RomImage, snes_to_pc};

fn request(payload: Vec<u8>) -> PayloadSaveRequest {
    PayloadSaveRequest {
        description: "save layer 1".into(),
        payload,
        pointer: 0x20.into(),
        mapper: Mapper::LoRom,
        allocation_policy: AllocationPolicy {
            search: 0x100..0x10000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x23)],
        },
        previous_block: None,
        reuse_identical: true,
        maximum_payload_len: 0x8000,
        erase_fill: 0xff,
    }
}

#[test]
fn allocation_and_pointer_commit_as_one_undo_step() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let original = project.save_snapshot();
    let result = project
        .save_tagged_payload(&request(vec![1, 2, 3]))
        .unwrap();
    assert_eq!(
        snes_to_pc(Mapper::LoRom, result.snes_pointer).unwrap(),
        result.block.payload.start
    );
    assert_eq!(
        project.rom.read(result.block.payload.start, 3).unwrap(),
        &[1, 2, 3]
    );
    let loaded = project.load_tagged_payload(0x20, Mapper::LoRom).unwrap();
    assert_eq!(loaded.bytes, [1, 2, 3]);
    assert_eq!(loaded.block, Some(result.block));
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), original);
}

#[test]
fn split_pointer_components_commit_as_one_undo_step() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let original = project.save_snapshot();
    let mut save = request(vec![1, 2, 3]);
    save.pointer = PayloadPointer::Split {
        low_word_offset: 0x20,
        bank_offset: 0x40,
        shared_bank: false,
    };
    save.allocation_policy.protected = vec![ProtectedRange(0x20..0x22), ProtectedRange(0x40..0x41)];
    let result = project.save_tagged_payload(&save).unwrap();
    let encoded = result.snes_pointer.to_le_bytes();
    assert_eq!(project.rom.read(0x20, 2).unwrap(), &encoded[..2]);
    assert_eq!(project.rom.read(0x40, 1).unwrap(), &encoded[2..3]);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), original);
}

#[test]
fn displaced_word_and_bank_encodes_lunar_magic_operands_atomically() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10_0000]).unwrap());
    let original = project.save_snapshot();
    let mut save = request(vec![1, 2, 3]);
    save.pointer = PayloadPointer::DisplacedWordAndBank {
        low_word_offset: 0x20,
        bank_offset: 0x40,
        displacement: 0x1000,
        low_bank: true,
    };
    save.allocation_policy.search = 0x80000..0x88000;
    save.allocation_policy.protected = vec![ProtectedRange(0x20..0x22), ProtectedRange(0x40..0x41)];

    let result = project.save_tagged_payload(&save).unwrap();
    let pointer = result.snes_pointer.to_le_bytes();
    let expected_low = u16::from_le_bytes([pointer[0], pointer[1]])
        .wrapping_sub(0x1000)
        .to_le_bytes();
    assert_eq!(project.rom.read(0x20, 2).unwrap(), expected_low);
    assert_eq!(project.rom.read(0x40, 1).unwrap(), &[pointer[2] & 0x7f]);
    assert!(project.history.can_undo());

    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), original);
}

#[test]
fn shared_bank_mismatch_is_atomic() {
    let mut bytes = vec![0xff; 0x10000];
    bytes[0x40] = 0x81;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let original = project.save_snapshot();
    let mut save = request(vec![1, 2, 3]);
    save.pointer = PayloadPointer::Split {
        low_word_offset: 0x20,
        bank_offset: 0x40,
        shared_bank: true,
    };
    save.allocation_policy.protected = vec![ProtectedRange(0x20..0x22), ProtectedRange(0x40..0x41)];
    assert!(matches!(
        project.save_tagged_payload(&save),
        Err(PayloadSaveError::SharedPointerBankMismatch { .. })
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn shared_bank_accepts_an_equivalent_low_lorom_mirror() {
    let mut bytes = vec![0xff; 0x10000];
    bytes[0x40] = 0x00;
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let mut save = request(vec![1, 2, 3]);
    save.pointer = PayloadPointer::Split {
        low_word_offset: 0x20,
        bank_offset: 0x40,
        shared_bank: true,
    };
    save.allocation_policy.protected = vec![ProtectedRange(0x20..0x22), ProtectedRange(0x40..0x41)];

    let result = project.save_tagged_payload(&save).unwrap();

    let encoded = result.snes_pointer.to_le_bytes();
    assert_eq!(encoded[2], 0x80);
    assert_eq!(project.rom.read(0x40, 1).unwrap(), &[0x00]);
    assert_eq!(
        snes_to_pc(
            Mapper::LoRom,
            u32::from_le_bytes([encoded[0], encoded[1], 0, 0])
        )
        .unwrap(),
        result.block.payload.start
    );
}

#[test]
fn failed_allocation_does_not_touch_project() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    let original = project.save_snapshot();
    let mut request = request(vec![1, 2, 3]);
    request.allocation_policy.search = 0x100..0x8000;
    assert!(project.save_tagged_payload(&request).is_err());
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn native_payload_save_rejects_partial_mapper_banks_before_allocation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8001]).unwrap());
    let original = project.save_snapshot();
    let mut save = request(vec![1, 2, 3]);
    save.allocation_policy.search = 0x100..0x8001;
    assert!(matches!(
        project.save_tagged_payload(&save),
        Err(PayloadSaveError::MapperCannotAddressImage {
            mapper: Mapper::LoRom,
            image_len: 0x8001,
        })
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn transactional_deduplication_never_reuses_a_protected_block() {
    let payload = [1, 2, 3];
    let mut bytes = vec![0xff; 0x10000];
    bytes[0x100..0x108].copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[0x108..0x10b].copy_from_slice(&payload);
    let protected_snapshot = bytes[0x100..0x10b].to_vec();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let mut request = request(payload.to_vec());
    request
        .allocation_policy
        .protected
        .push(ProtectedRange(0x100..0x10b));

    let result = project.save_tagged_payload(&request).unwrap();
    assert!(!result.reused_existing);
    assert_ne!(result.block.header_offset, 0x100);
    assert_eq!(project.rom.read(0x100, 0x0b).unwrap(), protected_snapshot);
}

#[test]
fn transactional_deduplication_never_reuses_a_cross_bank_block() {
    let payload = [1, 2, 3];
    let crossing_offset = 0x7ff8;
    let mut bytes = vec![0xff; 0x10000];
    bytes[crossing_offset..0x8000].copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[0x8000..0x8003].copy_from_slice(&payload);
    let crossing_snapshot = bytes[crossing_offset..0x8003].to_vec();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());

    let result = project
        .save_tagged_payload(&request(payload.to_vec()))
        .unwrap();
    assert!(!result.reused_existing);
    assert_ne!(result.block.header_offset, crossing_offset);
    assert!(
        request(payload.to_vec())
            .allocation_policy
            .fits_bank(&result.block.full_range())
    );
    assert_eq!(
        project
            .rom
            .read(crossing_offset, crossing_snapshot.len())
            .unwrap(),
        crossing_snapshot
    );
}

#[test]
fn transactional_deduplication_never_bypasses_search_authority() {
    let payload = [1, 2, 3];
    let unauthorized_offset = 0x40;
    let mut bytes = vec![0xff; 0x10000];
    bytes[unauthorized_offset..unauthorized_offset + 8]
        .copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[unauthorized_offset + 8..unauthorized_offset + 11].copy_from_slice(&payload);
    let unauthorized_snapshot = bytes[unauthorized_offset..unauthorized_offset + 11].to_vec();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());

    let result = project
        .save_tagged_payload(&request(payload.to_vec()))
        .unwrap();
    assert!(!result.reused_existing);
    assert!(result.block.header_offset >= 0x100);
    assert_eq!(
        project
            .rom
            .read(unauthorized_offset, unauthorized_snapshot.len())
            .unwrap(),
        unauthorized_snapshot
    );
}

#[test]
fn transactional_duplicate_cannot_bypass_invalid_policy() {
    let payload = [1, 2, 3];
    let mut bytes = vec![0xff; 0x10000];
    bytes[0x100..0x108].copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[0x108..0x10b].copy_from_slice(&payload);
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let snapshot = project.save_snapshot();
    let mut request = request(payload.to_vec());
    request.allocation_policy.bank_size = Some(0);

    assert!(matches!(
        project.save_tagged_payload(&request),
        Err(PayloadSaveError::Allocation(AllocationError::InvalidPolicy))
    ));
    assert_eq!(project.save_snapshot(), snapshot);
    assert!(!project.history.can_undo());
}

#[test]
fn pointer_bytes_are_intrinsically_protected_from_allocation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let mut unsafe_policy = request(vec![7]);
    unsafe_policy.allocation_policy.search = 0x20..0x40;
    unsafe_policy.allocation_policy.bank_size = None;
    unsafe_policy.allocation_policy.protected.clear();

    let result = project.save_tagged_payload(&unsafe_policy).unwrap();
    assert!(result.block.full_range().start >= 0x23);
    assert_eq!(
        parse_at(project.rom.logical_bytes(), result.block.header_offset).unwrap(),
        result.block
    );
    assert_eq!(
        project
            .load_tagged_payload(0x20, Mapper::LoRom)
            .unwrap()
            .bytes,
        [7]
    );
}

#[test]
fn every_pointer_in_a_batch_is_protected_from_every_allocation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let mut first = request(vec![1]);
    first.allocation_policy.search = 0x30..0x60;
    first.allocation_policy.bank_size = None;
    first.allocation_policy.protected.clear();
    let mut second = request(vec![2]);
    second.pointer = 0x30.into();
    second.allocation_policy = first.allocation_policy.clone();

    let results = project
        .save_tagged_payloads("two unsafe callers", &[first, second])
        .unwrap();
    for result in &results {
        assert!(!result.block.full_range().contains(&0x30));
        assert_eq!(
            parse_at(project.rom.logical_bytes(), result.block.header_offset).unwrap(),
            result.block
        );
    }
    assert_eq!(
        project
            .load_tagged_payload(0x20, Mapper::LoRom)
            .unwrap()
            .bytes,
        [1]
    );
    assert_eq!(
        project
            .load_tagged_payload(0x30, Mapper::LoRom)
            .unwrap()
            .bytes,
        [2]
    );
}

#[test]
fn overlapping_batch_pointers_are_rejected_atomically() {
    for second_offset in [0x20, 0x21, 0x22] {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let first = request(vec![1]);
        let mut second = request(vec![2]);
        second.pointer = second_offset.into();
        assert!(matches!(
            project.save_tagged_payloads("ambiguous pointers", &[first, second]),
            Err(PayloadSaveError::OverlappingPointers {
                first_offset: 0x20,
                second_offset: actual,
            }) if actual == second_offset
        ));
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}

#[test]
fn pointer_ranges_are_exact_checked_and_shared_by_the_transaction() {
    let first = request(vec![1]);
    let mut adjacent = request(vec![2]);
    adjacent.pointer = 0x23.into();
    assert_eq!(
        checked_pointer_ranges(&[first.clone(), adjacent]).unwrap(),
        [
            std::iter::once(0x20..0x23).collect::<Vec<_>>(),
            std::iter::once(0x23..0x26).collect::<Vec<_>>()
        ]
    );

    let mut overflowing = first;
    overflowing.pointer = (usize::MAX - 2).into();
    assert!(matches!(
        checked_pointer_ranges(std::slice::from_ref(&overflowing)),
        Err(PayloadSaveError::PointerRangeOverflow { offset })
            if offset == usize::MAX - 2
    ));
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let original = project.save_snapshot();
    assert!(matches!(
        project.save_tagged_payload(&overflowing),
        Err(PayloadSaveError::PointerRangeOverflow { .. })
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn implicit_pointer_protection_failure_is_atomic() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let original = project.save_snapshot();
    let mut impossible = request(vec![1]);
    impossible.allocation_policy.search = 0x20..0x29;
    impossible.allocation_policy.bank_size = None;
    impossible.allocation_policy.protected.clear();

    assert!(matches!(
        project.save_tagged_payload(&impossible),
        Err(PayloadSaveError::Allocation(AllocationError::NoSpace {
            required: 9
        }))
    ));
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn multiple_payloads_commit_and_undo_together() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let original = project.save_snapshot();
    let first = request(vec![1, 2, 3]);
    let mut second = request(vec![4, 5, 6]);
    second.pointer = 0x30.into();
    let results = project
        .save_tagged_payloads("save complete level", &[first, second])
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_ne!(results[0].block, results[1].block);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), original);
    assert!(!project.history.can_undo());
}

#[test]
fn full_rom_expands_allocates_and_undoes_as_one_batch() {
    let mut bytes = vec![0; 0x8000];
    bytes[0x20..0x23].fill(0);
    let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    let mut request = request(vec![1, 2, 3]);
    request.allocation_policy.search = 0x8000..0x10000;
    let result = project.save_tagged_payload(&request).unwrap();
    assert_eq!(project.rom.logical_len(), 0x10000);
    assert!(result.block.header_offset >= 0x8000);
    assert_eq!(
        project.rom.read(result.block.payload.start, 3).unwrap(),
        [1, 2, 3]
    );
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.rom.logical_len(), 0x8000);
    assert_eq!(project.rom.logical_bytes(), bytes);
    assert!(project.history.redo(&mut project.rom).unwrap());
    assert_eq!(project.rom.logical_len(), 0x10000);
    assert_eq!(
        project.rom.read(result.block.payload.start, 3).unwrap(),
        [1, 2, 3]
    );
}

#[test]
fn invalid_expansion_target_preserves_rom_and_history() {
    let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
    let before = project.save_snapshot();
    let mut request = request(vec![1]);
    request.allocation_policy.search = 0x8000..0x10001;
    assert!(project.save_tagged_payload(&request).is_err());
    assert_eq!(project.save_snapshot(), before);
    assert!(!project.history.can_undo());
}

#[test]
fn changing_one_owner_of_a_deduplicated_block_is_copy_on_write() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let first = project
        .save_tagged_payload(&request(vec![1, 2, 3]))
        .unwrap();
    let mut second_request = request(vec![1, 2, 3]);
    second_request.pointer = 0x30.into();
    let second = project.save_tagged_payload(&second_request).unwrap();
    assert_eq!(first.block, second.block);
    assert!(second.reused_existing);

    let mut changed = request(vec![9, 8]);
    changed.previous_block = Some(first.block.clone());
    let replacement = project.save_tagged_payload(&changed).unwrap();
    assert_ne!(replacement.block, first.block);
    assert_eq!(
        project
            .load_tagged_payload(0x20, Mapper::LoRom)
            .unwrap()
            .bytes,
        [9, 8]
    );
    assert_eq!(
        project
            .load_tagged_payload(0x30, Mapper::LoRom)
            .unwrap()
            .bytes,
        [1, 2, 3]
    );
    assert_eq!(
        parse_at(project.rom.logical_bytes(), first.block.header_offset).unwrap(),
        first.block
    );
}

#[test]
fn stale_previous_descriptor_fails_before_allocating() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let before = project.save_snapshot();
    let mut changed = request(vec![9]);
    changed.previous_block = Some(RatsBlock {
        header_offset: 0x100,
        payload: 0x108..0x109,
    });
    assert!(matches!(
        project.save_tagged_payload(&changed),
        Err(PayloadSaveError::Allocation(AllocationError::InvalidBlock))
    ));
    assert_eq!(project.save_snapshot(), before);
    assert!(!project.history.can_undo());
}

#[test]
fn tagged_payload_direct_write_and_checksum_commit_as_one_undo_step() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let before = project.save_snapshot();
    let mut request = request(vec![1, 2, 3]);
    request.allocation_policy.search = 0x100..0x8000;
    request
        .allocation_policy
        .protected
        .push(ProtectedRange(0x80..0xa0));
    request
        .allocation_policy
        .protected
        .push(ProtectedRange(0x7fdc..0x7fe0));
    let write = RomWrite {
        offset: 0x80,
        bytes: vec![0x5a; 0x20],
    };
    project
        .save_tagged_payloads_with_checksum_and_writes(
            "save grouped native data",
            &[request],
            &[write],
            0x7fdc,
        )
        .unwrap();
    assert_eq!(project.rom.read(0x80, 0x20).unwrap(), [0x5a; 0x20]);
    assert_eq!(project.history.undo_len(), 1);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), before);
}

#[test]
fn unsafe_extra_writes_fail_before_rom_or_history_mutation() {
    for write in [
        RomWrite {
            offset: 0x20,
            bytes: vec![1],
        },
        RomWrite {
            offset: 0x80,
            bytes: vec![1],
        },
        RomWrite {
            offset: 0x7fdc,
            bytes: vec![1],
        },
        RomWrite {
            offset: usize::MAX,
            bytes: vec![1, 2],
        },
    ] {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let before = project.save_snapshot();
        assert!(
            project
                .save_tagged_payloads_with_checksum_and_writes(
                    "unsafe grouped save",
                    &[request(vec![1])],
                    &[write],
                    0x7fdc,
                )
                .is_err()
        );
        assert_eq!(project.save_snapshot(), before);
        assert_eq!(project.history.undo_len(), 0);
    }
}

#[test]
fn owned_relocation_save_erase_checksum_and_pointer_are_one_undo_batch() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let old = project
        .save_tagged_payload(&request(vec![1, 2, 3]))
        .unwrap()
        .block;
    let before = project.save_snapshot();
    let undo_before = project.history.undo_len();
    let mut changed = request(vec![9, 8, 7, 6]);
    changed.previous_block = Some(old.clone());
    changed
        .allocation_policy
        .protected
        .push(ProtectedRange(0x7fdc..0x7fe0));
    let result = project
        .save_tagged_payloads_with_checksum_and_reclamation(
            "replace owned payload",
            &[changed],
            0x7fdc,
            &RatsOwnershipManifest {
                owned: vec![old.clone()],
                retained: Vec::new(),
            },
        )
        .unwrap()
        .remove(0);
    assert_ne!(result.block, old);
    assert!(
        project.rom.logical_bytes()[old.full_range()]
            .iter()
            .all(|byte| *byte == 0xff)
    );
    assert_eq!(project.history.undo_len(), undo_before + 1);
    assert!(project.history.undo(&mut project.rom).unwrap());
    assert_eq!(project.save_snapshot(), before);
    assert!(project.history.redo(&mut project.rom).unwrap());
    assert!(
        project.rom.logical_bytes()[old.full_range()]
            .iter()
            .all(|byte| *byte == 0xff)
    );
}

#[test]
fn nonexact_owned_relocation_proof_fails_without_mutation() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let old = project
        .save_tagged_payload(&request(vec![1, 2, 3]))
        .unwrap()
        .block;
    let foreign = project
        .save_tagged_payload(&PayloadSaveRequest {
            pointer: 0x30.into(),
            payload: vec![4, 5],
            ..request(Vec::new())
        })
        .unwrap()
        .block;
    let before = project.save_snapshot();
    let undo_before = project.history.undo_len();
    let mut changed = request(vec![9]);
    changed.previous_block = Some(old);
    changed
        .allocation_policy
        .protected
        .push(ProtectedRange(0x7fdc..0x7fe0));
    assert!(matches!(
        project.save_tagged_payloads_with_checksum_and_reclamation(
            "reject foreign proof",
            &[changed],
            0x7fdc,
            &RatsOwnershipManifest {
                owned: vec![foreign],
                retained: Vec::new(),
            },
        ),
        Err(PayloadSaveError::ReclamationPreviousBlocksMismatch { .. })
    ));
    assert_eq!(project.save_snapshot(), before);
    assert_eq!(project.history.undo_len(), undo_before);
}
