use super::*;
use crate::parse_at;

fn reference_start(bytes: &[u8], policy: &AllocationPolicy, required: usize) -> Option<usize> {
    if required > policy.search.len() {
        return None;
    }
    (policy.search.start..=policy.search.end - required).find(|start| {
        let candidate = *start..*start + required;
        let within_bank = policy
            .bank_size
            .is_none_or(|bank| candidate.start / bank == (candidate.end - 1) / bank);
        within_bank
            && !policy.protects(&candidate)
            && bytes[candidate]
                .iter()
                .all(|byte| policy.fill_bytes.contains(byte))
    })
}

#[test]
fn allocates_and_erases_block() {
    let mut bytes = vec![0xff; 0x10000];
    let mut allocator = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0..0x10000));
    let block = allocator.allocate(&[1, 2, 3]).unwrap();
    assert_eq!(block.header_offset, 0);
    allocator.erase(&block, 0xff).unwrap();
    assert!(bytes[..11].iter().all(|byte| *byte == 0xff));
}

#[test]
fn respects_bank_boundaries_and_protected_ranges() {
    let mut bytes = vec![0xff; 0x10000];
    let policy = AllocationPolicy {
        search: 0x7ff8..0x10000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![ProtectedRange(0x8000..0x8010)],
    };
    let mut allocator = FreeSpaceAllocator::new(&mut bytes, policy);
    let block = allocator.allocate(&[7; 16]).unwrap();
    assert_eq!(block.header_offset, 0x8010);
    assert!(parse_at(&bytes, block.header_offset).is_ok());
}

#[test]
fn identical_payload_is_reused() {
    let mut bytes = vec![0xff; 0x10000];
    let mut allocator = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0..0x10000));
    let first = allocator.allocate(&[1, 2, 3]).unwrap();
    let second = allocator.allocate_or_reuse(&[1, 2, 3], 0x8000).unwrap();
    assert!(second.reused_existing);
    assert_eq!(second.block, first);
    assert_eq!(scan(&bytes).len(), 1);
}

#[test]
fn duplicate_crossing_lorom_bank_is_not_reused() {
    let mut bytes = vec![0xff; 0x18000];
    let crossing_offset = 0x7ff8;
    bytes[crossing_offset..0x8000].copy_from_slice(&make_header(3).unwrap());
    bytes[0x8000..0x8003].copy_from_slice(&[1, 2, 3]);
    let mut allocator =
        FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x8003..0x18000));
    let outcome = allocator.allocate_or_reuse(&[1, 2, 3], 0x8000).unwrap();
    assert!(!outcome.reused_existing);
    assert_ne!(outcome.block.header_offset, crossing_offset);
    assert!(allocator.policy.fits_bank(&outcome.block.full_range()));
}

#[test]
fn standalone_duplicate_lookup_enforces_complete_policy() {
    let payload = [1, 2, 3];
    let mut bytes = vec![0xff; 0x10000];
    bytes[0x100..0x108].copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[0x108..0x10b].copy_from_slice(&payload);
    let protected = AllocationPolicy {
        protected: vec![ProtectedRange(0x100..0x10b)],
        ..AllocationPolicy::lorom(0..0x10000)
    };
    assert_eq!(
        find_duplicate(&bytes, &payload, 16, &protected).unwrap(),
        None
    );

    let invalid = AllocationPolicy {
        bank_size: Some(0),
        ..AllocationPolicy::lorom(0..0x10000)
    };
    assert_eq!(
        find_duplicate(&bytes, &payload, 16, &invalid),
        Err(AllocationError::InvalidPolicy)
    );
}

#[test]
fn duplicate_outside_authorized_search_is_not_reused() {
    let payload = [1, 2, 3];
    let mut bytes = vec![0xff; 0x1000];
    bytes[0x100..0x108].copy_from_slice(&make_header(payload.len()).unwrap());
    bytes[0x108..0x10b].copy_from_slice(&payload);
    let policy = AllocationPolicy::lorom(0x200..0x1000);
    assert_eq!(find_duplicate(&bytes, &payload, 16, &policy).unwrap(), None);

    let mut allocator = FreeSpaceAllocator::new(&mut bytes, policy);
    let outcome = allocator.allocate_or_reuse(&payload, 16).unwrap();
    assert!(!outcome.reused_existing);
    assert!(outcome.block.header_offset >= 0x200);
}

#[test]
fn crossing_existing_block_cannot_be_replaced_or_erased_under_banked_policy() {
    let mut bytes = vec![0xff; 0x10000];
    let offset = 0x7ff8;
    bytes[offset..0x8000].copy_from_slice(&make_header(3).unwrap());
    bytes[0x8000..0x8003].copy_from_slice(&[1, 2, 3]);
    let block = parse_at(&bytes, offset).unwrap();
    let snapshot = bytes.clone();
    let mut allocator = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0..0x10000));
    assert_eq!(
        allocator.replace(&block, &[4, 5, 6], 0xff),
        Err(AllocationError::InvalidBlock)
    );
    assert_eq!(
        allocator.erase(&block, 0xff),
        Err(AllocationError::InvalidBlock)
    );
    assert_eq!(allocator.bytes, snapshot);
}

#[test]
fn replacement_failure_preserves_old_block() {
    let mut bytes = vec![0xff; 32];
    let policy = AllocationPolicy {
        search: 0..32,
        bank_size: None,
        fill_bytes: vec![0xff],
        protected: Vec::new(),
    };
    let mut allocator = FreeSpaceAllocator::new(&mut bytes, policy);
    let old = allocator.allocate(&[7; 16]).unwrap();
    let snapshot = allocator.bytes.to_vec();
    assert!(matches!(
        allocator.replace(&old, &[8; 17], 0xff),
        Err(AllocationError::NoSpace { .. })
    ));
    assert_eq!(allocator.bytes, snapshot);
}

#[test]
fn smaller_replacement_reuses_header_and_releases_tail() {
    let mut bytes = vec![0xff; 64];
    let mut allocator = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
    );
    let old = allocator.allocate(&[3; 12]).unwrap();
    let replacement = allocator.replace(&old, &[4; 3], 0xff).unwrap();
    assert_eq!(replacement.header_offset, old.header_offset);
    assert_eq!(&allocator.bytes[replacement.payload], &[4; 3]);
    assert!(allocator.bytes[11..20].iter().all(|byte| *byte == 0xff));
}

#[test]
fn forged_or_stale_blocks_cannot_replace_or_erase_arbitrary_bytes() {
    let mut bytes = vec![0xff; 64];
    bytes[20..24].copy_from_slice(&[1, 2, 3, 4]);
    let snapshot = bytes.clone();
    let forged = RatsBlock {
        header_offset: 12,
        payload: 20..24,
    };
    let mut allocator = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
    );
    assert_eq!(
        allocator.erase(&forged, 0),
        Err(AllocationError::InvalidBlock)
    );
    assert_eq!(
        allocator.replace(&forged, &[9], 0),
        Err(AllocationError::InvalidBlock)
    );
    assert_eq!(allocator.bytes, snapshot);

    let valid = allocator.allocate(&[7, 8]).unwrap();
    allocator.bytes[valid.header_offset] ^= 1;
    let corrupted = allocator.bytes.to_vec();
    assert_eq!(
        allocator.erase(&valid, 0),
        Err(AllocationError::InvalidBlock)
    );
    assert_eq!(allocator.bytes, corrupted);
}

#[test]
fn protected_existing_blocks_cannot_be_reused_replaced_or_erased() {
    let mut bytes = vec![0xff; 64];
    let protected_block = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
    )
    .allocate(&[1, 2, 3])
    .unwrap();
    let protected_range = protected_block.full_range();
    let snapshot = bytes.clone();
    let mut allocator = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(protected_range.clone())],
        },
    );
    assert_eq!(
        allocator.erase(&protected_block, 0),
        Err(AllocationError::ProtectedBlock)
    );
    assert_eq!(
        allocator.replace(&protected_block, &[9], 0),
        Err(AllocationError::ProtectedBlock)
    );
    assert_eq!(allocator.bytes, snapshot);

    let duplicate = allocator.allocate_or_reuse(&[1, 2, 3], 16).unwrap();
    assert!(!duplicate.reused_existing);
    assert!(!overlaps(&duplicate.block.full_range(), &protected_range));
    assert_eq!(&allocator.bytes[protected_block.payload], &[1, 2, 3]);
}

#[test]
fn malformed_search_bank_and_protected_policies_are_rejected() {
    for policy in [
        AllocationPolicy {
            search: 0..65,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
        AllocationPolicy {
            search: 0..64,
            bank_size: Some(0),
            fill_bytes: vec![0xff],
            protected: vec![],
        },
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(4..65)],
        },
    ] {
        let mut bytes = vec![0xff; 64];
        let snapshot = bytes.clone();
        assert_eq!(
            FreeSpaceAllocator::new(&mut bytes, policy).allocate(&[1]),
            Err(AllocationError::InvalidPolicy)
        );
        assert_eq!(bytes, snapshot);
    }
}

#[test]
fn malformed_policies_cannot_replace_or_erase_existing_blocks() {
    let mut source = vec![0xff; 64];
    source[8..16].copy_from_slice(&make_header(3).unwrap());
    source[16..19].copy_from_slice(&[1, 2, 3]);
    let block = parse_at(&source, 8).unwrap();
    for policy in [
        AllocationPolicy {
            search: 0..65,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
        AllocationPolicy {
            search: 0..64,
            bank_size: Some(0),
            fill_bytes: vec![0xff],
            protected: vec![],
        },
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: Vec::new(),
            protected: vec![],
        },
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(4..65)],
        },
    ] {
        let mut replacement_bytes = source.clone();
        let replacement_snapshot = replacement_bytes.clone();
        assert_eq!(
            FreeSpaceAllocator::new(&mut replacement_bytes, policy.clone()).replace(
                &block,
                &[4, 5, 6],
                0xff,
            ),
            Err(AllocationError::InvalidPolicy)
        );
        assert_eq!(replacement_bytes, replacement_snapshot);

        let mut erase_bytes = source.clone();
        let erase_snapshot = erase_bytes.clone();
        assert_eq!(
            FreeSpaceAllocator::new(&mut erase_bytes, policy).erase(&block, 0xff),
            Err(AllocationError::InvalidPolicy)
        );
        assert_eq!(erase_bytes, erase_snapshot);
    }
}

#[test]
fn undersized_zero_based_search_cannot_allocate_beyond_its_end() {
    let mut bytes = vec![0xff; 32];
    let snapshot = bytes.clone();
    let mut allocator = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..8,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: Vec::new(),
        },
    );
    assert_eq!(
        allocator.allocate(&[1]),
        Err(AllocationError::NoSpace { required: 9 })
    );
    assert_eq!(bytes, snapshot);
}

#[test]
fn small_allocation_choices_match_a_brute_force_reference() {
    const IMAGE_LEN: usize = 18;
    const REQUIRED: usize = HEADER_LEN + 1;
    for start in 0..IMAGE_LEN {
        for end in start + 1..=IMAGE_LEN {
            for bank_size in [None, Some(8), Some(16)] {
                for protected in [Vec::new(), vec![ProtectedRange(5..7)]] {
                    for occupied_byte in 0..=IMAGE_LEN {
                        let mut bytes = vec![0xff; IMAGE_LEN];
                        if occupied_byte < IMAGE_LEN {
                            bytes[occupied_byte] = 0x55;
                        }
                        let policy = AllocationPolicy {
                            search: start..end,
                            bank_size,
                            fill_bytes: vec![0xff],
                            protected: protected.clone(),
                        };
                        let expected = reference_start(&bytes, &policy, REQUIRED);
                        let snapshot = bytes.clone();
                        let actual = FreeSpaceAllocator::new(&mut bytes, policy).allocate(&[1]);
                        if let Some(offset) = expected {
                            assert_eq!(actual.unwrap().header_offset, offset);
                        } else {
                            assert_eq!(
                                actual,
                                Err(AllocationError::NoSpace { required: REQUIRED })
                            );
                            assert_eq!(bytes, snapshot);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn duplicate_reuse_cannot_bypass_policy_validation() {
    let mut bytes = vec![0xff; 64];
    let block = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..64,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
    )
    .allocate(&[1, 2, 3])
    .unwrap();
    let snapshot = bytes.clone();
    let mut allocator = FreeSpaceAllocator::new(
        &mut bytes,
        AllocationPolicy {
            search: 0..65,
            bank_size: None,
            fill_bytes: vec![0xff],
            protected: vec![],
        },
    );
    assert_eq!(
        allocator.allocate_or_reuse(&[1, 2, 3], 16),
        Err(AllocationError::InvalidPolicy)
    );
    assert_eq!(allocator.bytes, snapshot);
    assert_eq!(&allocator.bytes[block.payload], &[1, 2, 3]);
}
