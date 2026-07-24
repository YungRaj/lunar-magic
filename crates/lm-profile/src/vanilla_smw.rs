//! Recovered native layouts used by an unmodified North American SMW revision 0 ROM.

use lm_project::{
    GraphicsCompression, GraphicsPointerPlanes, GraphicsRomLayout, LevelPointerTable,
    LevelRomLayout, SpritePointerTable,
};
use lm_rom::Mapper;

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
