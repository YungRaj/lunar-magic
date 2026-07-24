use super::{RatsOwnershipManifest, RatsReclamationError};
use lm_rats::{RatsBlock, parse_at};
use std::ops::Range;

pub(super) fn validate_manifest(
    bytes: &[u8],
    manifest: &RatsOwnershipManifest,
) -> Result<(), RatsReclamationError> {
    validate_blocks(bytes, &manifest.owned, true)?;
    validate_blocks(bytes, &manifest.retained, false)?;
    for (index, block) in manifest.retained.iter().enumerate() {
        if !manifest.owned.contains(block) {
            return Err(RatsReclamationError::RetainedBlockNotOwned { index });
        }
    }
    Ok(())
}

fn validate_blocks(
    bytes: &[u8],
    blocks: &[RatsBlock],
    owned: bool,
) -> Result<(), RatsReclamationError> {
    for (index, block) in blocks.iter().enumerate() {
        if parse_at(bytes, block.header_offset).as_ref() != Ok(block) {
            return Err(if owned {
                RatsReclamationError::StaleOwnedBlock { index }
            } else {
                RatsReclamationError::StaleRetainedBlock { index }
            });
        }
        for (first, prior) in blocks[..index].iter().enumerate() {
            if prior == block {
                return Err(if owned {
                    RatsReclamationError::DuplicateOwnedBlock {
                        first,
                        second: index,
                    }
                } else {
                    RatsReclamationError::DuplicateRetainedBlock {
                        first,
                        second: index,
                    }
                });
            }
            if owned && overlaps(&prior.full_range(), &block.full_range()) {
                return Err(RatsReclamationError::OverlappingOwnedBlocks {
                    first,
                    second: index,
                });
            }
        }
    }
    Ok(())
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
