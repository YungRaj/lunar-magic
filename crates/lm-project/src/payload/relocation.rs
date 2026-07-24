//! One-transaction validation and staging for explicitly owned payload relocation.

use super::{PayloadSaveError, PayloadSaveRequest, PayloadSaveResult};
use crate::{Project, RatsOwnershipManifest};
use lm_rats::RatsBlock;
use std::ops::Range;

pub(super) struct PreparedReclamation {
    blocks: Vec<RatsBlock>,
    fill: u8,
}

pub(super) fn prepare_relocation(
    project: &Project,
    requests: &[PayloadSaveRequest],
    manifest: &RatsOwnershipManifest,
    pointer_ranges: &[Range<usize>],
    extra_ranges: &[Range<usize>],
    checksum_field: Option<usize>,
) -> Result<PreparedReclamation, PayloadSaveError> {
    let Some(first) = requests.first() else {
        return Err(PayloadSaveError::ReclamationRequiresPreviousBlock);
    };
    if requests
        .iter()
        .any(|request| request.erase_fill != first.erase_fill)
    {
        return Err(PayloadSaveError::ReclamationMixedEraseFills);
    }
    let mut expected = requests
        .iter()
        .filter_map(|request| request.previous_block.clone())
        .collect::<Vec<_>>();
    expected.sort_by_key(|block| block.header_offset);
    expected.dedup();
    if expected.is_empty() {
        return Err(PayloadSaveError::ReclamationRequiresPreviousBlock);
    }
    let plan = project.plan_rats_reclamation(manifest, first.erase_fill)?;
    if plan.reclaimed != expected {
        return Err(PayloadSaveError::ReclamationPreviousBlocksMismatch {
            expected: expected.len(),
            reclaimable: plan.reclaimed.len(),
        });
    }
    for block in &expected {
        let full = block.full_range();
        if let Some(pointer) = pointer_ranges.iter().find(|range| overlaps(&full, range)) {
            return Err(PayloadSaveError::ReclamationOverlapsPointer {
                block_offset: block.header_offset,
                pointer_offset: pointer.start,
            });
        }
        if let Some(write) = extra_ranges.iter().find(|range| overlaps(&full, range)) {
            return Err(PayloadSaveError::ReclamationOverlapsExtraWrite {
                block_offset: block.header_offset,
                write_offset: write.start,
            });
        }
        if let Some(field) = checksum_field {
            let checksum = field..field + 4;
            if overlaps(&full, &checksum) {
                return Err(PayloadSaveError::ReclamationOverlapsChecksum {
                    block_offset: block.header_offset,
                    checksum_field: field,
                });
            }
        }
    }
    Ok(PreparedReclamation {
        blocks: expected,
        fill: first.erase_fill,
    })
}

pub(super) fn stage_reclamation(
    staged: &mut [u8],
    reclamation: &PreparedReclamation,
    results: &[PayloadSaveResult],
) {
    for block in &reclamation.blocks {
        if results.iter().any(|result| result.block == *block) {
            continue;
        }
        staged[block.full_range()].fill(reclamation.fill);
    }
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
