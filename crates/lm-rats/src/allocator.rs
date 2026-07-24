use crate::{HEADER_LEN, RatsBlock, make_header, parse_at, scan};
use std::fmt;
use std::ops::Range;

mod policy;

pub use policy::{AllocationPolicy, ProtectedRange};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllocationError {
    EmptyPayload,
    PayloadTooLarge,
    InvalidPolicy,
    InvalidBlock,
    ProtectedBlock,
    NoSpace { required: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationOutcome {
    pub block: RatsBlock,
    pub reused_existing: bool,
}

impl fmt::Display for AllocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPayload => write!(f, "RATS payload cannot be empty"),
            Self::PayloadTooLarge => write!(f, "RATS payload exceeds the 65536-byte header limit"),
            Self::InvalidPolicy => write!(f, "free-space allocation policy is invalid"),
            Self::InvalidBlock => write!(f, "RATS block is stale, malformed, or forged"),
            Self::ProtectedBlock => write!(f, "RATS block intersects a protected range"),
            Self::NoSpace { required } => write!(f, "no free range can hold {required:#x} bytes"),
        }
    }
}

impl std::error::Error for AllocationError {}

pub struct FreeSpaceAllocator<'a> {
    bytes: &'a mut [u8],
    policy: AllocationPolicy,
}

impl<'a> FreeSpaceAllocator<'a> {
    #[must_use]
    pub fn new(bytes: &'a mut [u8], policy: AllocationPolicy) -> Self {
        Self { bytes, policy }
    }

    /// Finds space and writes a complete RATS allocation.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError`] for invalid sizes/policies or when no suitable range exists.
    pub fn allocate(&mut self, payload: &[u8]) -> Result<RatsBlock, AllocationError> {
        let header = make_header(payload.len()).ok_or(if payload.is_empty() {
            AllocationError::EmptyPayload
        } else {
            AllocationError::PayloadTooLarge
        })?;
        let required = HEADER_LEN + payload.len();
        let offset = self.find_free_range(required)?;
        self.bytes[offset..offset + HEADER_LEN].copy_from_slice(&header);
        self.bytes[offset + HEADER_LEN..offset + required].copy_from_slice(payload);
        Ok(RatsBlock {
            header_offset: offset,
            payload: offset + HEADER_LEN..offset + required,
        })
    }

    /// Reuses a byte-identical validated payload or allocates a new block.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::allocate`] when no duplicate exists.
    pub fn allocate_or_reuse(
        &mut self,
        payload: &[u8],
        maximum_payload_len: usize,
    ) -> Result<AllocationOutcome, AllocationError> {
        let duplicate = find_duplicate(self.bytes, payload, maximum_payload_len, &self.policy)?;
        if let Some(block) = duplicate {
            return Ok(AllocationOutcome {
                block,
                reused_existing: true,
            });
        }
        Ok(AllocationOutcome {
            block: self.allocate(payload)?,
            reused_existing: false,
        })
    }

    /// Replaces a validated allocation. Growth allocates first, so failure preserves the old
    /// allocation. Equal-size or smaller payloads are rewritten in place.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError`] for invalid blocks, payloads, policies, or insufficient space.
    pub fn replace(
        &mut self,
        old: &RatsBlock,
        payload: &[u8],
        fill: u8,
    ) -> Result<RatsBlock, AllocationError> {
        self.validate_policy()?;
        let old_range = old.full_range();
        self.validate_block(old)?;
        let header = make_header(payload.len()).ok_or(if payload.is_empty() {
            AllocationError::EmptyPayload
        } else {
            AllocationError::PayloadTooLarge
        })?;
        if payload.len() <= old.payload.len() {
            self.bytes[old.header_offset..old.header_offset + HEADER_LEN].copy_from_slice(&header);
            let payload_end = old.payload.start + payload.len();
            self.bytes[old.payload.start..payload_end].copy_from_slice(payload);
            self.bytes[payload_end..old.payload.end].fill(fill);
            return Ok(RatsBlock {
                header_offset: old.header_offset,
                payload: old.payload.start..payload_end,
            });
        }

        let replacement = self.allocate(payload)?;
        self.bytes[old_range].fill(fill);
        Ok(replacement)
    }

    /// Erases a validated block using `fill`.
    ///
    /// # Errors
    ///
    /// Returns [`AllocationError::InvalidBlock`] unless the descriptor exactly matches a valid
    /// block currently present in the image.
    pub fn erase(&mut self, block: &RatsBlock, fill: u8) -> Result<(), AllocationError> {
        self.validate_policy()?;
        self.validate_block(block)?;
        let range = block.full_range();
        let target = self
            .bytes
            .get_mut(range)
            .ok_or(AllocationError::InvalidBlock)?;
        target.fill(fill);
        Ok(())
    }

    fn find_free_range(&self, required: usize) -> Result<usize, AllocationError> {
        self.validate_policy()?;
        let end = self.policy.search.end;
        let occupied: Vec<_> = scan(self.bytes)
            .into_iter()
            .map(|block| block.full_range())
            .collect();
        let Some(last_start) = end
            .checked_sub(required)
            .filter(|last| *last >= self.policy.search.start)
        else {
            return Err(AllocationError::NoSpace { required });
        };
        for start in self.policy.search.start..=last_start {
            let candidate = start..start + required;
            if !self.policy.permits_allocation(&candidate) || overlaps_any(&candidate, &occupied) {
                continue;
            }
            if self.bytes[candidate.clone()]
                .iter()
                .all(|byte| self.policy.fill_bytes.contains(byte))
            {
                return Ok(start);
            }
        }
        Err(AllocationError::NoSpace { required })
    }

    fn validate_policy(&self) -> Result<(), AllocationError> {
        self.policy.validate(self.bytes.len())
    }

    fn validate_block(&self, block: &RatsBlock) -> Result<(), AllocationError> {
        match parse_at(self.bytes, block.header_offset) {
            Ok(actual) if &actual == block && self.block_is_protected(block) => {
                Err(AllocationError::ProtectedBlock)
            }
            Ok(actual) if &actual == block && self.policy.fits_bank(&block.full_range()) => Ok(()),
            _ => Err(AllocationError::InvalidBlock),
        }
    }

    fn block_is_protected(&self, block: &RatsBlock) -> bool {
        self.policy.protects(&block.full_range())
    }
}

/// Finds a policy-compatible, byte-identical payload no larger than the subsystem limit.
///
/// # Errors
///
/// Returns [`AllocationError::InvalidPolicy`] before scanning if `policy` is malformed for the
/// supplied image. Protected and cross-bank blocks are never returned.
pub fn find_duplicate(
    bytes: &[u8],
    payload: &[u8],
    maximum_payload_len: usize,
    policy: &AllocationPolicy,
) -> Result<Option<RatsBlock>, AllocationError> {
    policy.validate(bytes.len())?;
    if payload.is_empty() || payload.len() > maximum_payload_len {
        return Ok(None);
    }
    Ok(scan(bytes).into_iter().find(|block| {
        let range = block.full_range();
        policy.permits_allocation(&range)
            && block.payload.len() == payload.len()
            && bytes.get(block.payload.clone()) == Some(payload)
    }))
}

fn overlaps_any(candidate: &Range<usize>, ranges: &[Range<usize>]) -> bool {
    ranges.iter().any(|range| overlaps(candidate, range))
}

fn overlaps(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(test)]
#[path = "allocator_tests.rs"]
mod tests;
