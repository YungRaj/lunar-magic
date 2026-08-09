//! Descriptor-derived native custom-overworld-sprite routing.
//!
//! Lunar Magic 3.63's `LoadCustomOverworldSpriteRecords` (`$004BDE10`) reads the stream
//! pointer from active descriptor field `+$114`, plus `$0D`. Its adjacent
//! `LoadOverworldSpriteRecordSizeTable` (`$004BDB10`) uses descriptor field `+$BFC` as a
//! three-byte pointer operand and byte `+$03` as the `$42` installed marker. Descriptor values
//! include the copier prefix; [`RomImage`] offsets do not.

use lm_overworld::CUSTOM_OVERWORLD_SPRITE_ID_COUNT;
use lm_project::NativeCustomOverworldSpriteRomLayout;
use lm_rats::{HEADER_LEN, HeaderError, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, RomImage, SnesPointer24, SupportedGame};

pub const LUNAR_MAGIC_CUSTOM_OVERWORLD_SPRITE_DESCRIPTOR_FIELD: usize = 0x114;
pub const LUNAR_MAGIC_OVERWORLD_SPRITE_SIZE_DESCRIPTOR_FIELD: usize = 0xbfc;
pub const SMW_US_V1_CUSTOM_OVERWORLD_SPRITE_MAX_PAYLOAD_LEN: usize = 0x0fff;

const COPIER_PREFIX: usize = 0x200;
const SMW_STREAM_BASE_PHYSICAL: usize = 0x077750;
const SMW_SIZE_POINTER_PHYSICAL: usize = 0x06e38c;
const ALL_STARS_STREAM_BASE_PHYSICAL: usize = 0x277780;
const ALL_STARS_SIZE_POINTER_PHYSICAL: usize = 0x16638c;
const STREAM_OPERAND_DISPLACEMENT: usize = 0x0d;
const SIZE_INSTALLED_MARKER: u8 = 0x42;
const DEFAULT_RECORD_SIZE: u8 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1NativeCustomOverworldSpriteLayout {
    pub stream: NativeCustomOverworldSpriteRomLayout,
    pub record_sizes: [u8; CUSTOM_OVERWORLD_SPRITE_ID_COUNT],
    pub record_size_pointer_offset: usize,
    pub record_size_block: Option<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1NativeCustomOverworldSpriteLayoutError {
    Rom(RomError),
    AddressOverflow,
    SizePointerEncoding(usize),
    SizePointerBeforeHeader(usize),
    SizeHeader { offset: usize, source: HeaderError },
    SizeOwnerStart { expected: usize, actual: usize },
    SizePayloadLength(usize),
}

impl std::fmt::Display for SmwUsV1NativeCustomOverworldSpriteLayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "native custom overworld sprite layout failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1NativeCustomOverworldSpriteLayoutError {}

impl From<RomError> for SmwUsV1NativeCustomOverworldSpriteLayoutError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

const fn active_body(mapper: Mapper) -> usize {
    if matches!(mapper, Mapper::ExLoRom) {
        0x40_0000
    } else {
        0
    }
}

fn logical_descriptor_offset(
    game: SupportedGame,
    mapper: Mapper,
    smw_physical: usize,
    all_stars_physical: usize,
) -> Result<usize, SmwUsV1NativeCustomOverworldSpriteLayoutError> {
    active_body(mapper)
        .checked_add(match game {
            SupportedGame::SuperMarioWorld => smw_physical,
            SupportedGame::AllStarsAndWorld => all_stars_physical,
        })
        .and_then(|offset| offset.checked_sub(COPIER_PREFIX))
        .ok_or(SmwUsV1NativeCustomOverworldSpriteLayoutError::AddressOverflow)
}

/// Resolves both descriptor-selected operands and authenticates an installed size-table owner.
///
/// A pristine ROM has no `$42` marker and therefore uses Lunar Magic's initialized four-byte
/// record size for all IDs. Installed tables may contain either 128 bytes, or the legacy 127-byte
/// tail beginning at ID 1; every value is normalized to the low nibble and clamped to 3..15 just
/// as the original loader does.
pub fn smw_us_v1_native_custom_overworld_sprite_layout(
    image: &RomImage,
    game: SupportedGame,
    mapper: Mapper,
) -> Result<SmwUsV1NativeCustomOverworldSpriteLayout, SmwUsV1NativeCustomOverworldSpriteLayoutError>
{
    let stream_base = logical_descriptor_offset(
        game,
        mapper,
        SMW_STREAM_BASE_PHYSICAL,
        ALL_STARS_STREAM_BASE_PHYSICAL,
    )?;
    let pointer_offset = stream_base
        .checked_add(STREAM_OPERAND_DISPLACEMENT)
        .ok_or(SmwUsV1NativeCustomOverworldSpriteLayoutError::AddressOverflow)?;
    image.read(pointer_offset, 3)?;

    let size_pointer_offset = logical_descriptor_offset(
        game,
        mapper,
        SMW_SIZE_POINTER_PHYSICAL,
        ALL_STARS_SIZE_POINTER_PHYSICAL,
    )?;
    let marker = image.read(size_pointer_offset + 3, 1)?[0];
    let mut record_sizes = [DEFAULT_RECORD_SIZE; CUSTOM_OVERWORLD_SPRITE_ID_COUNT];
    let mut record_size_block = None;
    if marker == SIZE_INSTALLED_MARKER {
        let raw = image.read(size_pointer_offset, 3)?;
        let pointer = SnesPointer24::decode(raw)
            .map_err(SmwUsV1NativeCustomOverworldSpriteLayoutError::SizePointerEncoding)?;
        let payload_offset = pointer.to_pc(mapper)?;
        let header = payload_offset.checked_sub(HEADER_LEN).ok_or(
            SmwUsV1NativeCustomOverworldSpriteLayoutError::SizePointerBeforeHeader(payload_offset),
        )?;
        let block = parse_at(image.logical_bytes(), header).map_err(|source| {
            SmwUsV1NativeCustomOverworldSpriteLayoutError::SizeHeader {
                offset: header,
                source,
            }
        })?;
        if block.payload.start != payload_offset {
            return Err(
                SmwUsV1NativeCustomOverworldSpriteLayoutError::SizeOwnerStart {
                    expected: payload_offset,
                    actual: block.payload.start,
                },
            );
        }
        let payload = &image.logical_bytes()[block.payload.clone()];
        match payload.len() {
            128 => record_sizes.copy_from_slice(payload),
            127 => record_sizes[1..].copy_from_slice(payload),
            actual => {
                return Err(
                    SmwUsV1NativeCustomOverworldSpriteLayoutError::SizePayloadLength(actual),
                );
            }
        }
        for size in &mut record_sizes {
            *size = (*size & 0x0f).clamp(3, 15);
        }
        record_size_block = Some(block);
    }
    Ok(SmwUsV1NativeCustomOverworldSpriteLayout {
        stream: NativeCustomOverworldSpriteRomLayout {
            mapper,
            pointer_offset,
            maximum_payload_len: SMW_US_V1_CUSTOM_OVERWORLD_SPRITE_MAX_PAYLOAD_LEN,
        },
        record_sizes,
        record_size_pointer_offset: size_pointer_offset,
        record_size_block,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::make_header;
    use lm_rom::pc_to_snes;

    fn image(len: usize) -> RomImage {
        RomImage::from_bytes(vec![0xff; len]).unwrap()
    }

    #[test]
    fn descriptor_routes_cover_smw_mapper_bodies_and_all_stars() {
        for (game, mapper, stream, sizes) in [
            (
                SupportedGame::SuperMarioWorld,
                Mapper::LoRom,
                0x7755d,
                0x6e18c,
            ),
            (
                SupportedGame::SuperMarioWorld,
                Mapper::Sa1,
                0x7755d,
                0x6e18c,
            ),
            (
                SupportedGame::SuperMarioWorld,
                Mapper::ExLoRom,
                0x47755d,
                0x46e18c,
            ),
            (
                SupportedGame::AllStarsAndWorld,
                Mapper::LoRom,
                0x27758d,
                0x16618c,
            ),
            (
                SupportedGame::AllStarsAndWorld,
                Mapper::ExLoRom,
                0x67758d,
                0x56618c,
            ),
        ] {
            let resolved =
                smw_us_v1_native_custom_overworld_sprite_layout(&image(0x80_0000), game, mapper)
                    .unwrap();
            assert_eq!(resolved.stream.pointer_offset, stream);
            assert_eq!(resolved.record_size_pointer_offset, sizes);
            assert_eq!(resolved.record_sizes, [4; 128]);
            assert!(resolved.record_size_block.is_none());
        }
    }

    #[test]
    fn installed_size_owner_is_authenticated_and_normalized() {
        let mut bytes = vec![0xff; 0x10_0000];
        let payload_offset = 0x8010;
        let header = payload_offset - HEADER_LEN;
        bytes[header..payload_offset].copy_from_slice(&make_header(128).unwrap());
        for (index, byte) in bytes[payload_offset..payload_offset + 128]
            .iter_mut()
            .enumerate()
        {
            *byte = match index {
                0 => 0,
                1 => 0x22,
                2 => 0xff,
                _ => 5,
            };
        }
        let pointer =
            SnesPointer24::new(pc_to_snes(Mapper::LoRom, payload_offset).unwrap()).unwrap();
        bytes[0x6e18c..0x6e18f].copy_from_slice(&pointer.encode());
        bytes[0x6e18f] = SIZE_INSTALLED_MARKER;
        let resolved = smw_us_v1_native_custom_overworld_sprite_layout(
            &RomImage::from_bytes(bytes).unwrap(),
            SupportedGame::SuperMarioWorld,
            Mapper::LoRom,
        )
        .unwrap();
        assert_eq!(&resolved.record_sizes[..4], &[3, 3, 15, 5]);
        assert_eq!(
            resolved.record_size_block.unwrap().payload,
            payload_offset..payload_offset + 128
        );
    }
}
