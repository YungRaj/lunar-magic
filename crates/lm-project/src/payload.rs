use crate::{Project, RatsOwnershipManifest, RatsReclamationError, RomWrite, TransactionError};
use lm_rats::{AllocationError, AllocationPolicy, RatsBlock};
use lm_rom::{Mapper, RomError, compute_snes_checksum};
use std::fmt;

mod relocation;
pub(crate) mod staging;

use relocation::{prepare_relocation, stage_reclamation};
use staging::{commit_staged, expanded_staging_image, stage_request};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadSaveRequest {
    pub description: String,
    pub payload: Vec<u8>,
    pub pointer: PayloadPointer,
    pub mapper: Mapper,
    pub allocation_policy: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub maximum_payload_len: usize,
    /// Preferred fill byte for a later explicit [`crate::RatsOwnershipManifest`] reclamation pass.
    /// Ordinary high-level saves are copy-on-write because `previous_block` may have other owners.
    pub erase_fill: u8,
}

/// Locations used to encode a payload's 24-bit SNES address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadPointer {
    Contiguous {
        offset: usize,
    },
    /// Contiguous pointer encoded through the low-bank `LoROM` mirror (`bank & $7F`).
    ContiguousLowBank {
        offset: usize,
    },
    Split {
        low_word_offset: usize,
        bank_offset: usize,
        shared_bank: bool,
    },
}

impl PayloadPointer {
    #[must_use]
    pub const fn contiguous(offset: usize) -> Self {
        Self::Contiguous { offset }
    }

    #[must_use]
    pub const fn contiguous_low_bank(offset: usize) -> Self {
        Self::ContiguousLowBank { offset }
    }
}

impl From<usize> for PayloadPointer {
    fn from(offset: usize) -> Self {
        Self::contiguous(offset)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadSaveResult {
    pub block: RatsBlock,
    pub snes_pointer: u32,
    pub reused_existing: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PayloadReclamation<'a> {
    pub checksum_field: usize,
    pub manifest: &'a RatsOwnershipManifest,
}

#[derive(Debug)]
pub enum PayloadSaveError {
    PayloadLimit {
        actual: usize,
        maximum: usize,
    },
    Allocation(AllocationError),
    Rom(RomError),
    Transaction(TransactionError),
    MixedExpansionMappers,
    OverlappingPointers {
        first_offset: usize,
        second_offset: usize,
    },
    PointerRangeOverflow {
        offset: usize,
    },
    SharedPointerBankMismatch {
        bank_offset: usize,
        existing: u8,
        required: u8,
    },
    MapperCannotAddressImage {
        mapper: Mapper,
        image_len: usize,
    },
    InvalidExtraWrite {
        offset: usize,
        len: usize,
    },
    OverlappingExtraWrites {
        first_offset: usize,
        second_offset: usize,
    },
    ExtraWriteOverlapsPointer {
        write_offset: usize,
        pointer_offset: usize,
    },
    ExtraWriteUnprotected {
        offset: usize,
        len: usize,
    },
    ExtraWriteOverlapsChecksum {
        offset: usize,
        checksum_field: usize,
    },
    ChecksumUnprotected {
        checksum_field: usize,
    },
    Reclamation(RatsReclamationError),
    ReclamationRequiresPreviousBlock,
    ReclamationPreviousBlocksMismatch {
        expected: usize,
        reclaimable: usize,
    },
    ReclamationMixedEraseFills,
    ReclamationOverlapsPointer {
        block_offset: usize,
        pointer_offset: usize,
    },
    ReclamationOverlapsExtraWrite {
        block_offset: usize,
        write_offset: usize,
    },
    ReclamationOverlapsChecksum {
        block_offset: usize,
        checksum_field: usize,
    },
}

impl fmt::Display for PayloadSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "payload save failed: {self:?}")
    }
}

impl std::error::Error for PayloadSaveError {}

impl From<AllocationError> for PayloadSaveError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for PayloadSaveError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for PayloadSaveError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl From<RatsReclamationError> for PayloadSaveError {
    fn from(value: RatsReclamationError) -> Self {
        Self::Reclamation(value)
    }
}

impl Project {
    /// Allocates or replaces a tagged payload and updates its three-byte SNES pointer atomically.
    ///
    /// Work is performed on a staging image. Allocation, mapper conversion, and pointer validation
    /// must all succeed before the project ROM receives one undoable edit batch.
    ///
    /// # Errors
    ///
    /// Returns [`PayloadSaveError`] for limits, allocation failure, invalid mapping, pointer bounds,
    /// or transaction failure. Failure leaves the project ROM and history unchanged.
    pub fn save_tagged_payload(
        &mut self,
        request: &PayloadSaveRequest,
    ) -> Result<PayloadSaveResult, PayloadSaveError> {
        let mut results =
            self.save_tagged_payloads(&request.description, std::slice::from_ref(request))?;
        Ok(results.remove(0))
    }

    /// Stages several tagged allocations and pointer updates as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`PayloadSaveError`] if any request fails. The project ROM and history remain
    /// unchanged unless every request succeeds.
    pub fn save_tagged_payloads(
        &mut self,
        description: impl Into<String>,
        requests: &[PayloadSaveRequest],
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        self.save_tagged_payload_group(description.into(), requests, &[], None, None)
    }

    /// Stages allocations, pointer updates, and the final SNES checksum as one undoable commit.
    ///
    /// # Errors
    ///
    /// Returns [`PayloadSaveError`] for any request, allocation, mapping, checksum, or transaction
    /// failure. No ROM or history state changes unless the complete group succeeds.
    pub fn save_tagged_payloads_with_checksum(
        &mut self,
        description: impl Into<String>,
        requests: &[PayloadSaveRequest],
        checksum_field: usize,
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        self.save_tagged_payload_group(
            description.into(),
            requests,
            &[],
            Some(checksum_field),
            None,
        )
    }

    /// Atomically saves payloads, reclaims exactly proven displaced blocks, and repairs checksum.
    ///
    /// The manifest must validate against the pre-edit ROM and make exactly the unique
    /// `previous_block` descriptors reclaimable. Blocks reused by any resulting pointer are
    /// retained. Allocation, pointer writes, reclamation, and checksum repair produce one history
    /// batch or no mutation.
    ///
    /// # Errors
    ///
    /// Rejects stale or non-exact ownership, mixed erase fills, protected-field overlap, or any
    /// ordinary payload-save failure without changing ROM bytes or history.
    pub fn save_tagged_payloads_with_checksum_and_reclamation(
        &mut self,
        description: impl Into<String>,
        requests: &[PayloadSaveRequest],
        checksum_field: usize,
        manifest: &RatsOwnershipManifest,
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        self.save_tagged_payload_group(
            description.into(),
            requests,
            &[],
            Some(checksum_field),
            Some(manifest),
        )
    }

    /// Stages tagged payloads, protected direct writes, and checksum repair as one commit.
    ///
    /// # Errors
    ///
    /// In addition to payload failures, rejects empty/out-of-bounds/overlapping direct writes,
    /// writes intersecting pointers or checksum fields, and writes not protected by every
    /// allocation policy.
    pub fn save_tagged_payloads_with_checksum_and_writes(
        &mut self,
        description: impl Into<String>,
        requests: &[PayloadSaveRequest],
        writes: &[RomWrite],
        checksum_field: usize,
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        self.save_tagged_payload_group(
            description.into(),
            requests,
            writes,
            Some(checksum_field),
            None,
        )
    }

    /// Stages tagged payloads, protected direct writes, exact owned-block reclamation, and checksum
    /// repair as one commit.
    ///
    /// # Errors
    ///
    /// Rejects non-exact ownership, unsafe direct-write intersections, invalid allocation or
    /// mapping, and checksum failure without changing ROM bytes or history.
    pub fn save_tagged_payloads_with_checksum_writes_and_reclamation(
        &mut self,
        description: impl Into<String>,
        requests: &[PayloadSaveRequest],
        writes: &[RomWrite],
        checksum_field: usize,
        manifest: &RatsOwnershipManifest,
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        self.save_tagged_payload_group(
            description.into(),
            requests,
            writes,
            Some(checksum_field),
            Some(manifest),
        )
    }

    fn save_tagged_payload_group(
        &mut self,
        description: String,
        requests: &[PayloadSaveRequest],
        extra_writes: &[RomWrite],
        checksum_field: Option<usize>,
        reclamation_manifest: Option<&RatsOwnershipManifest>,
    ) -> Result<Vec<PayloadSaveResult>, PayloadSaveError> {
        let pointer_groups = checked_pointer_ranges(requests)?;
        let pointer_ranges = pointer_groups.iter().flatten().cloned().collect::<Vec<_>>();
        validate_checksum_protection(requests, checksum_field)?;
        let extra_ranges =
            checked_extra_ranges(requests, extra_writes, &pointer_ranges, checksum_field)?;
        let reclamation = reclamation_manifest
            .map(|manifest| {
                prepare_relocation(
                    self,
                    requests,
                    manifest,
                    &pointer_ranges,
                    &extra_ranges,
                    checksum_field,
                )
            })
            .transpose()?;
        for request in requests {
            validate_request(self, request)?;
        }
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = expanded_staging_image(&original, requests)?;
        let mut results = Vec::with_capacity(requests.len());
        for (request, pointer_group) in requests.iter().zip(&pointer_groups) {
            results.push(stage_request(
                &mut staged,
                request,
                pointer_group,
                &pointer_ranges,
            )?);
        }
        for (write, range) in extra_writes.iter().zip(extra_ranges) {
            let image_len = staged.len();
            let target = staged
                .get_mut(range)
                .ok_or(PayloadSaveError::InvalidExtraWrite {
                    offset: write.offset,
                    len: write.bytes.len(),
                })?;
            debug_assert_eq!(target.len(), write.bytes.len());
            target.copy_from_slice(&write.bytes);
            debug_assert_eq!(staged.len(), image_len);
        }
        if let Some(reclamation) = &reclamation {
            stage_reclamation(&mut staged, reclamation, &results);
        }
        if let Some(field) = checksum_field {
            let checksum = compute_snes_checksum(&staged, field)?;
            let end =
                field
                    .checked_add(checksum.encoded().len())
                    .ok_or(RomError::RangeOutOfBounds {
                        offset: field,
                        len: checksum.encoded().len(),
                        image_len: staged.len(),
                    })?;
            let image_len = staged.len();
            let target = staged
                .get_mut(field..end)
                .ok_or(RomError::RangeOutOfBounds {
                    offset: field,
                    len: checksum.encoded().len(),
                    image_len,
                })?;
            target.copy_from_slice(&checksum.encoded());
        }
        commit_staged(self, description, &original, &staged)?;
        Ok(results)
    }
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
mod validation;

use validation::{
    checked_extra_ranges, checked_pointer_ranges, validate_checksum_protection, validate_request,
};
