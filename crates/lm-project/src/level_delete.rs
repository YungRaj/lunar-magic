use crate::{
    LevelLayer2RomLayout, LevelLoadError, LevelPointerTable, LevelRomLayout,
    Lfix3LevelFieldsRomLayout, NativeLevelAssetsLayout, PayloadLoadError, PayloadReadPolicy,
    Project, RomWrite, SpritePointerTable, VanillaEntranceRomLayout,
};
use lm_level::ExpandedLevelSettingsRecord;
use lm_rats::RatsBlock;
use lm_rom::{RomError, SnesPointer24, compute_snes_checksum, snes_to_pc};
use std::fmt;

/// Result of redirecting one level's native streams to the original-area test level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletedLevelStreams {
    pub level: usize,
    pub reclaimed: Vec<RatsBlock>,
    pub reclaimed_bytes: usize,
}

/// Result of deleting every pointer-backed asset modeled by [`NativeLevelAssetsLayout`].
pub type DeletedNativeLevelAssets = DeletedLevelStreams;

const ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET: usize = 0x30258;
const ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD: &[u8; 32] = b"Free Area DO NOT ERASE THIS TAG!";
const ORIGINAL_LEVEL_CLEAR_METADATA_OFFSET: usize = 0x7efc2;
const ORIGINAL_LEVEL_CLEAR_METADATA_LEN: usize = 0x6f;
const ORIGINAL_LEVEL_CLEAR_TEST_SPRITE_LOW_BYTES: [(usize, u8); 2] =
    [(0x2da7f, 0x6d), (0x2da83, 0xe7)];
const ORIGINAL_LEVEL_PROTECTED_BLOCKS: [(usize, usize); 7] = [
    (0x3495c, 0x35000),
    (0x3697d, 0x36cc9),
    (0x37531, 0x380c3),
    (0x3a171, 0x3a600),
    (0x3c21e, 0x3c300),
    (0x3d7dd, 0x3d8be),
    (0x3e765, 0x40000),
];

#[derive(Debug)]
pub enum DeleteLevelStreamsError {
    Level(LevelLoadError),
    Payload(PayloadLoadError),
    Rom(RomError),
    Transaction(crate::TransactionError),
    LevelNotExpanded { level: usize, layer1_offset: usize },
    ReplacementOutsideOriginalArea { pointer: u32, offset: usize },
    SharedSpriteBankMismatch { installed: u8, replacement: u8 },
    ReclaimedByteCountOverflow,
    ReplacementLevelOutOfRange { level: usize, entries: usize },
    DirectTableLayout,
}

impl fmt::Display for DeleteLevelStreamsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "delete level streams failed: {self:?}")
    }
}

impl std::error::Error for DeleteLevelStreamsError {}

impl From<LevelLoadError> for DeleteLevelStreamsError {
    fn from(value: LevelLoadError) -> Self {
        Self::Level(value)
    }
}

impl From<PayloadLoadError> for DeleteLevelStreamsError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Payload(value)
    }
}

impl From<RomError> for DeleteLevelStreamsError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<crate::TransactionError> for DeleteLevelStreamsError {
    fn from(value: crate::TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Clears Lunar Magic's authenticated original SMW level-data area while protecting the
    /// seven fixed runtime/data islands with exact RATS owners.
    ///
    /// The marker owner at `$30258` makes the operation idempotent. The gaps, owner headers, clear
    /// metadata, and checksum are one Undo step. Protected payload bytes are never rewritten.
    pub fn clear_original_level_data_area(
        &mut self,
        description: impl Into<String>,
        checksum_field: usize,
    ) -> Result<bool, DeleteLevelStreamsError> {
        let marker = rats_owner_bytes(ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD.len())?;
        let marker_end = ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET
            + marker.len()
            + ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD.len();
        if self
            .rom
            .read(ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET, marker.len())?
            == marker
            && self.rom.read(
                ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET + marker.len(),
                ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD.len(),
            )? == ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD
        {
            return Ok(false);
        }

        let mut writes = vec![
            RomWrite {
                offset: ORIGINAL_LEVEL_CLEAR_TEST_SPRITE_LOW_BYTES[0].0,
                bytes: vec![ORIGINAL_LEVEL_CLEAR_TEST_SPRITE_LOW_BYTES[0].1],
            },
            RomWrite {
                offset: ORIGINAL_LEVEL_CLEAR_TEST_SPRITE_LOW_BYTES[1].0,
                bytes: vec![ORIGINAL_LEVEL_CLEAR_TEST_SPRITE_LOW_BYTES[1].1],
            },
            RomWrite {
                offset: 0x30253,
                bytes: vec![0; ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET - 0x30253],
            },
            RomWrite {
                offset: ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET,
                bytes: [marker, ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD.to_vec()].concat(),
            },
        ];
        let mut cursor = marker_end;
        for &(header_offset, end) in &ORIGINAL_LEVEL_PROTECTED_BLOCKS {
            writes.push(RomWrite {
                offset: cursor,
                bytes: vec![0; header_offset - cursor],
            });
            let payload_len = end - header_offset - 8;
            writes.push(RomWrite {
                offset: header_offset,
                bytes: rats_owner_bytes(payload_len)?,
            });
            cursor = end;
        }
        let mut metadata = vec![0; ORIGINAL_LEVEL_CLEAR_METADATA_LEN];
        metadata[0] = 0xaa;
        writes.push(RomWrite {
            offset: ORIGINAL_LEVEL_CLEAR_METADATA_OFFSET,
            bytes: metadata,
        });

        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        self.apply_writes(description, &writes)?;
        Ok(true)
    }

    /// Replaces one level's object and sprite streams with original-area test streams.
    ///
    /// Tagged displaced streams are erased only when no other object or sprite table entry still
    /// points at them. Pointer replacement, reclamation, and checksum repair are committed as one
    /// undoable transaction. This is the core operation behind Lunar Magic 3.50+'s “Delete Level
    /// from ROM”; higher-level per-level assets are deliberately handled by the aggregate caller.
    ///
    /// # Errors
    ///
    /// Rejects invalid layouts or pointers, replacement streams outside the original 512-KiB
    /// area, a shared-bank sprite layout whose bank cannot represent the replacement, malformed
    /// tagged payloads, checksum failures, and atomic transaction failures.
    pub fn delete_level_streams_to_original(
        &mut self,
        description: impl Into<String>,
        layout: LevelRomLayout,
        level: usize,
        replacement_layer1: SnesPointer24,
        replacement_sprites: SnesPointer24,
        checksum_field: usize,
        erase_fill: u8,
    ) -> Result<DeletedLevelStreams, DeleteLevelStreamsError> {
        const ORIGINAL_AREA_LEN: usize = 0x80_000;
        for pointer in [replacement_layer1, replacement_sprites] {
            let offset = snes_to_pc(layout.mapper, pointer.get())?;
            if offset >= ORIGINAL_AREA_LEN {
                return Err(DeleteLevelStreamsError::ReplacementOutsideOriginalArea {
                    pointer: pointer.get(),
                    offset,
                });
            }
        }

        let old_layer1 = layout.layer1.read_snes_pointer(&self.rom, level)?;
        let old_sprites = layout.sprites.read_snes_pointer(&self.rom, level)?;
        let old_layer1_offset = snes_to_pc(layout.mapper, old_layer1.get())?;
        if old_layer1_offset < ORIGINAL_AREA_LEN {
            return Err(DeleteLevelStreamsError::LevelNotExpanded {
                level,
                layer1_offset: old_layer1_offset,
            });
        }
        let candidates = [
            Some(old_layer1),
            (snes_to_pc(layout.mapper, old_sprites.get())? >= ORIGINAL_AREA_LEN)
                .then_some(old_sprites),
        ];

        let mut reclaimed = Vec::new();
        for pointer in candidates.into_iter().flatten() {
            let Some(block) = self.tagged_block_at(pointer, layout.mapper)? else {
                continue;
            };
            if reclaimed.contains(&block) {
                continue;
            }
            let pointer =
                SnesPointer24::new(lm_rom::pc_to_snes(layout.mapper, block.payload.start)?)
                    .map_err(|_| LevelLoadError::AddressOverflow)?;
            if !level_stream_pointer_is_shared(&self.rom, layout, level, pointer)? {
                reclaimed.push(block);
            }
        }
        reclaimed.sort_by_key(|block| block.header_offset);
        let reclaimed_bytes = reclaimed
            .iter()
            .try_fold(0_usize, |total, block| {
                total.checked_add(block.full_range().len())
            })
            .ok_or(DeleteLevelStreamsError::ReclaimedByteCountOverflow)?;

        let mut writes = pointer_writes(
            &self.rom,
            layout,
            level,
            replacement_layer1,
            replacement_sprites,
        )?;
        writes.extend(reclaimed.iter().map(|block| RomWrite {
            offset: block.header_offset,
            bytes: vec![erase_fill; block.full_range().len()],
        }));

        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        self.apply_writes(description, &writes)?;
        Ok(DeletedLevelStreams {
            level,
            reclaimed,
            reclaimed_bytes,
        })
    }

    /// Redirects every modeled native level asset to an original-area test-level slot.
    ///
    /// Layer 1, sprites, Layer 2, palette, ExAnimation, the optional Layer 2 descriptor, and the
    /// optional expanded-settings record are replaced together. Every displaced tagged payload is
    /// reclaimed only after all participating pointer tables prove that no other level retains it.
    /// The complete redirect, erasure set, and checksum are one undoable transaction.
    ///
    /// `replacement_level` identifies a slot already backed by Lunar Magic's original-area test
    /// data. Its Layer 1 and sprite pointers are authenticated to the original 512-KiB area before
    /// any write is prepared.
    ///
    /// # Errors
    ///
    /// Rejects invalid table shapes or slots, a replacement core stream outside the original
    /// area, malformed tagged ownership, unsafe shared-bank sprite replacement, checksum failures,
    /// and atomic transaction failures. Original-area targets are redirected without attempting
    /// payload reclamation; this is required by Lunar Magic's multi-level unmodified/all modes.
    pub fn delete_native_level_assets_to_original_source(
        &mut self,
        description: impl Into<String>,
        layout: NativeLevelAssetsLayout,
        layer2: Option<LevelLayer2RomLayout>,
        entrance: Option<VanillaEntranceRomLayout>,
        lfix3: Option<Lfix3LevelFieldsRomLayout>,
        level: usize,
        replacement_level: usize,
        checksum_field: usize,
        erase_fill: u8,
    ) -> Result<DeletedNativeLevelAssets, DeleteLevelStreamsError> {
        const ORIGINAL_AREA_LEN: usize = 0x80_000;
        if replacement_level >= layout.level.layer1.entries {
            return Err(DeleteLevelStreamsError::ReplacementLevelOutOfRange {
                level: replacement_level,
                entries: layout.level.layer1.entries,
            });
        }
        let replacement_layer1 = layout
            .level
            .layer1
            .read_snes_pointer(&self.rom, replacement_level)?;
        let replacement_sprites = layout
            .level
            .sprites
            .read_snes_pointer(&self.rom, replacement_level)?;
        for pointer in [replacement_layer1, replacement_sprites] {
            let offset = snes_to_pc(layout.level.mapper, pointer.get())?;
            if offset >= ORIGINAL_AREA_LEN {
                return Err(DeleteLevelStreamsError::ReplacementOutsideOriginalArea {
                    pointer: pointer.get(),
                    offset,
                });
            }
        }
        let old_layer1 = layout.level.layer1.read_snes_pointer(&self.rom, level)?;
        let contiguous = contiguous_asset_tables(layout, layer2);
        let mut candidates = Vec::new();
        candidates.push(old_layer1);
        candidates.push(layout.level.sprites.read_snes_pointer(&self.rom, level)?);
        for table in &contiguous[1..] {
            if let Some(pointer) = readable_pointer(&self.rom, *table, level)? {
                candidates.push(pointer);
            }
        }

        let mut reclaimed = Vec::new();
        for pointer in candidates {
            let Ok(offset) = snes_to_pc(layout.level.mapper, pointer.get()) else {
                continue;
            };
            if offset < ORIGINAL_AREA_LEN {
                continue;
            }
            let Some(block) = self.tagged_block_at(pointer, layout.level.mapper)? else {
                continue;
            };
            if reclaimed.contains(&block) {
                continue;
            }
            let block_pointer = SnesPointer24::new(lm_rom::pc_to_snes(
                layout.level.mapper,
                block.payload.start,
            )?)
            .map_err(|_| LevelLoadError::AddressOverflow)?;
            if !native_asset_pointer_is_shared(
                &self.rom,
                layout.level.sprites,
                &contiguous,
                level,
                block_pointer,
            )? {
                reclaimed.push(block);
            }
        }
        reclaimed.sort_by_key(|block| block.header_offset);
        let reclaimed_bytes = reclaimed
            .iter()
            .try_fold(0_usize, |total, block| {
                total.checked_add(block.full_range().len())
            })
            .ok_or(DeleteLevelStreamsError::ReclaimedByteCountOverflow)?;

        let mut writes = pointer_writes(
            &self.rom,
            layout.level,
            level,
            replacement_layer1,
            replacement_sprites,
        )?;
        for table in &contiguous[1..] {
            writes.push(copy_pointer_entry(
                &self.rom,
                *table,
                replacement_level,
                level,
            )?);
        }
        if let Some(layer2) = layer2
            && let Some(descriptors) = layer2.descriptor_table
        {
            writes.push(copy_direct_entry(
                &self.rom,
                descriptors.offset,
                descriptors.entries,
                descriptors.stride,
                1,
                replacement_level,
                level,
            )?);
        }
        if let Some(settings) = layout.expanded_settings {
            writes.push(copy_direct_entry(
                &self.rom,
                settings.table_offset,
                settings.entries,
                settings.stride,
                ExpandedLevelSettingsRecord::ENCODED_LEN,
                replacement_level,
                level,
            )?);
        }
        if let Some(entrance) = entrance {
            if entrance.entries <= level || entrance.entries <= replacement_level {
                return Err(DeleteLevelStreamsError::DirectTableLayout);
            }
            for offset in [
                entrance.position_offset,
                entrance.vertical_settings_offset,
                entrance.screen_and_method_offset,
                entrance.level_mode_and_screen_offset,
            ] {
                writes.push(copy_direct_entry(
                    &self.rom,
                    offset,
                    entrance.entries,
                    1,
                    1,
                    replacement_level,
                    level,
                )?);
            }
        }
        if let Some(lfix3) = lfix3 {
            if lfix3.entries <= level || lfix3.entries <= replacement_level {
                return Err(DeleteLevelStreamsError::DirectTableLayout);
            }
            for offset in [
                lfix3.flags_offset,
                lfix3.high_position_offset,
                lfix3.additional_flags_offset,
                lfix3.runtime_flags_offset,
            ] {
                writes.push(copy_direct_entry(
                    &self.rom,
                    offset,
                    lfix3.entries,
                    1,
                    1,
                    replacement_level,
                    level,
                )?);
            }
        }
        writes.extend(reclaimed.iter().map(|block| RomWrite {
            offset: block.header_offset,
            bytes: vec![erase_fill; block.full_range().len()],
        }));
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        self.apply_writes(description, &writes)?;
        Ok(DeletedNativeLevelAssets {
            level,
            reclaimed,
            reclaimed_bytes,
        })
    }

    fn tagged_block_at(
        &self,
        pointer: SnesPointer24,
        mapper: lm_rom::Mapper,
    ) -> Result<Option<RatsBlock>, DeleteLevelStreamsError> {
        match self.load_payload_from_pointer(pointer, mapper, &PayloadReadPolicy::Tagged) {
            Ok(payload) => Ok(payload.block),
            Err(PayloadLoadError::PointerNotTagged { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

fn rats_owner_bytes(payload_len: usize) -> Result<Vec<u8>, DeleteLevelStreamsError> {
    let encoded_len = payload_len
        .checked_sub(1)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(DeleteLevelStreamsError::DirectTableLayout)?;
    let complement = !encoded_len;
    let mut bytes = b"STAR".to_vec();
    bytes.extend_from_slice(&encoded_len.to_le_bytes());
    bytes.extend_from_slice(&complement.to_le_bytes());
    Ok(bytes)
}

fn contiguous_asset_tables(
    layout: NativeLevelAssetsLayout,
    layer2: Option<LevelLayer2RomLayout>,
) -> Vec<LevelPointerTable> {
    let mut tables = vec![
        layout.level.layer1,
        layout.palette.pointers,
        layout.exanimation.pointers,
    ];
    if let Some(layer2) = layer2 {
        tables.push(layer2.pointers);
    }
    tables
}

fn readable_pointer(
    rom: &lm_rom::RomImage,
    table: LevelPointerTable,
    level: usize,
) -> Result<Option<SnesPointer24>, DeleteLevelStreamsError> {
    let offset = table.pointer_offset(level)?;
    let bytes = rom.read(offset, 3)?;
    if bytes == [0, 0, 0] {
        return Ok(None);
    }
    Ok(SnesPointer24::decode(bytes).ok())
}

fn native_asset_pointer_is_shared(
    rom: &lm_rom::RomImage,
    sprites: SpritePointerTable,
    contiguous: &[LevelPointerTable],
    deleted_level: usize,
    pointer: SnesPointer24,
) -> Result<bool, DeleteLevelStreamsError> {
    for table in contiguous {
        for level in 0..table.entries {
            if level != deleted_level && readable_pointer(rom, *table, level)? == Some(pointer) {
                return Ok(true);
            }
        }
    }
    for level in 0..sprites.low_or_contiguous_table().entries {
        if level != deleted_level && sprites.read_snes_pointer(rom, level)? == pointer {
            return Ok(true);
        }
    }
    Ok(false)
}

fn copy_pointer_entry(
    rom: &lm_rom::RomImage,
    table: LevelPointerTable,
    source: usize,
    target: usize,
) -> Result<RomWrite, DeleteLevelStreamsError> {
    let source = table.pointer_offset(source)?;
    let target = table.pointer_offset(target)?;
    Ok(RomWrite {
        offset: target,
        bytes: rom.read(source, 3)?.to_vec(),
    })
}

fn copy_direct_entry(
    rom: &lm_rom::RomImage,
    offset: usize,
    entries: usize,
    stride: usize,
    width: usize,
    source: usize,
    target: usize,
) -> Result<RomWrite, DeleteLevelStreamsError> {
    if source >= entries || target >= entries || stride < width {
        return Err(DeleteLevelStreamsError::DirectTableLayout);
    }
    let source = offset
        .checked_add(
            source
                .checked_mul(stride)
                .ok_or(DeleteLevelStreamsError::DirectTableLayout)?,
        )
        .ok_or(DeleteLevelStreamsError::DirectTableLayout)?;
    let target = offset
        .checked_add(
            target
                .checked_mul(stride)
                .ok_or(DeleteLevelStreamsError::DirectTableLayout)?,
        )
        .ok_or(DeleteLevelStreamsError::DirectTableLayout)?;
    Ok(RomWrite {
        offset: target,
        bytes: rom.read(source, width)?.to_vec(),
    })
}

fn level_stream_pointer_is_shared(
    rom: &lm_rom::RomImage,
    layout: LevelRomLayout,
    deleted_level: usize,
    pointer: SnesPointer24,
) -> Result<bool, LevelLoadError> {
    for level in 0..layout.layer1.entries {
        if level != deleted_level && layout.layer1.read_snes_pointer(rom, level)? == pointer {
            return Ok(true);
        }
    }
    for level in 0..layout.sprites.low_or_contiguous_table().entries {
        if level != deleted_level && layout.sprites.read_snes_pointer(rom, level)? == pointer {
            return Ok(true);
        }
    }
    Ok(false)
}

fn pointer_writes(
    rom: &lm_rom::RomImage,
    layout: LevelRomLayout,
    level: usize,
    layer1: SnesPointer24,
    sprites: SnesPointer24,
) -> Result<Vec<RomWrite>, DeleteLevelStreamsError> {
    let mut writes = Vec::with_capacity(3);
    writes.push(RomWrite {
        offset: layout.layer1.pointer_offset(level)?,
        bytes: layer1.get().to_le_bytes()[..3].to_vec(),
    });
    let (low, bank) = layout.sprites.pointer_ranges(level)?;
    let encoded = sprites.get().to_le_bytes();
    writes.push(RomWrite {
        offset: low.start,
        bytes: encoded[..low.len()].to_vec(),
    });
    if let Some(bank) = bank {
        if matches!(layout.sprites, SpritePointerTable::SplitSharedBank { .. }) {
            let installed = rom.read(bank.start, 1)?[0];
            if installed != encoded[2] {
                return Err(DeleteLevelStreamsError::SharedSpriteBankMismatch {
                    installed,
                    replacement: encoded[2],
                });
            }
        } else {
            writes.push(RomWrite {
                offset: bank.start,
                bytes: vec![encoded[2]],
            });
        }
    }
    Ok(writes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExAnimationRomLayout, ExpandedLevelSettingsLayout, LevelLayer2DescriptorTable,
        LevelLayer2TilemapEncoding, LevelPointerTable, NativeLevelAssetsLayout, PaletteRomLayout,
        SpritePointerTable,
    };
    use lm_rats::{AllocationPolicy, FreeSpaceAllocator, parse_at};
    use lm_rom::{Mapper, RomImage, SnesChecksum, pc_to_snes};

    const CHECKSUM: usize = 0x7fdc;

    fn pointer(offset: usize) -> SnesPointer24 {
        SnesPointer24::new(pc_to_snes(Mapper::LoRom, offset).unwrap()).unwrap()
    }

    fn write_pointer(bytes: &mut [u8], offset: usize, pointer: SnesPointer24) {
        bytes[offset..offset + 3].copy_from_slice(&pointer.get().to_le_bytes()[..3]);
    }

    fn fixture(shared_layer1: bool) -> (Project, LevelRomLayout, RatsBlock, RatsBlock) {
        let mut bytes = vec![0xff; 0x10_0000];
        let (layer1, sprites) = {
            let mut allocator =
                FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x09_0000..0x0f_0000));
            (
                allocator.allocate(&[1, 2, 3, 4]).unwrap(),
                allocator.allocate(&[5, 6, 7]).unwrap(),
            )
        };
        let layer1_pointer = pointer(layer1.payload.start);
        let sprite_pointer = pointer(sprites.payload.start);
        let replacement_layer1 = pointer(0x4000);
        let replacement_sprites = pointer(0x5000);
        write_pointer(&mut bytes, 0x100, layer1_pointer);
        write_pointer(
            &mut bytes,
            0x103,
            if shared_layer1 {
                layer1_pointer
            } else {
                replacement_layer1
            },
        );
        bytes[0x200..0x202].copy_from_slice(&sprite_pointer.get().to_le_bytes()[..2]);
        bytes[0x202..0x204].copy_from_slice(&replacement_sprites.get().to_le_bytes()[..2]);
        bytes[0x210] = sprite_pointer.get().to_le_bytes()[2];
        bytes[0x211] = replacement_sprites.get().to_le_bytes()[2];
        let layout = LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: LevelPointerTable {
                offset: 0x100,
                entries: 2,
                stride: 3,
            },
            sprites: SpritePointerTable::SplitBankTable {
                low_words: LevelPointerTable {
                    offset: 0x200,
                    entries: 2,
                    stride: 2,
                },
                banks: LevelPointerTable {
                    offset: 0x210,
                    entries: 2,
                    stride: 1,
                },
            },
            expanded_sprites: true,
        };
        (
            Project::new(RomImage::from_bytes(bytes).unwrap()),
            layout,
            layer1,
            sprites,
        )
    }

    fn table(offset: usize) -> LevelPointerTable {
        LevelPointerTable {
            offset,
            entries: 2,
            stride: 3,
        }
    }

    #[test]
    fn deletion_redirects_reclaims_repairs_checksum_and_undoes_atomically() {
        let (mut project, layout, layer1, sprites) = fixture(false);
        let before = project.rom.logical_bytes().to_vec();
        let replacement_layer1 = pointer(0x4000);
        let replacement_sprites = pointer(0x5000);
        let result = project
            .delete_level_streams_to_original(
                "delete level 000",
                layout,
                0,
                replacement_layer1,
                replacement_sprites,
                CHECKSUM,
                0xff,
            )
            .unwrap();
        assert_eq!(result.reclaimed, [layer1.clone(), sprites.clone()]);
        assert_eq!(
            result.reclaimed_bytes,
            layer1.full_range().len() + sprites.full_range().len()
        );
        assert_eq!(
            layout.layer1.read_snes_pointer(&project.rom, 0).unwrap(),
            replacement_layer1
        );
        assert_eq!(
            layout.sprites.read_snes_pointer(&project.rom, 0).unwrap(),
            replacement_sprites
        );
        assert!(parse_at(project.rom.logical_bytes(), layer1.header_offset).is_err());
        assert!(parse_at(project.rom.logical_bytes(), sprites.header_offset).is_err());
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), CHECKSUM).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), CHECKSUM).unwrap()
        );
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), before);
    }

    #[test]
    fn deletion_retains_a_tagged_stream_still_referenced_by_another_level() {
        let (mut project, layout, layer1, sprites) = fixture(true);
        let result = project
            .delete_level_streams_to_original(
                "delete shared level",
                layout,
                0,
                pointer(0x4000),
                pointer(0x5000),
                CHECKSUM,
                0xff,
            )
            .unwrap();
        assert_eq!(result.reclaimed, [sprites]);
        assert_eq!(
            parse_at(project.rom.logical_bytes(), layer1.header_offset).unwrap(),
            layer1
        );
    }

    #[test]
    fn aggregate_deletion_redirects_every_modeled_domain_and_undoes_once() {
        let mut bytes = vec![0xff; 0x10_0000];
        let blocks = {
            let mut allocator =
                FreeSpaceAllocator::new(&mut bytes, AllocationPolicy::lorom(0x09_0000..0x0f_0000));
            (0..5)
                .map(|value| allocator.allocate(&[value; 4]).unwrap())
                .collect::<Vec<_>>()
        };
        let replacement_offsets = [0x4000, 0x5000, 0x6000, 0x7000, 0x8000];
        for ((table_offset, block), replacement) in [0x100, 0x300, 0x400, 0x500]
            .into_iter()
            .zip(&blocks)
            .zip(replacement_offsets)
        {
            write_pointer(&mut bytes, table_offset, pointer(block.payload.start));
            write_pointer(&mut bytes, table_offset + 3, pointer(replacement));
        }
        bytes[0x200..0x202]
            .copy_from_slice(&pointer(blocks[4].payload.start).get().to_le_bytes()[..2]);
        bytes[0x202..0x204]
            .copy_from_slice(&pointer(replacement_offsets[4]).get().to_le_bytes()[..2]);
        bytes[0x210] = pointer(blocks[4].payload.start).get().to_le_bytes()[2];
        bytes[0x211] = pointer(replacement_offsets[4]).get().to_le_bytes()[2];
        bytes[0x600] = 0xa5;
        bytes[0x601] = 0x5a;
        let settings_width = ExpandedLevelSettingsRecord::ENCODED_LEN;
        bytes[0x700..0x700 + settings_width].fill(0x11);
        bytes[0x700 + settings_width..0x700 + settings_width * 2].fill(0x22);

        let level = LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(0x100),
            sprites: SpritePointerTable::SplitBankTable {
                low_words: LevelPointerTable {
                    offset: 0x200,
                    entries: 2,
                    stride: 2,
                },
                banks: LevelPointerTable {
                    offset: 0x210,
                    entries: 2,
                    stride: 1,
                },
            },
            expanded_sprites: true,
        };
        let layout = NativeLevelAssetsLayout {
            level,
            palette: PaletteRomLayout {
                mapper: Mapper::LoRom,
                pointers: table(0x300),
                colors_per_palette: 257,
            },
            exanimation: ExAnimationRomLayout {
                mapper: Mapper::LoRom,
                pointers: table(0x400),
                maximum_records: 32,
                maximum_encoded_len: 0x100,
            },
            expanded_settings: Some(ExpandedLevelSettingsLayout {
                mapper: Mapper::LoRom,
                table_offset: 0x700,
                entries: 2,
                stride: settings_width,
            }),
        };
        let layer2 = LevelLayer2RomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x500),
            background_bank_substitution: None,
            legacy_pointer_redirect: None,
            descriptor_table: Some(LevelLayer2DescriptorTable {
                offset: 0x600,
                entries: 2,
                stride: 1,
            }),
            maximum_compressed_len: 0x8000,
            tilemap_encoding: LevelLayer2TilemapEncoding::SplitPlanes,
        };
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let before = project.rom.logical_bytes().to_vec();
        let result = project
            .delete_native_level_assets_to_original_source(
                "delete native level 000",
                layout,
                Some(layer2),
                None,
                None,
                0,
                1,
                CHECKSUM,
                0xff,
            )
            .unwrap();

        assert_eq!(result.reclaimed, blocks);
        assert_eq!(
            level.layer1.read_snes_pointer(&project.rom, 0).unwrap(),
            pointer(0x4000)
        );
        assert_eq!(
            level.sprites.read_snes_pointer(&project.rom, 0).unwrap(),
            pointer(0x8000)
        );
        for (table, replacement) in [table(0x300), table(0x400), table(0x500)]
            .into_iter()
            .zip([0x5000, 0x6000, 0x7000])
        {
            assert_eq!(
                table.read_snes_pointer(&project.rom, 0).unwrap(),
                pointer(replacement)
            );
        }
        assert_eq!(project.rom.read(0x600, 1).unwrap(), [0x5a]);
        assert_eq!(
            project.rom.read(0x700, settings_width).unwrap(),
            vec![0x22; settings_width]
        );
        for block in &result.reclaimed {
            assert!(parse_at(project.rom.logical_bytes(), block.header_offset).is_err());
        }
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), before);
    }

    #[test]
    fn original_level_area_clear_matches_the_recovered_gap_and_owner_layout() {
        let bytes = vec![0x5a; 0x10_0000];
        let protected_before =
            ORIGINAL_LEVEL_PROTECTED_BLOCKS.map(|(start, end)| bytes[start + 8..end].to_vec());
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        assert!(
            project
                .clear_original_level_data_area("clear original level area", CHECKSUM)
                .unwrap()
        );

        assert_eq!(project.rom.read(0x30253, 5).unwrap(), [0; 5]);
        let marker = parse_at(
            project.rom.logical_bytes(),
            ORIGINAL_LEVEL_CLEAR_MARKER_OFFSET,
        )
        .unwrap();
        assert_eq!(
            project
                .rom
                .read(marker.payload.start, marker.payload.len())
                .unwrap(),
            ORIGINAL_LEVEL_CLEAR_MARKER_PAYLOAD
        );
        let mut cursor = marker.full_range().end;
        for (index, &(start, end)) in ORIGINAL_LEVEL_PROTECTED_BLOCKS.iter().enumerate() {
            assert!(
                project
                    .rom
                    .read(cursor, start - cursor)
                    .unwrap()
                    .iter()
                    .all(|&byte| byte == 0)
            );
            let owner = parse_at(project.rom.logical_bytes(), start).unwrap();
            assert_eq!(owner.full_range(), start..end);
            assert_eq!(
                project
                    .rom
                    .read(owner.payload.start, owner.payload.len())
                    .unwrap(),
                protected_before[index]
            );
            cursor = end;
        }
        let metadata = project
            .rom
            .read(
                ORIGINAL_LEVEL_CLEAR_METADATA_OFFSET,
                ORIGINAL_LEVEL_CLEAR_METADATA_LEN,
            )
            .unwrap();
        assert_eq!(metadata[0], 0xaa);
        assert!(metadata[1..].iter().all(|&byte| byte == 0));
        assert_eq!(
            SnesChecksum::decode(project.rom.logical_bytes(), CHECKSUM).unwrap(),
            compute_snes_checksum(project.rom.logical_bytes(), CHECKSUM).unwrap()
        );
        assert_eq!(project.history.undo_len(), 1);

        let cleared = project.rom.logical_bytes().to_vec();
        assert!(
            !project
                .clear_original_level_data_area("clear again", CHECKSUM)
                .unwrap()
        );
        assert_eq!(project.history.undo_len(), 1);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), bytes);
        assert!(project.history.redo(&mut project.rom).unwrap());
        assert_eq!(project.rom.logical_bytes(), cleared);
    }
}
