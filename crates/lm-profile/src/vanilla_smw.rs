//! Recovered native layouts used by an unmodified North American SMW revision 0 ROM.

use lm_project::{
    ExpandedLevelModeLocator, GraphicsCompression, GraphicsPointerPlanes, GraphicsRomLayout,
    LevelLayer2DescriptorTable, LevelLayer2PointerRedirect, LevelLayer2RomLayout,
    LevelLayer2TilemapEncoding, LevelPointerTable, LevelRomLayout, Lfix3LevelFieldsRomLayout,
    SeparateMidwayPatchLocator, SpritePointerTable, VanillaEntranceRomLayout,
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
/// Contiguous pointers for GFX33 animated tiles followed by GFX32 player graphics.
pub const SMW_US_V1_SPECIAL_GRAPHICS_POINTER_OFFSET: usize = 0x3882;
pub const SMW_US_V1_SPECIAL_GRAPHICS_FILES: usize = 2;

/// Number of native level slots in SMW's primary and secondary level ranges.
pub const SMW_US_V1_VANILLA_LEVEL_SLOTS: usize = 0x200;
/// Original headerless ROM boundary used by Lunar Magic's modified-level export predicate.
///
/// `ExportAllLevelsToDirectory` compares each Layer 1 payload PC offset with descriptor entry
/// `0x31`; the SMW-US revision-0 descriptor stores the original 512 KiB image length here.
pub const SMW_US_V1_ORIGINAL_LOGICAL_LEN: usize = 0x80_000;
/// Contiguous 24-bit Layer 1 object-stream pointer table.
pub const SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET: usize = 0x2e000;
/// Contiguous 24-bit Layer 2 object/tilemap pointer table.
pub const SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET: usize = 0x2e600;
/// One-byte per-level descriptor table installed by Lunar Magic's format-$103 Layer 2 runtime.
pub const SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET: usize = 0x77310;
/// Format-$103 hook base and its identifying `LM` marker.
pub const SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET: usize = 0x77510;
pub const SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET: usize =
    SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET + 0x3c;
/// Parallel low-word table for native sprite-stream pointers.
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET: usize = 0x2ec00;
/// Shared bank operand for native sprite-stream pointers.
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET: usize = 0x2d8f6;
/// Opcode byte selecting Lunar Magic's installed per-level sprite bank table.
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_HOOK_OFFSET: usize = 0x2d8f5;
/// Parallel 512-byte sprite bank table selected when the hook opcode is `JSL` (`$22`).
pub const SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_TABLE_OFFSET: usize = 0x77100;
/// Four 512-byte main-entrance planes, proven against Lunar Magic's complete MWL export corpus.
pub const SMW_US_V1_ENTRANCE_POSITION_OFFSET: usize = 0x2f000;
pub const SMW_US_V1_ENTRANCE_VERTICAL_SETTINGS_OFFSET: usize = 0x2f200;
pub const SMW_US_V1_ENTRANCE_SCREEN_AND_METHOD_OFFSET: usize = 0x2f400;
pub const SMW_US_V1_ENTRANCE_LEVEL_MODE_AND_SCREEN_OFFSET: usize = 0x2f600;
/// Four additional 512-byte Lfix3 planes proven against Ghidra's generation-3 loader and every
/// header in Lunar Magic's complete 512-level MWL export corpus.
pub const SMW_US_V1_LFIX3_FLAGS_OFFSET: usize = 0x2de00;
pub const SMW_US_V1_LFIX3_RUNTIME_FLAGS_OFFSET: usize = 0x37a00;
pub const SMW_US_V1_LFIX3_HIGH_POSITION_OFFSET: usize = 0x37c00;
pub const SMW_US_V1_LFIX3_ADDITIONAL_FLAGS_OFFSET: usize = 0x37e00;
/// Paired game-runtime JSL hooks which publish the expanded level-mode runtime entry point.
pub const SMW_US_V1_EXPANDED_LEVEL_MODE_HOOK_OFFSETS: [usize; 2] = [0x2da8a, 0x2db5f];
/// Lunar Magic subtracts `$240` from the shared runtime entry to address its 512-byte table.
pub const SMW_US_V1_EXPANDED_LEVEL_MODE_RUNTIME_BIAS: usize = 0x240;
/// JSL hook installed by Lunar Magic's separate-midway runtime.
pub const SMW_US_V1_SEPARATE_MIDWAY_HOOK_OFFSET: usize = 0x2d9e3;
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
pub enum SmwUsV1Layer2LayoutError {
    LevelOutOfRange(usize),
    AddressOverflow,
    Rom(RomError),
}

impl fmt::Display for SmwUsV1Layer2LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot resolve SMW-US Layer 2 layout: {self:?}")
    }
}

impl std::error::Error for SmwUsV1Layer2LayoutError {}

impl From<RomError> for SmwUsV1Layer2LayoutError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Returns the pristine SMW-US Layer 2 layout recovered from descriptor entry 26.
#[must_use]
pub const fn smw_us_v1_vanilla_layer2_layout() -> LevelLayer2RomLayout {
    LevelLayer2RomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET,
            entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
            stride: 3,
        },
        background_bank_substitution: Some(0x0c),
        legacy_pointer_redirect: Some(LevelLayer2PointerRedirect {
            selector_pointers: LevelPointerTable {
                offset: SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 3,
            },
            selector_value: [0x00, 0x80, 0x06],
            source_value: [0x00, 0xd9, 0xff],
            target_value: [0x54, 0xde, 0xff],
        }),
        descriptor_table: None,
        maximum_compressed_len: 0x8000,
        tilemap_encoding: LevelLayer2TilemapEncoding::Legacy { high_byte: 0 },
    }
}

/// Resolves the Layer 2 layout for one pristine or Lunar Magic-installed level.
///
/// Pristine SMW uses an `$FF` pointer bank to select a shared background in bank `$0C`; the layout
/// retains that native substitution instead of mistaking it for an absent Layer 2 payload.
///
/// # Errors
///
/// Rejects level indexes outside the 512 native slots, address overflow, truncated pointer tables,
/// and malformed installed format-$103 metadata.
pub fn smw_us_v1_level_layer2_layout(
    rom: &RomImage,
    level: usize,
) -> Result<Option<LevelLayer2RomLayout>, SmwUsV1Layer2LayoutError> {
    if level >= SMW_US_V1_VANILLA_LEVEL_SLOTS {
        return Err(SmwUsV1Layer2LayoutError::LevelOutOfRange(level));
    }
    let pointer_offset = SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET
        .checked_add(
            level
                .checked_mul(3)
                .ok_or(SmwUsV1Layer2LayoutError::AddressOverflow)?,
        )
        .ok_or(SmwUsV1Layer2LayoutError::AddressOverflow)?;
    rom.read(pointer_offset, 3)?;
    smw_us_v1_layer2_layout(rom).map(Some).map_err(Into::into)
}

/// Reports whether a pristine level entry selects SMW's shared bank-$0C background path.
///
/// # Errors
///
/// Rejects indexes outside the 512 native slots and truncated pointer tables.
pub fn smw_us_v1_level_uses_shared_background(
    rom: &RomImage,
    level: usize,
) -> Result<bool, SmwUsV1Layer2LayoutError> {
    if level >= SMW_US_V1_VANILLA_LEVEL_SLOTS {
        return Err(SmwUsV1Layer2LayoutError::LevelOutOfRange(level));
    }
    let offset = SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET
        .checked_add(
            level
                .checked_mul(3)
                .ok_or(SmwUsV1Layer2LayoutError::AddressOverflow)?,
        )
        .ok_or(SmwUsV1Layer2LayoutError::AddressOverflow)?;
    Ok(rom.read(offset, 3)?[2] == 0xff)
}

/// Detects the exact format-$103 Layer 2 descriptor table installed by Lunar Magic 3.63.
///
/// A pristine ROM retains the legacy layout. The installed layout points at the recovered
/// one-byte descriptor table so cross-bank background remaps can be loaded and persisted.
///
/// # Errors
///
/// Returns a ROM bounds error when the image is too short to contain the recovered format marker
/// or descriptor table.
pub fn smw_us_v1_layer2_layout(rom: &RomImage) -> Result<LevelLayer2RomLayout, RomError> {
    let marker = rom.read(SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET, 2)?;
    let mut layout = smw_us_v1_vanilla_layer2_layout();
    if marker == b"LM" {
        rom.read(
            SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
            SMW_US_V1_VANILLA_LEVEL_SLOTS,
        )?;
        layout.descriptor_table = Some(LevelLayer2DescriptorTable {
            offset: SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
            entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
            stride: 1,
        });
        // Format $103 can still load pre-migration $360 tilemaps, but Lunar Magic's next save
        // normalizes them to the current split-plane representation together with the descriptor.
        layout.tilemap_encoding = LevelLayer2TilemapEncoding::SplitPlanes;
    }
    Ok(layout)
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

/// Returns the direct two-entry GFX33/GFX32 layout used during SMW startup.
#[must_use]
pub const fn smw_us_v1_vanilla_special_graphics_layout() -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: SMW_US_V1_SPECIAL_GRAPHICS_POINTER_OFFSET,
            entries: SMW_US_V1_SPECIAL_GRAPHICS_FILES,
            stride: 3,
        },
        split_pointer_planes: None,
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

/// Resolves Lunar Magic's pristine shared-bank or installed per-level-bank sprite pointer table.
///
/// `LoadSpriteDataPcOffsetTable` in Lunar Magic 3.63 reads descriptor index 23 for the low words,
/// tests descriptor index 50's opcode for `$22`, and then selects either descriptor index 51's
/// 512 bank bytes or descriptor index 24's shared bank operand.
///
/// # Errors
///
/// Rejects a truncated hook, shared-bank operand, low-word table, or installed bank table.
pub fn smw_us_v1_sprite_pointer_table(rom: &RomImage) -> Result<SpritePointerTable, RomError> {
    rom.read(
        SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET,
        SMW_US_V1_VANILLA_LEVEL_SLOTS * 2,
    )?;
    let installed = rom.read(SMW_US_V1_LEVEL_SPRITE_POINTER_HOOK_OFFSET, 1)?[0] == 0x22;
    if installed {
        rom.read(
            SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_TABLE_OFFSET,
            SMW_US_V1_VANILLA_LEVEL_SLOTS,
        )?;
        Ok(SpritePointerTable::SplitBankTable {
            low_words: LevelPointerTable {
                offset: SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 2,
            },
            banks: LevelPointerTable {
                offset: SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_TABLE_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 1,
            },
        })
    } else {
        rom.read(SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET, 1)?;
        Ok(SpritePointerTable::SplitSharedBank {
            low_words: LevelPointerTable {
                offset: SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 2,
            },
            bank_offset: SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET,
        })
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

/// Returns the four extra per-level fields used by Lunar Magic's current Lfix3 runtime.
#[must_use]
pub const fn smw_us_v1_lfix3_level_fields_layout() -> Lfix3LevelFieldsRomLayout {
    Lfix3LevelFieldsRomLayout {
        mapper: Mapper::LoRom,
        flags_offset: SMW_US_V1_LFIX3_FLAGS_OFFSET,
        high_position_offset: SMW_US_V1_LFIX3_HIGH_POSITION_OFFSET,
        additional_flags_offset: SMW_US_V1_LFIX3_ADDITIONAL_FLAGS_OFFSET,
        runtime_flags_offset: SMW_US_V1_LFIX3_RUNTIME_FLAGS_OFFSET,
        entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
    }
}

/// Returns the cross-checked locator for Lunar Magic's expanded per-level mode/settings table.
#[must_use]
pub const fn smw_us_v1_expanded_level_mode_locator() -> ExpandedLevelModeLocator {
    ExpandedLevelModeLocator {
        mapper: Mapper::LoRom,
        hook_offsets: SMW_US_V1_EXPANDED_LEVEL_MODE_HOOK_OFFSETS,
        runtime_to_table_bias: SMW_US_V1_EXPANDED_LEVEL_MODE_RUNTIME_BIAS,
        entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
    }
}

#[must_use]
pub const fn smw_us_v1_separate_midway_locator() -> SeparateMidwayPatchLocator {
    SeparateMidwayPatchLocator {
        mapper: Mapper::LoRom,
        hook_offset: SMW_US_V1_SEPARATE_MIDWAY_HOOK_OFFSET,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind, SpriteLengthTable};
    use lm_project::Project;
    use std::{fs, path::PathBuf};

    #[test]
    fn layer2_layout_detects_only_the_exact_format_103_marker() {
        let mut bytes = vec![0xff; 0x80_000];
        let pristine = RomImage::from_bytes(bytes.clone()).unwrap();
        assert_eq!(
            smw_us_v1_layer2_layout(&pristine).unwrap(),
            smw_us_v1_vanilla_layer2_layout()
        );

        bytes[SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET
            ..SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET + 2]
            .copy_from_slice(b"LM");
        let installed = RomImage::from_bytes(bytes).unwrap();
        let installed_layout = smw_us_v1_layer2_layout(&installed).unwrap();
        assert_eq!(
            installed_layout.descriptor_table,
            Some(LevelLayer2DescriptorTable {
                offset: SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
                entries: SMW_US_V1_VANILLA_LEVEL_SLOTS,
                stride: 1,
            })
        );
        assert_eq!(
            installed_layout.tilemap_encoding,
            LevelLayer2TilemapEncoding::SplitPlanes
        );
    }

    #[test]
    fn all_lfix3_level_fields_match_complete_lunar_magic_mwl_corpus() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(
                fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                    .unwrap(),
            )
            .unwrap(),
        );
        for slot in 0..SMW_US_V1_VANILLA_LEVEL_SLOTS {
            let file = MwlFile::decode(
                &fs::read(root.join(format!(
                    "oracle-work/lm363/pristine-us/levels/Level {slot:03X}.mwl"
                )))
                .unwrap(),
            )
            .unwrap();
            let header =
                MwlLevelHeaderSection::decode(file.section(MwlSectionKind::LevelHeader)).unwrap();
            let expected = header.main_entrance();
            let actual = project
                .load_lfix3_level_fields(slot, smw_us_v1_lfix3_level_fields_layout())
                .unwrap();
            assert_eq!(actual.flags, expected.flags, "slot {slot:03X}");
            assert_eq!(
                actual.high_position, expected.high_position,
                "slot {slot:03X}"
            );
            assert_eq!(
                actual.additional_flags, expected.additional_flags,
                "slot {slot:03X}"
            );
            assert_eq!(actual.runtime_flags, header.0[17], "slot {slot:03X}");
        }
    }

    #[test]
    fn expanded_level_mode_locator_matches_installed_rom_and_complete_mwl_corpus() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(
                fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                    .unwrap(),
            )
            .unwrap(),
        );
        let locator = smw_us_v1_expanded_level_mode_locator();
        assert_eq!(locator.resolve(&project).unwrap(), 0x83cfa);
        for slot in 0..SMW_US_V1_VANILLA_LEVEL_SLOTS {
            let file = MwlFile::decode(
                &fs::read(root.join(format!(
                    "oracle-work/lm363/pristine-us/levels/Level {slot:03X}.mwl"
                )))
                .unwrap(),
            )
            .unwrap();
            let header =
                MwlLevelHeaderSection::decode(file.section(MwlSectionKind::LevelHeader)).unwrap();
            assert_eq!(
                project.load_expanded_level_mode(slot, locator).unwrap(),
                header.0[16] & 0x7f,
                "slot {slot:03X}"
            );
        }
    }

    #[test]
    fn retained_format_103_level_normalizes_legacy_descriptor_before_split_plane_save() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let layout = smw_us_v1_layer2_layout(&project.rom).unwrap();
        assert_eq!(
            layout.tilemap_encoding,
            LevelLayer2TilemapEncoding::SplitPlanes
        );
        let level = project
            .load_level_slot(
                0x105,
                smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap();
        let layer2 = project
            .load_level_layer2_with_descriptor(0x105, level.layer1.header.level_mode(), layout)
            .unwrap();
        assert_eq!(
            layer2.descriptor,
            Some(lm_level::MwlLayer2Descriptor::from_raw(0x0c))
        );
        assert!(matches!(
            layer2.data,
            lm_level::NativeLayer2Data::Tilemap(ref bytes) if bytes.len() == 0x800
        ));
    }

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

    #[test]
    fn every_pristine_layer2_pointer_decodes_with_shared_background_substitution() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let layer2 = smw_us_v1_vanilla_layer2_layout();
        let level = smw_us_v1_vanilla_level_layout();
        let mut decoded = 0;
        let mut object_layers = 0;
        let mut shared_backgrounds = 0;
        for slot in 0..SMW_US_V1_VANILLA_LEVEL_SLOTS {
            let pointer = project
                .rom
                .read(SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET + slot * 3, 3)
                .unwrap();
            if pointer[2] == 0xff {
                shared_backgrounds += 1;
            }
            let loaded = project
                .load_level_slot(slot, level, &SpriteLengthTable::standard())
                .unwrap();
            match project
                .load_level_layer2(slot, loaded.layer1.header.level_mode(), layer2)
                .unwrap_or_else(|error| panic!("slot {slot:03X}: {error}"))
            {
                lm_level::NativeLayer2Data::Objects(_) => object_layers += 1,
                lm_level::NativeLayer2Data::Tilemap(_) => {}
            }
            decoded += 1;
        }
        assert_eq!(decoded, SMW_US_V1_VANILLA_LEVEL_SLOTS);
        assert!(object_layers > 0);
        assert!(shared_backgrounds > 0);
    }

    #[test]
    fn pristine_layer2_resolver_retains_shared_background_slots() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let rom = RomImage::from_bytes(bytes).unwrap();
        assert_eq!(
            smw_us_v1_level_layer2_layout(&rom, 0x105)
                .unwrap()
                .unwrap()
                .background_bank_substitution,
            Some(0x0c)
        );
        assert!(smw_us_v1_level_uses_shared_background(&rom, 0x105).unwrap());
        assert!(
            (0..SMW_US_V1_VANILLA_LEVEL_SLOTS)
                .any(|level| { !smw_us_v1_level_uses_shared_background(&rom, level).unwrap() })
        );
        let present = (0..SMW_US_V1_VANILLA_LEVEL_SLOTS)
            .find(|&level| {
                smw_us_v1_level_layer2_layout(&rom, level).is_ok_and(|layout| layout.is_some())
            })
            .expect("pristine SMW must contain at least one Layer 2 level");
        assert_eq!(
            smw_us_v1_level_layer2_layout(&rom, present).unwrap(),
            Some(smw_us_v1_vanilla_layer2_layout())
        );
        assert_eq!(
            smw_us_v1_level_layer2_layout(&rom, SMW_US_V1_VANILLA_LEVEL_SLOTS),
            Err(SmwUsV1Layer2LayoutError::LevelOutOfRange(
                SMW_US_V1_VANILLA_LEVEL_SLOTS
            ))
        );
    }
}
