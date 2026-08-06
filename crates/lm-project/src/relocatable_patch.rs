//! Failure-atomic installation of mutually relocatable tagged runtime payloads.

use crate::{
    EditKind, Project, RatsOwnershipManifest, RatsReclamationError,
    payload::staging::{commit_staged, commit_staged_with_kind},
};
use lm_rats::{AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes};
use std::fmt;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchFixupEncoding {
    /// Contiguous little-endian 24-bit SNES address.
    Long24,
    /// Contiguous 24-bit address using the equivalent low-bank `LoROM` mirror.
    Long24LowBank,
    /// Low 16 bits of the SNES address.
    Low16,
    /// Low byte of the SNES address, for split-plane pointer tables.
    Low8,
    /// High byte of the low word, for split-plane pointer tables.
    High8,
    /// Bank byte of the SNES address.
    Bank8,
    /// Bank byte using the equivalent low-bank `LoROM` mirror.
    Bank8LowBank,
}

impl PatchFixupEncoding {
    #[must_use]
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::Long24 | Self::Long24LowBank => 3,
            Self::Low16 => 2,
            Self::Low8 | Self::High8 | Self::Bank8 | Self::Bank8LowBank => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchFixup {
    /// Byte offset of the encoded SNES operand.
    pub offset: usize,
    /// Index of the allocated payload whose address is written.
    pub target_payload: usize,
    /// Logical byte displacement from the target payload start.
    pub target_addend: usize,
    pub encoding: PatchFixupEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchPayload {
    pub bytes: Vec<u8>,
    pub fixups: Vec<PatchFixup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchWrite {
    pub offset: usize,
    /// Exact revision bytes required before installation.
    pub expected: Vec<u8>,
    /// Equal-length replacement bytes, patched after all allocations are known.
    pub replacement: Vec<u8>,
    pub fixups: Vec<PatchFixup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocatablePatchPlan {
    pub description: String,
    pub mapper: Mapper,
    pub allocation: AllocationPolicy,
    pub checksum_field: usize,
    pub expansion_fill: u8,
    pub payloads: Vec<PatchPayload>,
    pub writes: Vec<PatchWrite>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelocatablePatchResult {
    pub blocks: Vec<RatsBlock>,
    pub snes_addresses: Vec<u32>,
}

#[derive(Debug)]
pub struct RelocatablePatchGroupError {
    pub plan: usize,
    pub source: RelocatablePatchError,
}

#[derive(Debug)]
pub enum RelocatablePatchReplacementError {
    Reclamation(RatsReclamationError),
    Patch(RelocatablePatchError),
}

impl fmt::Display for RelocatablePatchReplacementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "relocatable patch replacement failed: {self:?}")
    }
}

impl std::error::Error for RelocatablePatchReplacementError {}

impl fmt::Display for RelocatablePatchGroupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "relocatable patch group failed at plan {}: {}",
            self.plan, self.source
        )
    }
}

impl std::error::Error for RelocatablePatchGroupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub enum RelocatablePatchError {
    EmptyPayload {
        index: usize,
    },
    EmptyWrite {
        index: usize,
    },
    WriteLengthMismatch {
        index: usize,
    },
    WriteRangeOverflow {
        index: usize,
    },
    OverlappingWrites {
        first: usize,
        second: usize,
    },
    HookPreconditionMismatch {
        index: usize,
        offset: usize,
    },
    FixupRangeOverflow {
        owner: usize,
        offset: usize,
    },
    FixupTargetMissing {
        owner: usize,
        target: usize,
    },
    FixupTargetOverflow {
        owner: usize,
        target: usize,
    },
    FixupOverlapsFixup {
        owner: usize,
        first: usize,
        second: usize,
    },
    InvalidChecksumField(usize),
    ExpansionFillNotAllowed(u8),
    Allocation(AllocationError),
    Rom(RomError),
    Payload(crate::PayloadSaveError),
}

impl fmt::Display for RelocatablePatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "relocatable patch failed: {self:?}")
    }
}

impl std::error::Error for RelocatablePatchError {}

impl From<AllocationError> for RelocatablePatchError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RomError> for RelocatablePatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::PayloadSaveError> for RelocatablePatchError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Payload(value)
    }
}

impl Project {
    /// Installs tagged payloads whose code/data may refer to other allocations in the same group.
    ///
    /// Allocation occurs on a staging ROM. Once every address is known, all three-byte SNES
    /// fixups, identity-checked hook writes, and the checksum are applied before one history batch
    /// is committed.
    ///
    /// # Errors
    ///
    /// Rejects malformed plans, unexpected hook bytes, unsafe overlaps, allocation/mapping
    /// failures, or checksum bounds without changing ROM bytes, history, or project revision.
    pub fn install_relocatable_patch(
        &mut self,
        plan: &RelocatablePatchPlan,
    ) -> Result<RelocatablePatchResult, RelocatablePatchError> {
        let original = self.rom.logical_bytes().to_vec();
        let (staged, result) = stage_relocatable_patch(&original, plan)?;
        commit_staged(self, plan.description.clone(), &original, &staged)?;
        Ok(result)
    }

    /// Installs a relocatable plan while retaining a semantic history marker for undo/redo.
    pub fn install_relocatable_patch_with_kind(
        &mut self,
        plan: &RelocatablePatchPlan,
        kind: EditKind,
    ) -> Result<RelocatablePatchResult, RelocatablePatchError> {
        let original = self.rom.logical_bytes().to_vec();
        let (staged, result) = stage_relocatable_patch(&original, plan)?;
        commit_staged_with_kind(self, plan.description.clone(), &original, &staged, kind)?;
        Ok(result)
    }

    /// Installs several independently allocated patch plans as one project-history operation.
    ///
    /// Each plan keeps its own mapper, search range, fill policy, protected ranges, checksum field,
    /// payload namespace, and exact write preconditions. Later plans observe the staged output of
    /// earlier plans, but no ROM or history mutation escapes unless every plan succeeds.
    ///
    /// # Errors
    ///
    /// Returns the failing plan index and its ordinary relocatable-patch error without changing the
    /// project.
    pub fn install_relocatable_patch_group(
        &mut self,
        description: impl Into<String>,
        plans: &[RelocatablePatchPlan],
    ) -> Result<Vec<RelocatablePatchResult>, RelocatablePatchGroupError> {
        let original = self.rom.logical_bytes().to_vec();
        let mut staged = original.clone();
        let mut results = Vec::with_capacity(plans.len());
        for (index, plan) in plans.iter().enumerate() {
            let (next, result) = stage_relocatable_patch(&staged, plan).map_err(|source| {
                RelocatablePatchGroupError {
                    plan: index,
                    source,
                }
            })?;
            staged = next;
            results.push(result);
        }
        commit_staged(self, description.into(), &original, &staged).map_err(|source| {
            RelocatablePatchGroupError {
                plan: plans.len(),
                source: RelocatablePatchError::Payload(source),
            }
        })?;
        Ok(results)
    }

    /// Reclaims explicitly owned obsolete blocks and installs their replacement as one history
    /// operation.
    ///
    /// Ownership is validated against the original ROM before any bytes are staged. The reclaimed
    /// ranges become available to the allocator, while the replacement plan still checks every
    /// fixed-write precondition against the otherwise unchanged source. A reclamation, allocation,
    /// fixup, precondition, or checksum failure leaves ROM bytes and history untouched.
    ///
    /// # Errors
    ///
    /// Returns a typed reclamation or relocatable-patch error without modifying the project.
    pub fn replace_relocatable_patch(
        &mut self,
        plan: &RelocatablePatchPlan,
        obsolete: &RatsOwnershipManifest,
        reclamation_fill: u8,
    ) -> Result<RelocatablePatchResult, RelocatablePatchReplacementError> {
        let original = self.rom.logical_bytes().to_vec();
        let reclamation = self
            .plan_rats_reclamation(obsolete, reclamation_fill)
            .map_err(RelocatablePatchReplacementError::Reclamation)?;
        let mut reclaimed = original.clone();
        for write in reclamation.writes {
            let end = write.offset + write.bytes.len();
            reclaimed[write.offset..end].copy_from_slice(&write.bytes);
        }
        let (staged, result) = stage_relocatable_patch(&reclaimed, plan)
            .map_err(RelocatablePatchReplacementError::Patch)?;
        commit_staged(self, plan.description.clone(), &original, &staged)
            .map_err(|error| RelocatablePatchReplacementError::Patch(error.into()))?;
        Ok(result)
    }
}

fn stage_relocatable_patch(
    original: &[u8],
    plan: &RelocatablePatchPlan,
) -> Result<(Vec<u8>, RelocatablePatchResult), RelocatablePatchError> {
    let mut staged = expanded_image(original, plan)?;
    let write_ranges = validate_plan(&staged, plan)?;
    let mut policy = plan.allocation.clone();
    policy.protected.extend(
        write_ranges
            .iter()
            .cloned()
            .chain(std::iter::once(
                plan.checksum_field..plan.checksum_field + 4,
            ))
            .map(ProtectedRange),
    );
    policy.validate(staged.len())?;

    let mut blocks = Vec::with_capacity(plan.payloads.len());
    for payload in &plan.payloads {
        let placeholder = vec![0; payload.bytes.len()];
        blocks.push(FreeSpaceAllocator::new(&mut staged, policy.clone()).allocate(&placeholder)?);
    }
    let snes_addresses = blocks
        .iter()
        .map(|block| pc_to_snes(plan.mapper, block.payload.start))
        .collect::<Result<Vec<_>, _>>()?;

    for (owner, (payload, block)) in plan.payloads.iter().zip(&blocks).enumerate() {
        let mut bytes = payload.bytes.clone();
        apply_fixups(&mut bytes, &payload.fixups, &blocks, plan.mapper, owner)?;
        staged[block.payload.clone()].copy_from_slice(&bytes);
    }
    for (owner, write) in plan.writes.iter().enumerate() {
        let mut bytes = write.replacement.clone();
        apply_fixups(
            &mut bytes,
            &write.fixups,
            &blocks,
            plan.mapper,
            plan.payloads.len() + owner,
        )?;
        staged[write.offset..write.offset + bytes.len()].copy_from_slice(&bytes);
    }
    let checksum = compute_snes_checksum(&staged, plan.checksum_field)?;
    staged[plan.checksum_field..plan.checksum_field + 2]
        .copy_from_slice(&checksum.complement.to_le_bytes());
    staged[plan.checksum_field + 2..plan.checksum_field + 4]
        .copy_from_slice(&checksum.checksum.to_le_bytes());
    Ok((
        staged,
        RelocatablePatchResult {
            blocks,
            snes_addresses,
        },
    ))
}

fn expanded_image(
    original: &[u8],
    plan: &RelocatablePatchPlan,
) -> Result<Vec<u8>, RelocatablePatchError> {
    if plan.allocation.search.end <= original.len() {
        return Ok(original.to_vec());
    }
    if !plan.allocation.fill_bytes.contains(&plan.expansion_fill) {
        return Err(RelocatablePatchError::ExpansionFillNotAllowed(
            plan.expansion_fill,
        ));
    }
    let mut image = RomImage::from_bytes(original.to_vec())?;
    image.expand(plan.mapper, plan.allocation.search.end, plan.expansion_fill)?;
    Ok(image.logical_bytes().to_vec())
}

fn validate_plan(
    staged: &[u8],
    plan: &RelocatablePatchPlan,
) -> Result<Vec<Range<usize>>, RelocatablePatchError> {
    let checksum_end =
        plan.checksum_field
            .checked_add(4)
            .ok_or(RelocatablePatchError::InvalidChecksumField(
                plan.checksum_field,
            ))?;
    if checksum_end > staged.len() {
        return Err(RelocatablePatchError::InvalidChecksumField(
            plan.checksum_field,
        ));
    }
    for (index, payload) in plan.payloads.iter().enumerate() {
        if payload.bytes.is_empty() {
            return Err(RelocatablePatchError::EmptyPayload { index });
        }
        validate_fixups(
            payload.bytes.len(),
            &payload.fixups,
            index,
            plan.payloads.len(),
        )?;
    }
    let mut ranges = Vec::with_capacity(plan.writes.len());
    for (index, write) in plan.writes.iter().enumerate() {
        if write.replacement.is_empty() {
            return Err(RelocatablePatchError::EmptyWrite { index });
        }
        if write.expected.len() != write.replacement.len() {
            return Err(RelocatablePatchError::WriteLengthMismatch { index });
        }
        let end = write
            .offset
            .checked_add(write.replacement.len())
            .filter(|end| *end <= staged.len())
            .ok_or(RelocatablePatchError::WriteRangeOverflow { index })?;
        if staged[write.offset..end] != write.expected {
            return Err(RelocatablePatchError::HookPreconditionMismatch {
                index,
                offset: write.offset,
            });
        }
        validate_fixups(
            write.replacement.len(),
            &write.fixups,
            plan.payloads.len() + index,
            plan.payloads.len(),
        )?;
        ranges.push(write.offset..end);
    }
    ranges.sort_by_key(|range| range.start);
    for pair in ranges.windows(2) {
        if pair[0].end > pair[1].start {
            return Err(RelocatablePatchError::OverlappingWrites {
                first: pair[0].start,
                second: pair[1].start,
            });
        }
    }
    if ranges
        .iter()
        .any(|range| range.start < checksum_end && plan.checksum_field < range.end)
    {
        return Err(RelocatablePatchError::InvalidChecksumField(
            plan.checksum_field,
        ));
    }
    Ok(ranges)
}

fn validate_fixups(
    len: usize,
    fixups: &[PatchFixup],
    owner: usize,
    payload_count: usize,
) -> Result<(), RelocatablePatchError> {
    let mut ranges = Vec::with_capacity(fixups.len());
    for fixup in fixups {
        if fixup.target_payload >= payload_count {
            return Err(RelocatablePatchError::FixupTargetMissing {
                owner,
                target: fixup.target_payload,
            });
        }
        let end = fixup
            .offset
            .checked_add(fixup.encoding.encoded_len())
            .filter(|end| *end <= len)
            .ok_or(RelocatablePatchError::FixupRangeOverflow {
                owner,
                offset: fixup.offset,
            })?;
        ranges.push(fixup.offset..end);
    }
    ranges.sort_by_key(|range| range.start);
    for pair in ranges.windows(2) {
        if pair[0].end > pair[1].start {
            return Err(RelocatablePatchError::FixupOverlapsFixup {
                owner,
                first: pair[0].start,
                second: pair[1].start,
            });
        }
    }
    Ok(())
}

fn apply_fixups(
    bytes: &mut [u8],
    fixups: &[PatchFixup],
    blocks: &[RatsBlock],
    mapper: Mapper,
    owner: usize,
) -> Result<(), RelocatablePatchError> {
    for fixup in fixups {
        let block = &blocks[fixup.target_payload];
        let target = block
            .payload
            .start
            .checked_add(fixup.target_addend)
            .filter(|target| *target < block.payload.end)
            .ok_or(RelocatablePatchError::FixupTargetOverflow {
                owner,
                target: fixup.target_payload,
            })?;
        let snes = pc_to_snes(mapper, target)?;
        let mut encoded = snes.to_le_bytes();
        if matches!(
            fixup.encoding,
            PatchFixupEncoding::Long24LowBank | PatchFixupEncoding::Bank8LowBank
        ) {
            encoded[2] &= 0x7f;
        }
        let replacement: &[u8] = match fixup.encoding {
            PatchFixupEncoding::Long24 | PatchFixupEncoding::Long24LowBank => &encoded[..3],
            PatchFixupEncoding::Low16 => &encoded[..2],
            PatchFixupEncoding::Low8 => &encoded[..1],
            PatchFixupEncoding::High8 => &encoded[1..2],
            PatchFixupEncoding::Bank8 | PatchFixupEncoding::Bank8LowBank => &encoded[2..3],
        };
        bytes[fixup.offset..fixup.offset + replacement.len()].copy_from_slice(replacement);
    }
    Ok(())
}

#[cfg(test)]
#[path = "relocatable_patch_tests.rs"]
mod tests;
