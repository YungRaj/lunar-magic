use crate::{PayloadLoadError, PayloadReadPolicy, Project};
use lm_level::{
    LevelObjectData, NativeSpriteStream, ObjectStreamError, SpriteLengthTable, SpriteStreamError,
};
use lm_rom::{Mapper, SnesPointer24};
use std::fmt;
use std::ops::Range;

/// Location and shape of a 24-bit level pointer table in the logical ROM image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelPointerTable {
    pub offset: usize,
    pub entries: usize,
    pub stride: usize,
}

impl LevelPointerTable {
    /// Returns the ROM offset of `level`'s three-byte pointer.
    ///
    /// # Errors
    ///
    /// Returns [`LevelLoadError::LevelOutOfRange`] or [`LevelLoadError::InvalidPointerStride`].
    pub fn pointer_offset(self, level: usize) -> Result<usize, LevelLoadError> {
        if level >= self.entries {
            return Err(LevelLoadError::LevelOutOfRange {
                level,
                entries: self.entries,
            });
        }
        if self.stride < 3 {
            return Err(LevelLoadError::InvalidPointerStride(self.stride));
        }
        self.offset
            .checked_add(
                level
                    .checked_mul(self.stride)
                    .ok_or(LevelLoadError::AddressOverflow)?,
            )
            .ok_or(LevelLoadError::AddressOverflow)
    }

    /// Reads one contiguous 24-bit SNES pointer from this table.
    ///
    /// # Errors
    ///
    /// Rejects an invalid table shape, an out-of-range slot, or an out-of-bounds ROM read.
    pub fn read_snes_pointer(
        self,
        rom: &lm_rom::RomImage,
        level: usize,
    ) -> Result<SnesPointer24, LevelLoadError> {
        let offset = self.pointer_offset(level)?;
        let bytes = rom
            .read(offset, SnesPointer24::ENCODED_LEN)
            .map_err(PayloadLoadError::Rom)?;
        SnesPointer24::decode(bytes).map_err(|_| LevelLoadError::AddressOverflow)
    }

    pub(crate) fn pointer_offset_16(self, level: usize) -> Result<usize, LevelLoadError> {
        component_offset(self, level, 2)
    }

    pub(crate) fn pointer_offset_8(self, level: usize) -> Result<usize, LevelLoadError> {
        component_offset(self, level, 1)
    }
}

/// ROM representation of the sprite-stream pointers for all level slots.
///
/// Vanilla and Lunar Magic-installed ROMs commonly store the low words contiguously and keep
/// their bank byte either in one shared location or in a parallel byte table. Other revisions use
/// ordinary contiguous 24-bit pointers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpritePointerTable {
    Contiguous(LevelPointerTable),
    SplitSharedBank {
        low_words: LevelPointerTable,
        bank_offset: usize,
    },
    SplitBankTable {
        low_words: LevelPointerTable,
        banks: LevelPointerTable,
    },
}

impl From<LevelPointerTable> for SpritePointerTable {
    fn from(value: LevelPointerTable) -> Self {
        Self::Contiguous(value)
    }
}

impl SpritePointerTable {
    #[must_use]
    pub const fn low_or_contiguous_table(self) -> LevelPointerTable {
        match self {
            Self::Contiguous(table) => table,
            Self::SplitSharedBank { low_words, .. } | Self::SplitBankTable { low_words, .. } => {
                low_words
            }
        }
    }

    /// Returns the ROM ranges containing one encoded pointer.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range level, invalid component stride, or address overflow.
    pub fn pointer_ranges(
        self,
        level: usize,
    ) -> Result<(Range<usize>, Option<Range<usize>>), LevelLoadError> {
        match self {
            Self::Contiguous(table) => {
                let offset = table.pointer_offset(level)?;
                Ok((offset..offset + 3, None))
            }
            Self::SplitSharedBank {
                low_words,
                bank_offset,
            } => {
                let offset = component_offset(low_words, level, 2)?;
                Ok((offset..offset + 2, Some(bank_offset..bank_offset + 1)))
            }
            Self::SplitBankTable { low_words, banks } => {
                let low = component_offset(low_words, level, 2)?;
                let bank = component_offset(banks, level, 1)?;
                Ok((low..low + 2, Some(bank..bank + 1)))
            }
        }
    }

    /// Reads and combines one encoded 24-bit SNES pointer.
    ///
    /// # Errors
    ///
    /// Rejects an invalid layout, an out-of-bounds ROM read, or an overflowing pointer.
    pub fn read_snes_pointer(
        self,
        rom: &lm_rom::RomImage,
        level: usize,
    ) -> Result<SnesPointer24, LevelLoadError> {
        let (low, bank) = self.pointer_ranges(level)?;
        let low_bytes = rom
            .read(low.start, low.len())
            .map_err(PayloadLoadError::Rom)?;
        let mut pointer = u32::from(low_bytes[0]) | (u32::from(low_bytes[1]) << 8);
        if let Some(bank) = bank {
            pointer |= u32::from(rom.read(bank.start, 1).map_err(PayloadLoadError::Rom)?[0]) << 16;
        } else {
            pointer |= u32::from(low_bytes[2]) << 16;
        }
        SnesPointer24::new(pointer).map_err(|_| LevelLoadError::AddressOverflow)
    }
}

fn component_offset(
    table: LevelPointerTable,
    level: usize,
    width: usize,
) -> Result<usize, LevelLoadError> {
    if level >= table.entries {
        return Err(LevelLoadError::LevelOutOfRange {
            level,
            entries: table.entries,
        });
    }
    if table.stride < width {
        return Err(LevelLoadError::InvalidPointerStride(table.stride));
    }
    table
        .offset
        .checked_add(
            level
                .checked_mul(table.stride)
                .ok_or(LevelLoadError::AddressOverflow)?,
        )
        .ok_or(LevelLoadError::AddressOverflow)
}

/// ROM-version-specific tables and stream variants needed to load one level slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelRomLayout {
    pub mapper: Mapper,
    pub layer1: LevelPointerTable,
    pub sprites: SpritePointerTable,
    pub expanded_sprites: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedLevelSlot {
    pub number: usize,
    pub layer1: LevelObjectData,
    pub sprites: NativeSpriteStream,
}

#[derive(Debug)]
pub enum LevelLoadError {
    LevelOutOfRange { level: usize, entries: usize },
    InvalidPointerStride(usize),
    AddressOverflow,
    Payload(PayloadLoadError),
    Objects(ObjectStreamError),
    Sprites(SpriteStreamError),
}

impl fmt::Display for LevelLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "level load failed: {self:?}")
    }
}

impl std::error::Error for LevelLoadError {}

impl From<PayloadLoadError> for LevelLoadError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Payload(value)
    }
}

impl From<ObjectStreamError> for LevelLoadError {
    fn from(value: ObjectStreamError) -> Self {
        Self::Objects(value)
    }
}

impl From<SpriteStreamError> for LevelLoadError {
    fn from(value: SpriteStreamError) -> Self {
        Self::Sprites(value)
    }
}

impl Project {
    /// Loads and parses one native level/object stream and its sprite stream.
    ///
    /// Both vanilla untagged streams and relocated RATS-tagged streams are accepted. Reads are
    /// capped to one `LoROM` data bank, matching the native stream storage constraint.
    ///
    /// # Errors
    ///
    /// Returns [`LevelLoadError`] for invalid table layouts, pointers, delimiters, or streams.
    pub fn load_level_slot(
        &self,
        number: usize,
        layout: LevelRomLayout,
        sprite_lengths: &SpriteLengthTable,
    ) -> Result<LoadedLevelSlot, LevelLoadError> {
        let layer1_pointer = layout.layer1.pointer_offset(number)?;
        let sprite_snes = layout.sprites.read_snes_pointer(&self.rom, number)?;
        let layer1 = self.load_payload(
            layer1_pointer,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?;
        let sprites = self.load_payload_from_pointer(
            sprite_snes,
            layout.mapper,
            &PayloadReadPolicy::TaggedOrBounded {
                maximum_len: 0x8000,
                bank_size: Some(0x8000),
            },
        )?;
        let expanded_sprites = sprites
            .bytes
            .first()
            .map_or(layout.expanded_sprites, |header| {
                NativeSpriteStream::header_uses_expanded_framing(*header)
            });
        Ok(LoadedLevelSlot {
            number,
            layer1: LevelObjectData::parse(&layer1.bytes)?,
            sprites: NativeSpriteStream::parse(&sprites.bytes, expanded_sprites, sprite_lengths)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::RomImage;

    #[test]
    fn loads_a_vanilla_untagged_level_slot() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x30..0x33].copy_from_slice(&[0x20, 0x81, 0x80]);
        bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
        bytes[0x120..0x125].copy_from_slice(&[0x10, 0x00, 0x20, 0x01, 0xff]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let loaded = project
            .load_level_slot(
                0,
                LevelRomLayout {
                    mapper: Mapper::LoRom,
                    layer1: LevelPointerTable {
                        offset: 0x20,
                        entries: 1,
                        stride: 3,
                    },
                    sprites: LevelPointerTable {
                        offset: 0x30,
                        entries: 1,
                        stride: 3,
                    }
                    .into(),
                    expanded_sprites: false,
                },
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert_eq!(loaded.number, 0);
        assert_eq!(
            loaded.layer1.encode().unwrap(),
            [1, 2, 3, 4, 5, 9, 8, 7, 0xff]
        );
        assert_eq!(
            loaded.sprites.encode_checked().unwrap(),
            [0x10, 0x00, 0x20, 0x01, 0xff]
        );
    }

    #[test]
    fn rejects_a_level_outside_the_declared_tables() {
        let project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        let table = LevelPointerTable {
            offset: 0,
            entries: 1,
            stride: 3,
        };
        assert!(matches!(
            project.load_level_slot(
                1,
                LevelRomLayout {
                    mapper: Mapper::LoRom,
                    layer1: table,
                    sprites: table.into(),
                    expanded_sprites: false,
                },
                &SpriteLengthTable::standard()
            ),
            Err(LevelLoadError::LevelOutOfRange { level: 1, .. })
        ));
    }

    #[test]
    fn loads_a_split_shared_bank_sprite_pointer() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x30..0x32].copy_from_slice(&[0x20, 0x81]);
        bytes[0x40] = 0x80;
        bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
        bytes[0x120..0x125].copy_from_slice(&[0x10, 0x00, 0x20, 0x01, 0xff]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let loaded = project
            .load_level_slot(
                0,
                LevelRomLayout {
                    mapper: Mapper::LoRom,
                    layer1: LevelPointerTable {
                        offset: 0x20,
                        entries: 1,
                        stride: 3,
                    },
                    sprites: SpritePointerTable::SplitSharedBank {
                        low_words: LevelPointerTable {
                            offset: 0x30,
                            entries: 1,
                            stride: 2,
                        },
                        bank_offset: 0x40,
                    },
                    expanded_sprites: false,
                },
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        assert_eq!(
            loaded.sprites.encode_checked().unwrap(),
            [0x10, 0x00, 0x20, 0x01, 0xff]
        );
    }

    #[test]
    fn sprite_header_selects_framing_despite_stale_layout_metadata() {
        let mut bytes = vec![0; 0x8000];
        bytes[0x20..0x23].copy_from_slice(&[0x00, 0x81, 0x80]);
        bytes[0x30..0x33].copy_from_slice(&[0x20, 0x81, 0x80]);
        bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]);
        bytes[0x120..0x128].copy_from_slice(&[0x30, 0xff, 1, 0x00, 0x20, 0x01, 0xff, 0xfe]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let table = LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: LevelPointerTable {
                offset: 0x20,
                entries: 1,
                stride: 3,
            },
            sprites: LevelPointerTable {
                offset: 0x30,
                entries: 1,
                stride: 3,
            }
            .into(),
            expanded_sprites: false,
        };
        let expanded = project
            .load_level_slot(0, table, &SpriteLengthTable::standard())
            .unwrap();
        assert!(expanded.sprites.expanded);
        assert_eq!(expanded.sprites.tokens[0], lm_level::SpriteToken::Screen(1));

        let mut bytes = project.rom.logical_bytes().to_vec();
        bytes[0x120..0x125].copy_from_slice(&[0x10, 0x00, 0x20, 0x01, 0xff]);
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let mut stale_expanded = table;
        stale_expanded.expanded_sprites = true;
        let legacy = project
            .load_level_slot(0, stale_expanded, &SpriteLengthTable::standard())
            .unwrap();
        assert!(!legacy.sprites.expanded);
    }
}
