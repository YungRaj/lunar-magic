//! Descriptor-backed built-in overworld-animation table locations.
//!
//! Lunar Magic 3.63's `LoadNativeGraphicsAndCoreTables` path at `$004BA8D0` reads exactly
//! `$86` bytes from active ROM-layout-descriptor field `+$5C4`. The SMW descriptors store
//! physical file offsets (including the `$200` copier prefix), while Rust's [`RomImage`] always
//! exposes logical, headerless offsets. ExLoROM additionally selects the active SMW body in the
//! upper 4 MiB; SA-1's descriptor names its separately relocated table.

use lm_rom::{Mapper, RomError, RomImage};

/// Byte offset of the table pointer inside Lunar Magic's active ROM-layout descriptor.
pub const LUNAR_MAGIC_OVERWORLD_ANIMATION_DESCRIPTOR_FIELD: usize = 0x05c4;
/// Number of little-endian VRAM source words copied by Lunar Magic.
pub const SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS: usize = 3 + 8 * 8;
/// Descriptor value for the ordinary SMW LoROM table, including the copier prefix.
pub const SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET: usize = 0x02_0200;
/// Descriptor value for the SA-1-relocated table, including the copier prefix.
pub const SMW_US_V1_SA1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET: usize = 0x1a_0200;

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
/// ExLoROM retains the ordinary descriptor value but adds `$400000` while selecting the active
/// SMW body. SA-1's dedicated descriptor stores physical `$1A0200` instead.
#[must_use]
pub const fn smw_us_v1_builtin_overworld_animation_table_offset(mapper: Mapper) -> usize {
    match mapper {
        Mapper::LoRom => SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET - 0x200,
        Mapper::ExLoRom => {
            0x40_0000 + SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET - 0x200
        }
        Mapper::Sa1 => SMW_US_V1_SA1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET - 0x200,
    }
}

/// Loads and validates the exact 67-word table selected by Lunar Magic's active descriptor.
///
/// A malformed selected table is an error. In particular, this never falls back to the lower
/// ExLoROM compatibility mirror or to LoROM's address when the SA-1 table is invalid.
pub fn load_smw_us_v1_builtin_overworld_animation_table_for_mapper(
    image: &RomImage,
    mapper: Mapper,
) -> Result<SmwUsV1BuiltInOverworldAnimationTable, SmwUsV1BuiltInOverworldAnimationError> {
    let logical_offset = smw_us_v1_builtin_overworld_animation_table_offset(mapper);
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
            let offset = smw_us_v1_builtin_overworld_animation_table_offset(mapper);
            for (index, address) in addresses.into_iter().enumerate() {
                bytes[offset + index * 2..offset + index * 2 + 2]
                    .copy_from_slice(&address.to_le_bytes());
            }
        }
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn descriptor_field_routes_every_supported_mapper_to_its_active_body() {
        assert_eq!(LUNAR_MAGIC_OVERWORLD_ANIMATION_DESCRIPTOR_FIELD, 0x5c4);
        assert_eq!(
            smw_us_v1_builtin_overworld_animation_table_offset(Mapper::LoRom),
            0x020000
        );
        assert_eq!(
            smw_us_v1_builtin_overworld_animation_table_offset(Mapper::ExLoRom),
            0x420000
        );
        assert_eq!(
            smw_us_v1_builtin_overworld_animation_table_offset(Mapper::Sa1),
            0x1a0000
        );

        let lo = table(0x3000);
        let ex = table(0x5000);
        let sa1 = table(0x7000);
        let image = image_with_tables(&[
            (Mapper::LoRom, lo),
            (Mapper::ExLoRom, ex),
            (Mapper::Sa1, sa1),
        ]);
        for (mapper, expected) in [
            (Mapper::LoRom, lo),
            (Mapper::ExLoRom, ex),
            (Mapper::Sa1, sa1),
        ] {
            assert_eq!(
                load_smw_us_v1_builtin_overworld_animation_table_for_mapper(&image, mapper)
                    .unwrap()
                    .addresses,
                expected,
            );
        }
    }

    #[test]
    fn selected_table_corruption_rejects_without_cross_layout_fallback() {
        let valid_lower_mirror = table(0x3000);
        let mut image = image_with_tables(&[(Mapper::LoRom, valid_lower_mirror)]);
        assert!(matches!(
            load_smw_us_v1_builtin_overworld_animation_table_for_mapper(&image, Mapper::ExLoRom),
            Err(
                SmwUsV1BuiltInOverworldAnimationError::InvalidSourceAddress {
                    index: 0,
                    address: 0xffff
                }
            )
        ));

        let selected = smw_us_v1_builtin_overworld_animation_table_offset(Mapper::Sa1);
        image.write(selected, &0x1fff_u16.to_le_bytes()).unwrap();
        assert!(matches!(
            load_smw_us_v1_builtin_overworld_animation_table_for_mapper(&image, Mapper::Sa1),
            Err(
                SmwUsV1BuiltInOverworldAnimationError::InvalidSourceAddress {
                    index: 0,
                    address: 0x1fff
                }
            )
        ));
    }
}
