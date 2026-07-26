//! Recovered native layouts used by an unmodified North American SMW revision 0 ROM.

use lm_project::{
    GraphicsCompression, GraphicsPointerPlanes, GraphicsRomLayout, LevelPointerTable,
    LevelRomLayout, SpritePointerTable, VanillaEntranceRomLayout,
};
use lm_rom::Mapper;
use lm_rom::{RomError, RomImage};
use std::fmt;

/// Number of ordinary vanilla graphics files addressed by the parallel pointer planes.
pub const SMW_US_V1_VANILLA_GRAPHICS_FILES: usize = 0x32;

/// Low-byte plane for the vanilla graphics pointers, in headerless PC coordinates.
pub const SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET: usize = 0x3992;
/// High-byte plane for the vanilla graphics pointers, in headerless PC coordinates.
pub const SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET: usize = 0x39c4;
/// Bank-byte plane for the vanilla graphics pointers, in headerless PC coordinates.
pub const SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET: usize = 0x39f6;

/// Number of native level slots in SMW's primary and secondary level ranges.
pub const SMW_US_V1_VANILLA_LEVEL_SLOTS: usize = 0x200;
/// Contiguous 24-bit Layer 1 object-stream pointer table.
pub const SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET: usize = 0x2e000;
/// Parallel low-word table for native sprite-stream pointers.
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET: usize = 0x2ec00;
/// Shared bank operand for native sprite-stream pointers.
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET: usize = 0x2d8f6;
/// Four 512-byte main-entrance planes, proven against Lunar Magic's complete MWL export corpus.
pub const SMW_US_V1_ENTRANCE_POSITION_OFFSET: usize = 0x2f000;
pub const SMW_US_V1_ENTRANCE_VERTICAL_SETTINGS_OFFSET: usize = 0x2f200;
pub const SMW_US_V1_ENTRANCE_SCREEN_AND_METHOD_OFFSET: usize = 0x2f400;
pub const SMW_US_V1_ENTRANCE_LEVEL_MODE_AND_SCREEN_OFFSET: usize = 0x2f600;
/// Four-byte graphics-file assignment rows for the 16 native object tilesets.
pub const SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET: usize = 0x292b;
pub const SMW_US_V1_OBJECT_TILESETS: usize = 16;
pub const SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS: usize = 4;
/// Four-byte sprite GFX assignment rows used by the 32 native sprite tilesets.
///
/// Lunar Magic copies this pristine table into its `g_abSpriteGraphicsSets` working array before
/// resolving the four SP slots. The table begins at headerless PC `$0028C3`.
pub const SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET: usize = 0x28c3;
pub const SMW_US_V1_SPRITE_TILESETS: usize = 32;
pub const SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1ObjectTilesetGraphicsError {
    TilesetOutOfRange(usize),
    Rom(RomError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1SpriteTilesetGraphicsError {
    TilesetOutOfRange(usize),
    Rom(RomError),
}

impl fmt::Display for SmwUsV1SpriteTilesetGraphicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "pristine sprite-tileset graphics lookup failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1SpriteTilesetGraphicsError {}

impl From<RomError> for SmwUsV1SpriteTilesetGraphicsError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl fmt::Display for SmwUsV1ObjectTilesetGraphicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "pristine object-tileset graphics lookup failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1ObjectTilesetGraphicsError {}

impl From<RomError> for SmwUsV1ObjectTilesetGraphicsError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Reads the four GFX file numbers selected by one native object tileset.
///
/// # Errors
///
/// Rejects tileset indices above 15 and truncated assignment tables.
pub fn smw_us_v1_object_tileset_graphics_files(
    rom: &RomImage,
    tileset: usize,
) -> Result<[usize; SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS], SmwUsV1ObjectTilesetGraphicsError> {
    if tileset >= SMW_US_V1_OBJECT_TILESETS {
        return Err(SmwUsV1ObjectTilesetGraphicsError::TilesetOutOfRange(
            tileset,
        ));
    }
    let offset = SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET
        + tileset * SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS;
    let bytes = rom
        .logical_bytes()
        .get(offset..offset + SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS,
            image_len: rom.logical_len(),
        })?;
    Ok(std::array::from_fn(|index| usize::from(bytes[index])))
}

/// Reads the four SP graphics-file numbers selected by one native sprite tileset.
///
/// # Errors
///
/// Rejects tileset indices above 31 and truncated assignment tables.
pub fn smw_us_v1_sprite_tileset_graphics_files(
    rom: &RomImage,
    tileset: usize,
) -> Result<[usize; SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS], SmwUsV1SpriteTilesetGraphicsError> {
    if tileset >= SMW_US_V1_SPRITE_TILESETS {
        return Err(SmwUsV1SpriteTilesetGraphicsError::TilesetOutOfRange(
            tileset,
        ));
    }
    let offset = SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET
        + tileset * SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS;
    let bytes = rom
        .logical_bytes()
        .get(offset..offset + SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS)
        .ok_or(RomError::RangeOutOfBounds {
            offset,
            len: SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS,
            image_len: rom.logical_len(),
        })?;
    Ok(std::array::from_fn(|index| usize::from(bytes[index])))
}

/// Returns the native graphics layout recovered from Lunar Magic's SMW-US descriptor and
/// `ReadGraphicsFileRomPointer`.
#[must_use]
pub const fn smw_us_v1_vanilla_graphics_layout() -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET,
            entries: SMW_US_V1_VANILLA_GRAPHICS_FILES,
            stride: 1,
        },
        split_pointer_planes: Some(GraphicsPointerPlanes {
            low_offset: SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET,
            high_offset: SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET,
            bank_offset: SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET,
            entries: SMW_US_V1_VANILLA_GRAPHICS_FILES,
            stride: 1,
        }),
        compression: GraphicsCompression::Lz2,
        maximum_compressed_len: 0x8000,
        maximum_decompressed_len: 0x10000,
    }
}

/// Returns the native level layout used by an unmodified SMW-US revision 0 ROM.
#[must_use]
pub const fn smw_us_v1_vanilla_level_layout() -> LevelRomLayout {
    LevelRomLayout {
        mapper: Mapper::LoRom,
        layer1: LevelPointerTable {
            offset: SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET,
            entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
            stride: 3,
        },
        sprites: SpritePointerTable::SplitSharedBank {
            low_words: LevelPointerTable {
                offset: SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 2,
            },
            bank_offset: SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET,
        },
        expanded_sprites: false,
    }
}

/// Returns the four-plane main-entrance layout used by an unmodified SMW-US revision 0 ROM.
#[must_use]
pub const fn smw_us_v1_vanilla_entrance_layout() -> VanillaEntranceRomLayout {
    VanillaEntranceRomLayout {
        mapper: Mapper::LoRom,
        position_offset: SMW_US_V1_ENTRANCE_POSITION_OFFSET,
        vertical_settings_offset: SMW_US_V1_ENTRANCE_VERTICAL_SETTINGS_OFFSET,
        screen_and_method_offset: SMW_US_V1_ENTRANCE_SCREEN_AND_METHOD_OFFSET,
        level_mode_and_screen_offset: SMW_US_V1_ENTRANCE_LEVEL_MODE_AND_SCREEN_OFFSET,
        entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_tileset_assignments_are_bounded_and_ordered() {
        let mut bytes = vec![0xff; 0x30_000];
        bytes[SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET
            ..SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET + 8]
            .copy_from_slice(&[0x14, 0x17, 0x19, 0x15, 0x14, 0x17, 0x1b, 0x18]);
        let rom = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            smw_us_v1_object_tileset_graphics_files(&rom, 0).unwrap(),
            [0x14, 0x17, 0x19, 0x15]
        );
        assert_eq!(
            smw_us_v1_object_tileset_graphics_files(&rom, 1).unwrap(),
            [0x14, 0x17, 0x1b, 0x18]
        );
        assert_eq!(
            smw_us_v1_object_tileset_graphics_files(&rom, 16),
            Err(SmwUsV1ObjectTilesetGraphicsError::TilesetOutOfRange(16))
        );
    }

    #[test]
    fn sprite_tileset_assignments_are_bounded_and_ordered() {
        let mut bytes = vec![0xff; 0x30_000];
        bytes[SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET
            ..SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET + 8]
            .copy_from_slice(&[0x00, 0x01, 0x13, 0x02, 0x00, 0x01, 0x12, 0x03]);
        let rom = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            smw_us_v1_sprite_tileset_graphics_files(&rom, 0).unwrap(),
            [0x00, 0x01, 0x13, 0x02]
        );
        assert_eq!(
            smw_us_v1_sprite_tileset_graphics_files(&rom, 1).unwrap(),
            [0x00, 0x01, 0x12, 0x03]
        );
        assert_eq!(
            smw_us_v1_sprite_tileset_graphics_files(&rom, 32),
            Err(SmwUsV1SpriteTilesetGraphicsError::TilesetOutOfRange(32))
        );
    }
}
