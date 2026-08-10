use crate::{
    LevelLoadError, LevelRomLayout, PayloadLoadError, PayloadReadPolicy, Project, RomWrite,
    SpritePointerTable,
};
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
    use crate::{LevelPointerTable, SpritePointerTable};
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
}
