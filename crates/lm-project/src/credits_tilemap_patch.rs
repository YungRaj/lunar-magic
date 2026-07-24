//! Lunar Magic expanded credits row runtime detection, installation, and replacement.

use crate::{
    CreditsTilemapIoError, LegacyCreditsTilemapLayout, PatchFixup, PatchFixupEncoding,
    PatchPayload, PatchWrite, Project, RelocatablePatchError, RelocatablePatchPlan,
    payload::staging::commit_staged,
};
use lm_overworld::{CreditsTilemap, CreditsTilemapError, EncodedCreditsRows};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditsTilemapPatchLocator {
    pub mapper: Mapper,
    pub legacy: LegacyCreditsTilemapLayout,
    pub runtime: usize,
    pub expanded_offsets: usize,
    pub runtime_template: [u8; Self::RUNTIME_LEN],
}

impl CreditsTilemapPatchLocator {
    pub const RUNTIME_LEN: usize = 0x60;
    pub const PRIMARY_POINTER: usize = 3;
    pub const SECONDARY_POINTER: usize = 0x30;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditsTilemapStorage {
    Legacy,
    Expanded(RatsBlock),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedCreditsTilemap {
    pub tilemap: CreditsTilemap,
    pub storage: CreditsTilemapStorage,
}

#[derive(Debug)]
pub enum CreditsTilemapPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    RuntimeSignature,
    RuntimeMismatch,
    OffsetPointerMismatch,
    RecordPointersDisagree,
    MissingOwnership,
    Rom(RomError),
    Tilemap(CreditsTilemapError),
    Legacy(CreditsTilemapIoError),
    Allocation(AllocationError),
    Install(RelocatablePatchError),
    Commit(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for CreditsTilemapPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded credits tilemap patch failed: {self:?}")
    }
}

impl std::error::Error for CreditsTilemapPatchError {}

impl From<RomError> for CreditsTilemapPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<CreditsTilemapError> for CreditsTilemapPatchError {
    fn from(value: CreditsTilemapError) -> Self {
        Self::Tilemap(value)
    }
}

impl From<CreditsTilemapIoError> for CreditsTilemapPatchError {
    fn from(value: CreditsTilemapIoError) -> Self {
        Self::Legacy(value)
    }
}

impl From<AllocationError> for CreditsTilemapPatchError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RelocatablePatchError> for CreditsTilemapPatchError {
    fn from(value: RelocatablePatchError) -> Self {
        Self::Install(value)
    }
}

impl From<crate::PayloadSaveError> for CreditsTilemapPatchError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Loads either the pristine 202-row layout or Lunar Magic's exact expanded runtime.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, partial runtime signatures, altered fixed code, disagreeing
    /// pointers, missing exact RATS ownership, malformed offsets, or malformed row records.
    pub fn load_credits_tilemap_detected(
        &self,
        locator: &CreditsTilemapPatchLocator,
    ) -> Result<LoadedCreditsTilemap, CreditsTilemapPatchError> {
        validate_mapper(self, locator.mapper)?;
        if self.rom.read(locator.runtime + 2, 1)?[0] != 0xbf {
            return Ok(LoadedCreditsTilemap {
                tilemap: self.load_legacy_credits_tilemap(locator.legacy)?,
                storage: CreditsTilemapStorage::Legacy,
            });
        }
        validate_runtime(self, locator)?;
        let first = read_pointer(
            self,
            locator.runtime + CreditsTilemapPatchLocator::PRIMARY_POINTER,
            locator.mapper,
        )?;
        let second = read_pointer(
            self,
            locator.runtime + CreditsTilemapPatchLocator::SECONDARY_POINTER,
            locator.mapper,
        )?;
        if first != second {
            return Err(CreditsTilemapPatchError::RecordPointersDisagree);
        }
        let header = first
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(CreditsTilemapPatchError::MissingOwnership)?;
        let block = parse_at(self.rom.logical_bytes(), header)
            .map_err(|_| CreditsTilemapPatchError::MissingOwnership)?;
        if block.payload.start != first {
            return Err(CreditsTilemapPatchError::MissingOwnership);
        }
        let offsets = decode_offsets(
            self.rom
                .read(locator.expanded_offsets, CreditsTilemap::OFFSET_TABLE_LEN)?,
        );
        let records = self.rom.read(block.payload.start, block.payload.len())?;
        Ok(LoadedCreditsTilemap {
            tilemap: CreditsTilemap::decode_rows(&offsets, records, locator.legacy.blank_word)?,
            storage: CreditsTilemapStorage::Expanded(block),
        })
    }

    /// Installs or replaces the expanded runtime and all 256 rows as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Rejects malformed current storage, unsafe allocation policy, fixed-byte disagreement,
    /// mapping/checksum failures, or semantic disagreement after reopen. Failure is atomic.
    pub fn save_credits_tilemap_detected(
        &mut self,
        tilemap: &CreditsTilemap,
        locator: &CreditsTilemapPatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, CreditsTilemapPatchError> {
        let loaded = self.load_credits_tilemap_detected(locator)?;
        if loaded.tilemap == *tilemap {
            return Ok(false);
        }
        let encoded = tilemap.encode_rows(locator.legacy.blank_word)?;
        match loaded.storage {
            CreditsTilemapStorage::Legacy => {
                let plan =
                    installation_plan(self, locator, encoded, allocation, checksum_field, fill)?;
                self.install_relocatable_patch(&plan)?;
            }
            CreditsTilemapStorage::Expanded(block) => {
                replace_expanded(
                    self,
                    locator,
                    &encoded,
                    &block,
                    allocation,
                    checksum_field,
                    fill,
                )?;
            }
        }
        if self.load_credits_tilemap_detected(locator)?.tilemap != *tilemap {
            return Err(CreditsTilemapPatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn installation_plan(
    project: &Project,
    locator: &CreditsTilemapPatchLocator,
    encoded: EncodedCreditsRows,
    allocation: &AllocationPolicy,
    checksum_field: usize,
    fill: u8,
) -> Result<RelocatablePatchPlan, CreditsTilemapPatchError> {
    let low_word = expanded_offset_low_word(locator)?;
    let offset_bytes = encoded.offset_bytes();
    let mut runtime = locator.runtime_template;
    runtime[CreditsTilemapPatchLocator::PRIMARY_POINTER
        ..CreditsTilemapPatchLocator::PRIMARY_POINTER + 3]
        .fill(0);
    runtime[CreditsTilemapPatchLocator::SECONDARY_POINTER
        ..CreditsTilemapPatchLocator::SECONDARY_POINTER + 3]
        .fill(0);
    Ok(RelocatablePatchPlan {
        description: "install expanded credits tilemap".into(),
        mapper: locator.mapper,
        allocation: allocation.clone(),
        checksum_field,
        expansion_fill: fill,
        payloads: vec![PatchPayload {
            bytes: encoded.records,
            fixups: Vec::new(),
        }],
        writes: vec![
            PatchWrite {
                offset: locator.runtime - 2,
                expected: project.rom.read(locator.runtime - 2, 2)?.to_vec(),
                replacement: low_word.to_le_bytes().to_vec(),
                fixups: Vec::new(),
            },
            PatchWrite {
                offset: locator.runtime,
                expected: project
                    .rom
                    .read(locator.runtime, CreditsTilemapPatchLocator::RUNTIME_LEN)?
                    .to_vec(),
                replacement: runtime.to_vec(),
                fixups: vec![
                    pointer_fixup(CreditsTilemapPatchLocator::PRIMARY_POINTER),
                    pointer_fixup(CreditsTilemapPatchLocator::SECONDARY_POINTER),
                ],
            },
            PatchWrite {
                offset: locator.expanded_offsets,
                expected: project
                    .rom
                    .read(locator.expanded_offsets, CreditsTilemap::OFFSET_TABLE_LEN)?
                    .to_vec(),
                replacement: offset_bytes,
                fixups: Vec::new(),
            },
        ],
    })
}

fn replace_expanded(
    project: &mut Project,
    locator: &CreditsTilemapPatchLocator,
    encoded: &EncodedCreditsRows,
    previous: &RatsBlock,
    allocation: &AllocationPolicy,
    checksum_field: usize,
    fill: u8,
) -> Result<(), CreditsTilemapPatchError> {
    let original = project.rom.logical_bytes().to_vec();
    let mut image = RomImage::from_bytes(original.clone())?;
    if allocation.search.end > image.logical_len() {
        image.expand(locator.mapper, allocation.search.end, fill)?;
    }
    let mut staged = image.logical_bytes().to_vec();
    if parse_at(&staged, previous.header_offset).ok().as_ref() != Some(previous) {
        return Err(CreditsTilemapPatchError::MissingOwnership);
    }
    let mut policy = allocation.clone();
    policy.protected.extend([
        ProtectedRange(
            locator.runtime - 2..locator.runtime + CreditsTilemapPatchLocator::RUNTIME_LEN,
        ),
        ProtectedRange(
            locator.expanded_offsets..locator.expanded_offsets + CreditsTilemap::OFFSET_TABLE_LEN,
        ),
        ProtectedRange(checksum_field..checksum_field + 4),
    ]);
    policy.validate(staged.len())?;
    {
        let mut allocator = FreeSpaceAllocator::new(&mut staged, policy.clone());
        allocator.erase(previous, fill)?;
    }
    let block = FreeSpaceAllocator::new(&mut staged, policy).allocate(&encoded.records)?;
    let pointer = pc_to_snes(locator.mapper, block.payload.start)?.to_le_bytes();
    for operand in [
        CreditsTilemapPatchLocator::PRIMARY_POINTER,
        CreditsTilemapPatchLocator::SECONDARY_POINTER,
    ] {
        staged[locator.runtime + operand..locator.runtime + operand + 3]
            .copy_from_slice(&pointer[..3]);
    }
    staged[locator.expanded_offsets..locator.expanded_offsets + CreditsTilemap::OFFSET_TABLE_LEN]
        .copy_from_slice(&encoded.offset_bytes());
    let checksum = compute_snes_checksum(&staged, checksum_field)?;
    staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
    commit_staged(
        project,
        "replace expanded credits tilemap".into(),
        &original,
        &staged,
    )?;
    Ok(())
}

fn validate_runtime(
    project: &Project,
    locator: &CreditsTilemapPatchLocator,
) -> Result<(), CreditsTilemapPatchError> {
    if project.rom.read(locator.runtime - 2, 2)? != expanded_offset_low_word(locator)?.to_le_bytes()
    {
        return Err(CreditsTilemapPatchError::OffsetPointerMismatch);
    }
    let actual = project
        .rom
        .read(locator.runtime, CreditsTilemapPatchLocator::RUNTIME_LEN)?;
    for (index, (observed, expected)) in actual.iter().zip(locator.runtime_template).enumerate() {
        let pointer_byte = (CreditsTilemapPatchLocator::PRIMARY_POINTER
            ..CreditsTilemapPatchLocator::PRIMARY_POINTER + 3)
            .contains(&index)
            || (CreditsTilemapPatchLocator::SECONDARY_POINTER
                ..CreditsTilemapPatchLocator::SECONDARY_POINTER + 3)
                .contains(&index);
        if !pointer_byte && *observed != expected {
            return Err(CreditsTilemapPatchError::RuntimeMismatch);
        }
    }
    Ok(())
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), CreditsTilemapPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(CreditsTilemapPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn expanded_offset_low_word(
    locator: &CreditsTilemapPatchLocator,
) -> Result<u16, CreditsTilemapPatchError> {
    let address = pc_to_snes(locator.mapper, locator.expanded_offsets)?.to_le_bytes();
    Ok(u16::from_le_bytes([address[0], address[1]]))
}

fn read_pointer(
    project: &Project,
    offset: usize,
    mapper: Mapper,
) -> Result<usize, CreditsTilemapPatchError> {
    let bytes = project.rom.read(offset, 3)?;
    let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    Ok(snes_to_pc(mapper, address)?)
}

fn decode_offsets(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .collect()
}

fn pointer_fixup(offset: usize) -> PatchFixup {
    PatchFixup {
        offset,
        target_payload: 0,
        target_addend: 0,
        encoding: PatchFixupEncoding::Long24,
    }
}
