//! Descriptor-backed built-in overworld-animation table locations.
//!
//! Lunar Magic 3.63's `LoadNativeGraphicsAndCoreTables` path at `$004BA8D0` reads exactly
//! `$86` bytes from active ROM-layout-descriptor field `+$5C4`. The SMW descriptors store
//! physical file offsets (including the `$200` copier prefix), while Rust's [`RomImage`] always
//! exposes logical, headerless offsets. ExLoROM additionally selects the active SMW body in the
//! upper 4 MiB. All-Stars + World has a separately relocated SMW body; SA-1 retains the ordinary
//! SMW table location.

use lm_rom::{Mapper, RomError, RomImage, SupportedGame};

/// Byte offset of the table pointer inside Lunar Magic's active ROM-layout descriptor.
pub const LUNAR_MAGIC_OVERWORLD_ANIMATION_DESCRIPTOR_FIELD: usize = 0x05c4;
/// Number of little-endian VRAM source words copied by Lunar Magic.
pub const SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS: usize = 3 + 8 * 8;
/// Descriptor value for the ordinary SMW LoROM table, including the copier prefix.
pub const SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET: usize = 0x02_0200;
/// Descriptor value for All-Stars + World's relocated SMW table, including the copier prefix.
pub const ALL_STARS_WORLD_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET: usize = 0x1a_0200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1BuiltInOverworldAnimationTable {
    pub logical_offset: usize,
    pub addresses: [u16; SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1BuiltInOverworldAnimationError {
    Rom(RomError),
    InvalidSourceAddress { index: usize, address: u16 },
}

impl std::fmt::Display for SmwUsV1BuiltInOverworldAnimationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rom(error) => write!(
                formatter,
                "cannot read built-in overworld animation table: {error}"
            ),
            Self::InvalidSourceAddress { index, address } => write!(
                formatter,
                "built-in overworld animation source {index} is ${address:04X}, expected $2000..$C7FF",
            ),
        }
    }
}

impl std::error::Error for SmwUsV1BuiltInOverworldAnimationError {}

impl From<RomError> for SmwUsV1BuiltInOverworldAnimationError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Returns the logical counterpart of Lunar Magic descriptor field `+$5C4`.
///
/// ExLoROM adds `$400000` while selecting the active SMW body. All-Stars + World's dedicated
/// descriptor stores physical `$1A0200`; SA-1 retains the ordinary SMW descriptor location.
#[must_use]
pub const fn builtin_overworld_animation_table_offset(
    game: SupportedGame,
    mapper: Mapper,
) -> usize {
    let physical = match game {
        SupportedGame::SuperMarioWorld => SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET,
        SupportedGame::AllStarsAndWorld => {
            ALL_STARS_WORLD_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET
        }
    };
    let active_body = if matches!(mapper, Mapper::ExLoRom) {
        0x40_0000
    } else {
        0
    };
    active_body + physical - 0x200
}

/// Loads and validates the exact 67-word table selected by Lunar Magic's active descriptor.
///
/// A malformed selected table is an error. In particular, this never falls back to the lower
/// ExLoROM compatibility mirror or to LoROM's address when the SA-1 table is invalid.
pub fn load_builtin_overworld_animation_table(
    image: &RomImage,
    game: SupportedGame,
    mapper: Mapper,
) -> Result<SmwUsV1BuiltInOverworldAnimationTable, SmwUsV1BuiltInOverworldAnimationError> {
    let logical_offset = builtin_overworld_animation_table_offset(game, mapper);
    let bytes = image.read(
        logical_offset,
        SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS * 2,
    )?;
    let mut addresses = [0_u16; SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let address = u16::from_le_bytes([pair[0], pair[1]]);
        if !(0x2000..0xc800).contains(&usize::from(address)) {
            return Err(
                SmwUsV1BuiltInOverworldAnimationError::InvalidSourceAddress { index, address },
            );
        }
        addresses[index] = address;
    }
    Ok(SmwUsV1BuiltInOverworldAnimationTable {
        logical_offset,
        addresses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(seed: u16) -> [u16; SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS] {
        std::array::from_fn(|index| seed + u16::try_from(index).unwrap() * 0x18)
    }

    fn image_with_tables(
        tables: &[(Mapper, [u16; SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS])],
    ) -> RomImage {
        let mut bytes = vec![0xff; 0x80_0000];
        for &(mapper, addresses) in tables {
            let offset =
                builtin_overworld_animation_table_offset(SupportedGame::SuperMarioWorld, mapper);
            for (index, address) in addresses.into_iter().enumerate() {
                bytes[offset + index * 2..offset + index * 2 + 2]
                    .copy_from_slice(&address.to_le_bytes());
            }
        }
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn descriptor_field_routes_every_supported_identity_to_its_active_body() {
        assert_eq!(LUNAR_MAGIC_OVERWORLD_ANIMATION_DESCRIPTOR_FIELD, 0x5c4);
        assert_eq!(
            builtin_overworld_animation_table_offset(SupportedGame::SuperMarioWorld, Mapper::LoRom),
            0x020000
        );
        assert_eq!(
            builtin_overworld_animation_table_offset(
                SupportedGame::SuperMarioWorld,
                Mapper::ExLoRom
            ),
            0x420000
        );
        assert_eq!(
            builtin_overworld_animation_table_offset(SupportedGame::SuperMarioWorld, Mapper::Sa1),
            0x020000
        );
        assert_eq!(
            builtin_overworld_animation_table_offset(
                SupportedGame::AllStarsAndWorld,
                Mapper::LoRom
            ),
            0x1a0000
        );
        assert_eq!(
            builtin_overworld_animation_table_offset(
                SupportedGame::AllStarsAndWorld,
                Mapper::ExLoRom
            ),
            0x5a0000
        );

        let lo = table(0x3000);
        let ex = table(0x5000);
        let all_stars = table(0x7000);
        let image = image_with_tables(&[(Mapper::LoRom, lo), (Mapper::ExLoRom, ex)]);
        for (mapper, expected) in [
            (Mapper::LoRom, lo),
            (Mapper::ExLoRom, ex),
            (Mapper::Sa1, lo),
        ] {
            assert_eq!(
                load_builtin_overworld_animation_table(
                    &image,
                    SupportedGame::SuperMarioWorld,
                    mapper,
                )
                .unwrap()
                .addresses,
                expected,
            );
        }
        let mut all_stars_image = vec![0xff; 0x80_0000];
        let all_stars_offset = builtin_overworld_animation_table_offset(
            SupportedGame::AllStarsAndWorld,
            Mapper::LoRom,
        );
        for (index, address) in all_stars.into_iter().enumerate() {
            all_stars_image[all_stars_offset + index * 2..all_stars_offset + index * 2 + 2]
                .copy_from_slice(&address.to_le_bytes());
        }
        assert_eq!(
            load_builtin_overworld_animation_table(
                &RomImage::from_bytes(all_stars_image).unwrap(),
                SupportedGame::AllStarsAndWorld,
                Mapper::LoRom,
            )
            .unwrap()
            .addresses,
            all_stars,
        );
    }

    #[test]
    fn selected_table_corruption_rejects_without_cross_layout_fallback() {
        let valid_lower_mirror = table(0x3000);
        let mut image = image_with_tables(&[(Mapper::LoRom, valid_lower_mirror)]);
        assert!(matches!(
            load_builtin_overworld_animation_table(
                &image,
                SupportedGame::SuperMarioWorld,
                Mapper::ExLoRom,
            ),
            Err(
                SmwUsV1BuiltInOverworldAnimationError::InvalidSourceAddress {
                    index: 0,
                    address: 0xffff
                }
            )
        ));

        let selected = builtin_overworld_animation_table_offset(
            SupportedGame::AllStarsAndWorld,
            Mapper::LoRom,
        );
        image.write(selected, &0x1fff_u16.to_le_bytes()).unwrap();
        assert!(matches!(
            load_builtin_overworld_animation_table(
                &image,
                SupportedGame::AllStarsAndWorld,
                Mapper::LoRom,
            ),
            Err(
                SmwUsV1BuiltInOverworldAnimationError::InvalidSourceAddress {
                    index: 0,
                    address: 0x1fff
                }
            )
        ));
    }
}
