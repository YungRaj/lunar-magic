use super::*;
use lm_rats::parse_at;

fn plan() -> RelocatablePatchPlan {
    RelocatablePatchPlan {
        description: "install test runtime".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy {
            search: 0x8000..0x1_0000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x7fc0..0x8000)],
        },
        checksum_field: 0x7fdc,
        expansion_fill: 0xff,
        payloads: vec![
            PatchPayload {
                bytes: vec![0xaa; 8],
                fixups: vec![PatchFixup {
                    offset: 2,
                    target_payload: 1,
                    target_addend: 1,
                    encoding: PatchFixupEncoding::Long24,
                }],
            },
            PatchPayload {
                bytes: vec![0xbb; 5],
                fixups: Vec::new(),
            },
        ],
        writes: vec![PatchWrite {
            offset: 0x100,
            expected: vec![0xff; 4],
            replacement: vec![0x22, 0, 0, 0],
            fixups: vec![PatchFixup {
                offset: 1,
                target_payload: 0,
                target_addend: 0,
                encoding: PatchFixupEncoding::Long24,
            }],
        }],
    }
}

#[test]
fn installs_cross_referenced_payloads_hook_checksum_and_one_undo_batch() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let result = project.install_relocatable_patch(&plan()).unwrap();

    assert_eq!(project.rom.logical_len(), 0x1_0000);
    assert_eq!(result.blocks.len(), 2);
    assert_eq!(project.history.undo_len(), 1);
    for block in &result.blocks {
        assert_eq!(
            parse_at(project.rom.logical_bytes(), block.header_offset).unwrap(),
            *block
        );
    }
    let runtime = project.rom.read(result.blocks[0].payload.start, 8).unwrap();
    assert_eq!(&runtime[..2], &[0xaa; 2]);
    let table_plus_one = pc_to_snes(Mapper::LoRom, result.blocks[1].payload.start + 1).unwrap();
    assert_eq!(&runtime[2..5], &table_plus_one.to_le_bytes()[..3]);
    assert_eq!(project.rom.read(0x100, 1).unwrap(), &[0x22]);
    assert_eq!(
        project.rom.read(0x101, 3).unwrap(),
        &result.snes_addresses[0].to_le_bytes()[..3]
    );
    let stored = u16::from_le_bytes(project.rom.read(0x7fde, 2).unwrap().try_into().unwrap());
    assert_eq!(
        stored,
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc)
            .unwrap()
            .checksum
    );
    project.undo().unwrap();
    assert_eq!(project.rom.logical_len(), 0x8000);
    assert_eq!(project.rom.read(0x100, 4).unwrap(), &[0xff; 4]);
}

#[test]
fn late_fixup_and_hook_failures_are_atomic() {
    for mutate in 0..2 {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let mut invalid = plan();
        if mutate == 0 {
            invalid.payloads[0].fixups[0].target_addend = 99;
        } else {
            invalid.writes[0].expected[0] = 0;
        }
        let before = project.rom.logical_bytes().to_vec();
        assert!(project.install_relocatable_patch(&invalid).is_err());
        assert_eq!(project.rom.logical_bytes(), before);
        assert_eq!(project.history.undo_len(), 0);
    }
}

#[test]
fn replacement_reclaims_owned_block_reuses_space_and_undoes_as_one_batch() {
    let mut bytes = vec![0xff; 0x1_0000];
    let previous = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x8000..0x1_0000))
        .allocate(&[0x44; 0x40])
        .unwrap();
    let original = bytes.clone();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());

    let result = project
        .replace_relocatable_patch(
            &plan(),
            &RatsOwnershipManifest {
                owned: vec![previous.clone()],
                retained: Vec::new(),
            },
            0xff,
        )
        .unwrap();

    assert_eq!(result.blocks[0].header_offset, previous.header_offset);
    assert_eq!(project.history.undo_len(), 1);
    project.undo().unwrap();
    assert_eq!(project.rom.logical_bytes(), original);
}

#[test]
fn replacement_late_plan_failure_does_not_escape_staged_reclamation() {
    let mut bytes = vec![0xff; 0x1_0000];
    let previous = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x8000..0x1_0000))
        .allocate(&[0x44; 0x40])
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let before = project.rom.logical_bytes().to_vec();
    let mut invalid = plan();
    invalid.writes[0].expected[0] = 0;

    assert!(matches!(
        project.replace_relocatable_patch(
            &invalid,
            &RatsOwnershipManifest {
                owned: vec![previous],
                retained: Vec::new(),
            },
            0xff,
        ),
        Err(RelocatablePatchReplacementError::Patch(
            RelocatablePatchError::HookPreconditionMismatch { .. }
        ))
    ));
    assert_eq!(project.rom.logical_bytes(), before);
    assert_eq!(project.history.undo_len(), 0);
}

#[test]
fn grouped_plans_keep_independent_policies_and_commit_once() {
    let first = plan();
    let mut second = plan();
    second.description = "install second runtime".into();
    second.allocation.search = 0x1_0000..0x1_8000;
    second.writes[0].offset = 0x200;
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let results = project
        .install_relocatable_patch_group("install both runtimes", &[first, second])
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].blocks[0].header_offset, 0x8000);
    assert_eq!(results[1].blocks[0].header_offset, 0x1_0000);
    assert_eq!(project.rom.logical_len(), 0x1_8000);
    assert_eq!(project.history.undo_len(), 1);
    project.undo().unwrap();
    assert_eq!(project.rom.logical_len(), 0x8000);
}

#[test]
fn late_group_plan_failure_rolls_back_prior_expansion_allocation_and_hook() {
    let first = plan();
    let mut second = plan();
    second.allocation.search = 0x1_0000..0x1_8000;
    second.writes[0].offset = 0x200;
    second.writes[0].expected[0] = 0;
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let original = project.rom.logical_bytes().to_vec();

    let error = project
        .install_relocatable_patch_group("must fail", &[first, second])
        .unwrap_err();
    assert_eq!(error.plan, 1);
    assert!(matches!(
        error.source,
        RelocatablePatchError::HookPreconditionMismatch {
            index: 0,
            offset: 0x200
        }
    ));
    assert_eq!(project.rom.logical_bytes(), original);
    assert_eq!(project.history.undo_len(), 0);
}

#[test]
fn split_low_word_and_bank_fixups_publish_one_allocated_address() {
    let mut split = plan();
    split.payloads[0].fixups = vec![
        PatchFixup {
            offset: 0,
            target_payload: 1,
            target_addend: 1,
            encoding: PatchFixupEncoding::Low16,
        },
        PatchFixup {
            offset: 2,
            target_payload: 1,
            target_addend: 1,
            encoding: PatchFixupEncoding::Bank8,
        },
    ];
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let result = project.install_relocatable_patch(&split).unwrap();
    let target = pc_to_snes(Mapper::LoRom, result.blocks[1].payload.start + 1)
        .unwrap()
        .to_le_bytes();
    assert_eq!(
        project.rom.read(result.blocks[0].payload.start, 3).unwrap(),
        &target[..3]
    );
}

#[test]
fn malformed_fixups_writes_and_expansion_policy_are_rejected() {
    let mut cases = Vec::new();
    let mut missing = plan();
    missing.payloads[0].fixups[0].target_payload = 2;
    cases.push(missing);
    let mut overlap = plan();
    overlap.payloads[0].fixups.push(PatchFixup {
        offset: 3,
        target_payload: 0,
        target_addend: 0,
        encoding: PatchFixupEncoding::Long24,
    });
    cases.push(overlap);
    let mut writes = plan();
    writes.writes.push(PatchWrite {
        offset: 0x102,
        expected: vec![0xff],
        replacement: vec![0],
        fixups: Vec::new(),
    });
    cases.push(writes);
    let mut fill = plan();
    fill.expansion_fill = 0;
    cases.push(fill);
    for invalid in cases {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        assert!(project.install_relocatable_patch(&invalid).is_err());
        assert_eq!(project.rom.logical_len(), 0x8000);
        assert_eq!(project.history.undo_len(), 0);
    }
}
