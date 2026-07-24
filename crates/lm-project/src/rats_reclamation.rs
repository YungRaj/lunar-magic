use crate::{Project, RomWrite, TransactionError};
use lm_rats::RatsBlock;
use lm_rom::RomError;
use std::fmt;

mod checksum;
mod validation;

use validation::validate_manifest;

/// Caller-supplied proof boundary for safe RATS garbage collection.
///
/// `owned` must contain every allocation the calling subsystem is authorized to erase.
/// `retained` is the exact subset of those allocations that still has at least one live
/// reference. The project never infers ownership from pointer reachability or tag shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatsOwnershipManifest {
    pub owned: Vec<RatsBlock>,
    pub retained: Vec<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatsReclamationPlan {
    pub reclaimed: Vec<RatsBlock>,
    pub reclaimed_bytes: usize,
    pub writes: Vec<RomWrite>,
}

impl RatsReclamationPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.reclaimed.is_empty()
    }
}

#[derive(Debug)]
pub enum RatsReclamationError {
    StaleOwnedBlock { index: usize },
    StaleRetainedBlock { index: usize },
    DuplicateOwnedBlock { first: usize, second: usize },
    DuplicateRetainedBlock { first: usize, second: usize },
    OverlappingOwnedBlocks { first: usize, second: usize },
    RetainedBlockNotOwned { index: usize },
    ReclaimedByteCountOverflow,
    ChecksumFieldOverlap { block: usize },
    InternalHeaderOverlap { block: usize },
    Rom(RomError),
    Transaction(TransactionError),
}

impl fmt::Display for RatsReclamationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "RATS reclamation failed: {self:?}")
    }
}

impl std::error::Error for RatsReclamationError {}

impl From<TransactionError> for RatsReclamationError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<RomError> for RatsReclamationError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl Project {
    /// Validates an explicit ownership manifest and prepares non-overlapping erase writes.
    ///
    /// This is a dry run. A block is reclaimed only when it is in `owned`, absent from
    /// `retained`, and its descriptor exactly matches the current ROM. Merely finding a valid
    /// `STAR` tag is not ownership evidence.
    ///
    /// # Errors
    ///
    /// Returns [`RatsReclamationError`] for stale, duplicate, overlapping, or inconsistent proof
    /// entries. The project is never modified.
    pub fn plan_rats_reclamation(
        &self,
        manifest: &RatsOwnershipManifest,
        fill: u8,
    ) -> Result<RatsReclamationPlan, RatsReclamationError> {
        validate_manifest(self.rom.logical_bytes(), manifest)?;

        let mut reclaimed: Vec<_> = manifest
            .owned
            .iter()
            .filter(|block| !manifest.retained.contains(block))
            .cloned()
            .collect();
        reclaimed.sort_by_key(|block| block.header_offset);
        let reclaimed_bytes = reclaimed.iter().try_fold(0_usize, |total, block| {
            total.checked_add(block.full_range().len())
        });
        let reclaimed_bytes =
            reclaimed_bytes.ok_or(RatsReclamationError::ReclaimedByteCountOverflow)?;
        let writes = reclaimed
            .iter()
            .map(|block| RomWrite {
                offset: block.header_offset,
                bytes: vec![fill; block.full_range().len()],
            })
            .collect();
        Ok(RatsReclamationPlan {
            reclaimed,
            reclaimed_bytes,
            writes,
        })
    }

    /// Reclaims exclusively owned, non-retained RATS blocks as one undoable operation.
    ///
    /// # Errors
    ///
    /// Returns [`RatsReclamationError`] if proof validation or the atomic transaction fails.
    pub fn reclaim_owned_rats(
        &mut self,
        description: impl Into<String>,
        manifest: &RatsOwnershipManifest,
        fill: u8,
    ) -> Result<RatsReclamationPlan, RatsReclamationError> {
        let plan = self.plan_rats_reclamation(manifest, fill)?;
        self.apply_writes(description, &plan.writes)?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator, parse_at};
    use lm_rom::{RomImage, SnesChecksum, compute_snes_checksum};

    fn project_with_blocks() -> (Project, RatsBlock, RatsBlock) {
        let mut bytes = vec![0xff; 0x8000];
        let mut allocator =
            FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x100..0x8000));
        let first = allocator.allocate(&[1, 2, 3]).unwrap();
        let second = allocator.allocate(&[4, 5]).unwrap();
        (
            Project::new(RomImage::from_bytes(bytes).unwrap()),
            first,
            second,
        )
    }

    #[test]
    fn dry_run_reclaims_only_owned_non_retained_blocks() {
        let (project, first, second) = project_with_blocks();
        let manifest = RatsOwnershipManifest {
            owned: vec![second.clone(), first.clone()],
            retained: vec![second],
        };
        let plan = project.plan_rats_reclamation(&manifest, 0xff).unwrap();
        assert_eq!(plan.reclaimed, vec![first.clone()]);
        assert_eq!(plan.reclaimed_bytes, first.full_range().len());
        assert_eq!(plan.writes[0].offset, first.header_offset);
        assert_eq!(project.rom.logical_bytes()[first.header_offset], b'S');
    }

    #[test]
    fn reclamation_is_one_undoable_operation() {
        let (mut project, first, second) = project_with_blocks();
        let manifest = RatsOwnershipManifest {
            owned: vec![first.clone(), second.clone()],
            retained: vec![second.clone()],
        };
        project
            .reclaim_owned_rats("collect old payload", &manifest, 0xff)
            .unwrap();
        assert!(
            project.rom.logical_bytes()[first.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
        assert_eq!(
            parse_at(project.rom.logical_bytes(), second.header_offset).unwrap(),
            second
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(
            parse_at(project.rom.logical_bytes(), first.header_offset).unwrap(),
            first
        );
    }

    #[test]
    fn reclamation_and_checksum_are_one_exactly_reversible_operation() {
        let (mut project, first, second) = project_with_blocks();
        let before = project.rom.logical_bytes().to_vec();
        let manifest = RatsOwnershipManifest {
            owned: vec![first.clone(), second.clone()],
            retained: vec![second],
        };
        let (_, checksum) = project
            .reclaim_owned_rats_with_checksum("collect and checksum", &manifest, 0xff, 0x7fdc)
            .unwrap();
        assert_eq!(project.history.undo_len(), 1);
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
            checksum
        );
        assert_eq!(
            compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap(),
            checksum
        );
        let after = project.rom.logical_bytes().to_vec();
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), before);
        assert!(project.history.redo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), after);
    }

    #[test]
    fn checksum_overlap_rejects_reclamation_without_mutation() {
        let (mut project, first, _) = project_with_blocks();
        let before = project.rom.logical_bytes().to_vec();
        let checksum_field = first.header_offset;
        let error = project
            .reclaim_owned_rats_with_checksum(
                "must fail",
                &RatsOwnershipManifest {
                    owned: vec![first],
                    retained: Vec::new(),
                },
                0xff,
                checksum_field,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RatsReclamationError::ChecksumFieldOverlap { block: 0 }
        ));
        assert_eq!(project.rom.logical_bytes(), before);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn internal_header_overlap_outside_checksum_field_is_rejected() {
        let mut bytes = vec![0xff; 0x8000];
        let block = FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x7fc0..0x8000))
            .allocate(&[1])
            .unwrap();
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let error = project
            .reclaim_owned_rats_with_checksum(
                "must fail",
                &RatsOwnershipManifest {
                    owned: vec![block],
                    retained: Vec::new(),
                },
                0xff,
                0x7fdc,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RatsReclamationError::InternalHeaderOverlap { block: 0 }
        ));
        assert_eq!(project.rom.logical_bytes(), bytes);
        assert_eq!(project.history.undo_len(), 0);
    }

    #[test]
    fn rejects_stale_and_inconsistent_manifests_without_mutation() {
        let (project, first, second) = project_with_blocks();
        let before = project.rom.logical_bytes().to_vec();
        let stale = RatsBlock {
            header_offset: first.header_offset,
            payload: first.payload.start..first.payload.end + 1,
        };
        assert!(matches!(
            project.plan_rats_reclamation(
                &RatsOwnershipManifest {
                    owned: vec![stale],
                    retained: Vec::new(),
                },
                0xff,
            ),
            Err(RatsReclamationError::StaleOwnedBlock { index: 0 })
        ));
        assert!(matches!(
            project.plan_rats_reclamation(
                &RatsOwnershipManifest {
                    owned: vec![first],
                    retained: vec![second],
                },
                0xff,
            ),
            Err(RatsReclamationError::RetainedBlockNotOwned { index: 0 })
        ));
        assert_eq!(project.rom.logical_bytes(), before);
    }

    #[test]
    fn duplicate_proof_entries_are_rejected() {
        let (project, first, _) = project_with_blocks();
        let error = project
            .plan_rats_reclamation(
                &RatsOwnershipManifest {
                    owned: vec![first.clone(), first],
                    retained: Vec::new(),
                },
                0,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RatsReclamationError::DuplicateOwnedBlock {
                first: 0,
                second: 1
            }
        ));
    }

    #[test]
    fn overlapping_valid_owned_descriptors_are_rejected() {
        let nested_payload = [lm_rats::make_header(1).unwrap().as_slice(), &[0x42]].concat();
        let mut bytes = vec![0xff; 0x8000];
        let outer = {
            let mut allocator =
                FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x100..0x8000));
            allocator.allocate(&nested_payload).unwrap()
        };
        let inner = parse_at(&bytes, outer.payload.start).unwrap();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        assert!(matches!(
            project.plan_rats_reclamation(
                &RatsOwnershipManifest {
                    owned: vec![outer, inner],
                    retained: Vec::new(),
                },
                0xff,
            ),
            Err(RatsReclamationError::OverlappingOwnedBlocks {
                first: 0,
                second: 1
            })
        ));
    }

    #[test]
    fn empty_manifest_is_a_no_op() {
        let (mut project, _, _) = project_with_blocks();
        let plan = project
            .reclaim_owned_rats(
                "nothing",
                &RatsOwnershipManifest {
                    owned: Vec::new(),
                    retained: Vec::new(),
                },
                0,
            )
            .unwrap();
        assert!(plan.is_empty());
        assert_eq!(project.history.undo_len(), 0);
    }
}
