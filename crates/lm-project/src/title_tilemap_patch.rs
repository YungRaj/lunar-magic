//! Lunar Magic title-screen Layer 3 tilemap detection and transactional persistence.

use crate::{
    PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, Project, RelocatablePatchError,
    RelocatablePatchPlan, payload::staging::commit_staged,
};
use lm_graphics::{GRAPHICS_REMAP_WORDS, GraphicsRemapCommandStream, GraphicsRemapError};
use lm_overworld::{ExpandedLayerTilemap, ExpandedLayerTilemapError};
use lm_rats::{
    AllocationError, AllocationPolicy, FreeSpaceAllocator, ProtectedRange, RatsBlock, parse_at,
};
use lm_rom::{Mapper, RomError, RomImage, compute_snes_checksum, pc_to_snes, snes_to_pc};

const TITLE_PRIMARY_BLANK_TILE: u16 = 0x38fc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TitleTilemapPatchLocator {
    pub mapper: Mapper,
    /// Contiguous 24-bit SNES pointer consumed by the title-screen loader.
    pub pointer_operand: usize,
    /// Exact pristine target, used to distinguish vanilla data from an unowned redirection.
    pub pristine_stream: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TitleTilemapStorage {
    Pristine,
    Expanded(RatsBlock),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedTitleTilemap {
    pub tilemap: ExpandedLayerTilemap,
    pub storage: TitleTilemapStorage,
}

#[derive(Debug)]
pub enum TitleTilemapPatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    UnexpectedPointer { actual: usize },
    MissingOwnership,
    OwnedPayloadLength(usize),
    Rom(RomError),
    Stream(GraphicsRemapError),
    Tilemap(ExpandedLayerTilemapError),
    Allocation(AllocationError),
    Install(RelocatablePatchError),
    Commit(crate::PayloadSaveError),
    ReopenMismatch,
}

impl std::fmt::Display for TitleTilemapPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "title-screen tilemap patch failed: {self:?}")
    }
}

impl std::error::Error for TitleTilemapPatchError {}

impl From<RomError> for TitleTilemapPatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<GraphicsRemapError> for TitleTilemapPatchError {
    fn from(value: GraphicsRemapError) -> Self {
        Self::Stream(value)
    }
}

impl From<ExpandedLayerTilemapError> for TitleTilemapPatchError {
    fn from(value: ExpandedLayerTilemapError) -> Self {
        Self::Tilemap(value)
    }
}

impl From<AllocationError> for TitleTilemapPatchError {
    fn from(value: AllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<RelocatablePatchError> for TitleTilemapPatchError {
    fn from(value: RelocatablePatchError) -> Self {
        Self::Install(value)
    }
}

impl From<crate::PayloadSaveError> for TitleTilemapPatchError {
    fn from(value: crate::PayloadSaveError) -> Self {
        Self::Commit(value)
    }
}

impl Project {
    /// Loads the pristine command stream or an exact RATS-owned Lunar Magic literal stream.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, foreign pointer redirections, malformed command streams,
    /// malformed canonical streams, and pointers without exact RATS ownership.
    pub fn load_title_tilemap_detected(
        &self,
        locator: TitleTilemapPatchLocator,
    ) -> Result<LoadedTitleTilemap, TitleTilemapPatchError> {
        validate_mapper(self, locator.mapper)?;
        let target = read_pointer(self, locator.pointer_operand, locator.mapper)?;
        if target == locator.pristine_stream {
            let input = &self.rom.logical_bytes()[target..];
            let decoded = GraphicsRemapCommandStream::decode_prefix(input)?;
            let mut scratch = vec![ExpandedLayerTilemap::BLANK_TILE; GRAPHICS_REMAP_WORDS];
            decoded.stream.apply(&mut scratch)?;
            let primary_words = &mut scratch[0x5000..0x5000 + ExpandedLayerTilemap::WORD_COUNT];
            // Lunar Magic materializes untouched primary title-screen cells with palette/priority
            // attributes `$38`, while remap scratch and the blank secondary plane use `$00FC`.
            // Its pristine-to-pristine TransferTitleScreen command proves this normalization.
            for word in primary_words.iter_mut() {
                if *word == ExpandedLayerTilemap::BLANK_TILE {
                    *word = TITLE_PRIMARY_BLANK_TILE;
                }
            }
            let primary = words_as_bytes(primary_words);
            let secondary =
                words_as_bytes(&scratch[0x5400..0x5400 + ExpandedLayerTilemap::WORD_COUNT]);
            return Ok(LoadedTitleTilemap {
                tilemap: ExpandedLayerTilemap::decode_planes(&primary, &secondary)?,
                storage: TitleTilemapStorage::Pristine,
            });
        }
        let header = target
            .checked_sub(lm_rats::HEADER_LEN)
            .ok_or(TitleTilemapPatchError::MissingOwnership)?;
        let block = parse_at(self.rom.logical_bytes(), header)
            .map_err(|_| TitleTilemapPatchError::MissingOwnership)?;
        if block.payload.start != target {
            return Err(TitleTilemapPatchError::MissingOwnership);
        }
        let payload = self.rom.read(block.payload.start, block.payload.len())?;
        if payload.len() != 0x745 && payload.len() != 0xe89 {
            return Err(TitleTilemapPatchError::OwnedPayloadLength(payload.len()));
        }
        Ok(LoadedTitleTilemap {
            tilemap: ExpandedLayerTilemap::decode_native_stream(payload)?,
            storage: TitleTilemapStorage::Expanded(block),
        })
    }

    /// Installs or replaces Lunar Magic's canonical title tilemap allocation atomically.
    ///
    /// # Errors
    ///
    /// Rejects foreign current storage, unsafe allocation, pointer/checksum failures, or semantic
    /// disagreement after reopen. Failed operations leave the project unchanged.
    pub fn save_title_tilemap_detected(
        &mut self,
        tilemap: &ExpandedLayerTilemap,
        locator: TitleTilemapPatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, TitleTilemapPatchError> {
        let loaded = self.load_title_tilemap_detected(locator)?;
        if loaded.tilemap == *tilemap {
            return Ok(false);
        }
        let payload = tilemap.encode_native_stream();
        match loaded.storage {
            TitleTilemapStorage::Pristine => {
                self.install_relocatable_patch(&RelocatablePatchPlan {
                    description: "install title-screen tilemap".into(),
                    mapper: locator.mapper,
                    allocation: allocation.clone(),
                    checksum_field,
                    expansion_fill: fill,
                    payloads: vec![PatchPayload {
                        bytes: payload,
                        fixups: Vec::new(),
                    }],
                    writes: vec![PatchWrite {
                        offset: locator.pointer_operand,
                        expected: low_bank_pointer(locator.mapper, locator.pristine_stream)?,
                        replacement: vec![0; 3],
                        fixups: vec![PatchFixup {
                            offset: 0,
                            target_payload: 0,
                            target_addend: 0,
                            encoding: PatchFixupEncoding::Long24LowBank,
                        }],
                    }],
                })?;
            }
            TitleTilemapStorage::Expanded(previous) => replace_expanded(
                self,
                locator,
                &payload,
                &previous,
                allocation,
                checksum_field,
                fill,
            )?,
        }
        if self.load_title_tilemap_detected(locator)?.tilemap != *tilemap {
            return Err(TitleTilemapPatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn replace_expanded(
    project: &mut Project,
    locator: TitleTilemapPatchLocator,
    payload: &[u8],
    previous: &RatsBlock,
    allocation: &AllocationPolicy,
    checksum_field: usize,
    fill: u8,
) -> Result<(), TitleTilemapPatchError> {
    let original = project.rom.logical_bytes().to_vec();
    let mut image = RomImage::from_bytes(original.clone())?;
    if allocation.search.end > image.logical_len() {
        image.expand(locator.mapper, allocation.search.end, fill)?;
    }
    let mut staged = image.logical_bytes().to_vec();
    if parse_at(&staged, previous.header_offset).ok().as_ref() != Some(previous) {
        return Err(TitleTilemapPatchError::MissingOwnership);
    }
    let mut policy = allocation.clone();
    policy.protected.extend([
        ProtectedRange(locator.pointer_operand..locator.pointer_operand + 3),
        ProtectedRange(checksum_field..checksum_field + 4),
    ]);
    policy.validate(staged.len())?;
    {
        let mut allocator = FreeSpaceAllocator::new(&mut staged, policy.clone());
        allocator.erase(previous, fill)?;
    }
    let block = FreeSpaceAllocator::new(&mut staged, policy).allocate(payload)?;
    staged[locator.pointer_operand..locator.pointer_operand + 3]
        .copy_from_slice(&low_bank_pointer(locator.mapper, block.payload.start)?);
    let checksum = compute_snes_checksum(&staged, checksum_field)?;
    staged[checksum_field..checksum_field + 4].copy_from_slice(&checksum.encoded());
    commit_staged(
        project,
        "replace title-screen tilemap".into(),
        &original,
        &staged,
    )?;
    Ok(())
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), TitleTilemapPatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(TitleTilemapPatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn read_pointer(
    project: &Project,
    offset: usize,
    mapper: Mapper,
) -> Result<usize, TitleTilemapPatchError> {
    let bytes = project.rom.read(offset, 3)?;
    let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
    Ok(snes_to_pc(mapper, address)?)
}

fn words_as_bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn low_bank_pointer(mapper: Mapper, pc: usize) -> Result<Vec<u8>, RomError> {
    let mut bytes = pc_to_snes(mapper, pc)?.to_le_bytes()[..3].to_vec();
    // The low-bank mirror is equivalent only under LoROM. ExLoROM bit 23 selects the ROM half,
    // and SA-1 uses the full bank value as part of its mapping.
    if mapper == Mapper::LoRom {
        bytes[2] &= 0x7f;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_tilemap_pointer_preserves_mapper_significant_high_banks() {
        for (mapper, pc) in [
            (Mapper::LoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x2_0000),
            (Mapper::ExLoRom, 0x42_0000),
            (Mapper::Sa1, 0x2_0000),
            (Mapper::Sa1, 0x42_0000),
        ] {
            let pointer = low_bank_pointer(mapper, pc).unwrap();
            assert_eq!(
                snes_to_pc(
                    mapper,
                    u32::from(pointer[0])
                        | u32::from(pointer[1]) << 8
                        | u32::from(pointer[2]) << 16,
                )
                .unwrap(),
                pc
            );
        }
    }
}
