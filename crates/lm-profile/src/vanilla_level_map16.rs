//! Pristine SMW level-Map16 source layout recovered from Lunar Magic's ROM descriptor.

use lm_rom::{Mapper, RomError, RomImage, snes_to_pc};
use std::fmt;

/// Low-word table for the tileset-specific packed Map16 source.
pub const SMW_US_V1_MAP16_TILESET_WORD_TABLE_OFFSET: usize = 0x28000;
/// Bank byte shared by both packed Map16 sources.
pub const SMW_US_V1_MAP16_SOURCE_BANK_OFFSET: usize = 0x28a3d;
/// Low word for the common packed Map16 source.
pub const SMW_US_V1_MAP16_COMMON_WORD_OFFSET: usize = 0x28222;
/// Fixed 512-bit source-selection mask copied by Lunar Magic when a ROM is opened.
pub const SMW_US_V1_MAP16_OCCUPANCY_MASK_OFFSET: usize = 0x281bb;
/// Size of the per-definition source-selection mask.
pub const SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES: usize = 64;
/// Number of eight-byte Map16 definitions composed by the recovered loader.
pub const SMW_US_V1_MAP16_BASE_TILE_COUNT: usize = SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES * 8;
/// Size of one four-subtile Map16 definition.
pub const SMW_US_V1_MAP16_TILE_BYTES: usize = 8;
/// Size of the composed base table.
pub const SMW_US_V1_MAP16_BASE_BYTES: usize =
    SMW_US_V1_MAP16_BASE_TILE_COUNT * SMW_US_V1_MAP16_TILE_BYTES;
/// Fixed ROM table behind vanilla background Map16 pages `$10` and `$11`.
pub const SMW_US_V1_MAP16_BACKGROUND_OFFSET: usize = 0x69100;
/// Size of the two 256-definition vanilla background pages.
pub const SMW_US_V1_MAP16_BACKGROUND_BYTES: usize = SMW_US_V1_MAP16_BASE_BYTES;
/// Size of Lunar Magic's widened four-byte-per-subtile graphics table.
pub const SMW_US_V1_MAP16_EDITOR_GRAPHICS_BYTES: usize =
    SMW_US_V1_MAP16_BASE_TILE_COUNT * 4 * size_of::<u32>();

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1LevelMap16Base {
    pub bytes: [u8; SMW_US_V1_MAP16_BASE_BYTES],
    pub tileset_source_offset: usize,
    pub common_source_offset: usize,
    pub tileset_tiles: usize,
    pub common_tiles: usize,
}

/// Loads the fixed two-page vanilla background Map16 definition table.
///
/// # Errors
///
/// Returns a ROM bounds error when the image does not contain the complete bank-$0D table.
pub fn load_smw_us_v1_background_map16(
    rom: &RomImage,
) -> Result<[u8; SMW_US_V1_MAP16_BACKGROUND_BYTES], RomError> {
    rom.read(
        SMW_US_V1_MAP16_BACKGROUND_OFFSET,
        SMW_US_V1_MAP16_BACKGROUND_BYTES,
    )?
    .try_into()
    .map_err(|_| RomError::RangeOutOfBounds {
        offset: SMW_US_V1_MAP16_BACKGROUND_OFFSET,
        len: SMW_US_V1_MAP16_BACKGROUND_BYTES,
        image_len: rom.logical_len(),
    })
}

impl LoadedSmwUsV1LevelMap16Base {
    /// Widens SMW's SNES tilemap words into Lunar Magic's internal graphics descriptor ordering.
    ///
    /// The recovered loader stores each result in a 32-bit cell. Bits 16-19 carry attributes and
    /// must not be truncated by treating this as another SNES tilemap-word table.
    #[must_use]
    pub fn editor_graphics_bytes(&self) -> [u8; SMW_US_V1_MAP16_EDITOR_GRAPHICS_BYTES] {
        let mut converted = [0; SMW_US_V1_MAP16_EDITOR_GRAPHICS_BYTES];
        for (source, target) in self
            .bytes
            .chunks_exact(2)
            .zip(converted.chunks_exact_mut(4))
        {
            let word = u16::from_le_bytes([source[0], source[1]]);
            let word = u32::from(word);
            let editor_word = (word >> 2 & 0x3c00) | (word & 0xfc00) << 4 | word & 0x03ff;
            target.copy_from_slice(&editor_word.to_le_bytes());
        }
        converted
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1LevelMap16BaseError {
    TilesetOutOfRange(usize),
    Rom(RomError),
}

impl fmt::Display for SmwUsV1LevelMap16BaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "pristine level Map16 base load failed: {self:?}")
    }
}

impl std::error::Error for SmwUsV1LevelMap16BaseError {}

impl From<RomError> for SmwUsV1LevelMap16BaseError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Composes Lunar Magic's 512-entry base table from the two compact ROM streams.
///
/// The fixed occupancy mask is consumed most-significant-bit first. A set bit selects the common
/// stream and a clear bit selects the tileset-specific stream. Each compact source advances only
/// when selected, matching `LoadMap16BaseDataFromRom` rather than treating either source as a
/// sparse 512-entry table.
///
/// # Errors
///
/// Rejects tileset indices that cannot address the recovered 16-entry word table, invalid SNES
/// pointers, and truncated compact sources.
pub fn load_smw_us_v1_level_map16_base(
    rom: &RomImage,
    tileset: usize,
) -> Result<LoadedSmwUsV1LevelMap16Base, SmwUsV1LevelMap16BaseError> {
    const TILESET_COUNT: usize = 16;
    if tileset >= TILESET_COUNT {
        return Err(SmwUsV1LevelMap16BaseError::TilesetOutOfRange(tileset));
    }
    let bytes = rom.logical_bytes();
    let bank = read_byte(bytes, SMW_US_V1_MAP16_SOURCE_BANK_OFFSET)?;
    let tileset_word = read_word(
        bytes,
        SMW_US_V1_MAP16_TILESET_WORD_TABLE_OFFSET + tileset * 2,
    )?;
    let common_word = read_word(bytes, SMW_US_V1_MAP16_COMMON_WORD_OFFSET)?;
    let mut occupancy_mask = [0; SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES];
    occupancy_mask.copy_from_slice(source(
        bytes,
        SMW_US_V1_MAP16_OCCUPANCY_MASK_OFFSET,
        SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES,
    )?);
    let tileset_source_offset = source_offset(bank, tileset_word)?;
    let common_source_offset = source_offset(bank, common_word)?;

    let common_tiles = occupancy_mask
        .iter()
        .map(|byte| byte.count_ones() as usize)
        .sum();
    let tileset_tiles = SMW_US_V1_MAP16_BASE_TILE_COUNT - common_tiles;
    let tileset_bytes = source(
        bytes,
        tileset_source_offset,
        tileset_tiles * SMW_US_V1_MAP16_TILE_BYTES,
    )?;
    let common_bytes = source(
        bytes,
        common_source_offset,
        common_tiles * SMW_US_V1_MAP16_TILE_BYTES,
    )?;

    let mut composed = [0; SMW_US_V1_MAP16_BASE_BYTES];
    let mut tileset_cursor = 0;
    let mut common_cursor = 0;
    for tile in 0..SMW_US_V1_MAP16_BASE_TILE_COUNT {
        let use_common = occupancy_mask[tile / 8] & (0x80 >> (tile % 8)) != 0;
        let source_cursor = if use_common {
            &mut common_cursor
        } else {
            &mut tileset_cursor
        };
        let source = if use_common {
            common_bytes
        } else {
            tileset_bytes
        };
        let target_start = tile * SMW_US_V1_MAP16_TILE_BYTES;
        composed[target_start..target_start + SMW_US_V1_MAP16_TILE_BYTES]
            .copy_from_slice(&source[*source_cursor..*source_cursor + SMW_US_V1_MAP16_TILE_BYTES]);
        *source_cursor += SMW_US_V1_MAP16_TILE_BYTES;
    }
    Ok(LoadedSmwUsV1LevelMap16Base {
        bytes: composed,
        tileset_source_offset,
        common_source_offset,
        tileset_tiles,
        common_tiles,
    })
}

fn read_byte(bytes: &[u8], offset: usize) -> Result<u8, RomError> {
    bytes
        .get(offset)
        .copied()
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: 1,
            image_len: bytes.len(),
        })
}

fn read_word(bytes: &[u8], offset: usize) -> Result<u16, RomError> {
    let pair = source(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

fn source(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], RomError> {
    bytes
        .get(offset..offset.saturating_add(len))
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len,
            image_len: bytes.len(),
        })
}

fn source_offset(bank: u8, word: u16) -> Result<usize, RomError> {
    snes_to_pc(Mapper::LoRom, (u32::from(bank) << 16) | u32::from(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::pc_to_snes;
    use std::{fs, path::PathBuf};

    fn fixture() -> RomImage {
        let mut rom = RomImage::from_bytes(vec![0xff; 0x70_000]).unwrap();
        let tileset_source = 0x68b70;
        let common_source = 0x68000;
        let tileset_snes = pc_to_snes(Mapper::LoRom, tileset_source).unwrap();
        let common_snes = pc_to_snes(Mapper::LoRom, common_source).unwrap();
        rom.write(
            SMW_US_V1_MAP16_TILESET_WORD_TABLE_OFFSET,
            &u16::try_from(tileset_snes & 0xffff).unwrap().to_le_bytes(),
        )
        .unwrap();
        rom.write(
            SMW_US_V1_MAP16_COMMON_WORD_OFFSET,
            &u16::try_from(common_snes & 0xffff).unwrap().to_le_bytes(),
        )
        .unwrap();
        rom.write(
            SMW_US_V1_MAP16_SOURCE_BANK_OFFSET,
            &[u8::try_from(tileset_snes >> 16).unwrap()],
        )
        .unwrap();
        let tileset = (0_u16..256)
            .flat_map(|tile| [u8::try_from(tile).unwrap(); 8])
            .collect::<Vec<_>>();
        let common = (0_u16..256)
            .map(|tile| u8::try_from(tile).unwrap())
            .flat_map(|tile| [tile; 8])
            .collect::<Vec<_>>();
        rom.write(tileset_source, &tileset).unwrap();
        rom.write(common_source, &common).unwrap();
        rom
    }

    #[test]
    fn interleaves_compact_sources_msb_first() {
        let mut fixture = fixture();
        fixture
            .write(
                SMW_US_V1_MAP16_OCCUPANCY_MASK_OFFSET,
                &[0xaa; SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES],
            )
            .unwrap();
        let loaded = load_smw_us_v1_level_map16_base(&fixture, 0).unwrap();
        assert_eq!(loaded.tileset_tiles, 256);
        assert_eq!(loaded.common_tiles, 256);
        for tile in 0..512 {
            let expected = u8::try_from(tile / 2).unwrap();
            assert_eq!(
                &loaded.bytes[tile * 8..tile * 8 + 8],
                &[expected; 8],
                "tile {tile}"
            );
        }
    }

    #[test]
    fn rejects_unknown_tilesets() {
        assert_eq!(
            load_smw_us_v1_level_map16_base(&fixture(), 16),
            Err(SmwUsV1LevelMap16BaseError::TilesetOutOfRange(16))
        );
    }

    #[test]
    fn converts_rom_attributes_to_editor_subtile_ordering() {
        let loaded = LoadedSmwUsV1LevelMap16Base {
            bytes: {
                let mut bytes = [0; SMW_US_V1_MAP16_BASE_BYTES];
                bytes[..8].copy_from_slice(&[0x70, 0x1c, 0xf8, 0x89, 0x00, 0x0c, 0x00, 0x14]);
                bytes
            },
            tileset_source_offset: 0,
            common_source_offset: 0,
            tileset_tiles: 0,
            common_tiles: 512,
        };
        assert_eq!(
            &loaded.editor_graphics_bytes()[..16],
            &[
                0x70, 0xc4, 0x01, 0x00, 0xf8, 0xa1, 0x08, 0x00, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x44,
                0x01, 0x00,
            ]
        );
    }

    #[test]
    fn tileset_seven_matches_lunar_magic_complete_map16_export() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let rom_bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let Ok(exported_bytes) =
            fs::read(root.join("oracle-work/lm363/pristine-us/map16/all.map16"))
        else {
            return;
        };
        let rom = RomImage::from_bytes(rom_bytes).unwrap();
        let actual = load_smw_us_v1_level_map16_base(&rom, 7).unwrap();
        let exported = lm_level::Lm16Map16File::decode(&exported_bytes).unwrap();
        let auxiliary = exported.section(lm_level::Lm16Map16SectionKind::AuxiliaryTiles);
        let expected = &auxiliary[7 * SMW_US_V1_MAP16_BASE_BYTES..8 * SMW_US_V1_MAP16_BASE_BYTES];
        let differences = actual
            .bytes
            .iter()
            .zip(expected)
            .enumerate()
            .filter_map(|(index, (actual, expected))| {
                (actual != expected).then_some((index, *actual, *expected))
            })
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "Map16 byte differences: {:02X?}",
            &differences[..differences.len().min(32)]
        );
    }
}
