use lm_rom::{RomError, RomImage};
use std::fmt;

pub const SMW_US_V1_STANDARD_OBJECT_FAMILIES: usize = 5;
pub const SMW_US_V1_STANDARD_OBJECTS_PER_FAMILY: usize = 64;
pub const SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION: u8 = 0xff;

const TABLE_OFFSETS: [usize; SMW_US_V1_STANDARD_OBJECT_FAMILIES] =
    [0x6a455, 0x6c19a, 0x6cd9a, 0x6d99a, 0x6e89a];
#[allow(clippy::unreadable_literal)]
const KNOWN_HANDLERS: [u32; 78] = [
    0x0da8c3, 0x0daa26, 0x0daab4, 0x0dab0d, 0x0dab3e, 0x0db075, 0x0db1c8, 0x0db1d4, 0x0db224,
    0x0db336, 0x0db3bd, 0x0db3e3, 0x0db42d, 0x0db461, 0x0db49e, 0x0db51f, 0x0db547, 0x0db5b7,
    0x0db604, 0x0db6c3, 0x0db705, 0x0db73f, 0x0db7aa, 0x0db863, 0x0db916, 0x0db91e, 0x0db966,
    0x0db9c0, 0x0dba0a, 0x0dba4c, 0x0dbadc, 0x0dbb2c, 0x0dbb63, 0x0dc341, 0x0dc42e, 0x0dc44f,
    0x0dc478, 0x0dc4c9, 0x0dc4ef, 0x0dc58a, 0x0dc5d8, 0x0dcef2, 0x0dcf12, 0x0dcf33, 0x0dcf53,
    0x0dd070, 0x0dd103, 0x0dd145, 0x0dd182, 0x0dd1a5, 0x0dd1d9, 0x0dd24e, 0x0ddac8, 0x0ddaf2,
    0x0ddca9, 0x0ddcea, 0x0ddd2e, 0x0ddd5c, 0x0ddd87, 0x0ddf3a, 0x0de135, 0x0decc9, 0x0ded12,
    0x0ded43, 0x0ded6b, 0x0ded99, 0x0dedb9, 0x0deddb, 0x0dee17, 0x0dee52, 0x0dee89, 0x0deec0,
    0x0def45, 0x0def67, 0x0defa8, 0x0df02b, 0x0df066, 0x0df06c,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwUsV1StandardObjectDefinitionMap {
    families: [[u8; SMW_US_V1_STANDARD_OBJECTS_PER_FAMILY]; SMW_US_V1_STANDARD_OBJECT_FAMILIES],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwUsV1StandardObjectMapError {
    Rom(RomError),
}

impl fmt::Display for SmwUsV1StandardObjectMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot load vanilla standard-object map: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1StandardObjectMapError {}

impl From<RomError> for SmwUsV1StandardObjectMapError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl SmwUsV1StandardObjectDefinitionMap {
    #[must_use]
    pub fn family(&self, family: usize) -> Option<&[u8; SMW_US_V1_STANDARD_OBJECTS_PER_FAMILY]> {
        self.families.get(family)
    }

    #[must_use]
    pub fn definition(&self, family: usize, object: u8) -> Option<u8> {
        let definition = *self.family(family)?.get(usize::from(object))?;
        (definition != SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION).then_some(definition)
    }
}

/// Reproduces `LoadStandardObjectDefinitionIndexMap` for an unmodified SMW-US revision-0 ROM.
///
/// Each family stores object zero as definition zero followed by 63 packed 24-bit SNES handler
/// pointers. Known pointers are replaced with their index in Lunar Magic's sorted 78-handler
/// catalog; foreign pointers remain explicitly unknown.
///
/// # Errors
///
/// Returns a ROM range error when any complete family table is unavailable.
pub fn load_smw_us_v1_standard_object_definition_map(
    rom: &RomImage,
) -> Result<SmwUsV1StandardObjectDefinitionMap, SmwUsV1StandardObjectMapError> {
    let mut families = [[SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION;
        SMW_US_V1_STANDARD_OBJECTS_PER_FAMILY];
        SMW_US_V1_STANDARD_OBJECT_FAMILIES];
    for (family_index, &offset) in TABLE_OFFSETS.iter().enumerate() {
        families[family_index][0] = 0;
        let bytes = rom.read(offset, 63 * 3)?;
        for (object_index, pointer) in bytes.chunks_exact(3).enumerate() {
            let address = u32::from_le_bytes([pointer[0], pointer[1], pointer[2], 0]) & 0x7f_ffff;
            families[family_index][object_index + 1] = if address == 0 {
                SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION
            } else {
                KNOWN_HANDLERS
                    .binary_search(&address)
                    .ok()
                    .and_then(|index| u8::try_from(index).ok())
                    .unwrap_or(SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION)
            };
        }
    }
    Ok(SmwUsV1StandardObjectDefinitionMap { families })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_packed_handlers_and_retains_foreign_entries() {
        let mut bytes = vec![0; TABLE_OFFSETS[4] + 63 * 3];
        for &offset in &TABLE_OFFSETS {
            bytes[offset..offset + 3].copy_from_slice(&KNOWN_HANDLERS[17].to_le_bytes()[..3]);
            bytes[offset + 3..offset + 6].copy_from_slice(&[0x34, 0x12, 0x00]);
        }
        let map =
            load_smw_us_v1_standard_object_definition_map(&RomImage::from_bytes(bytes).unwrap())
                .unwrap();
        for family in 0..SMW_US_V1_STANDARD_OBJECT_FAMILIES {
            assert_eq!(map.definition(family, 0), Some(0));
            assert_eq!(map.definition(family, 1), Some(17));
            assert_eq!(map.definition(family, 2), None);
        }
    }

    #[test]
    fn every_pristine_family_entry_maps_to_the_recovered_handler_catalog() {
        let bytes = crate::test_support::pristine_smw_us_rom_bytes();
        let map =
            load_smw_us_v1_standard_object_definition_map(&RomImage::from_bytes(bytes).unwrap())
                .unwrap();
        for family in 0..SMW_US_V1_STANDARD_OBJECT_FAMILIES {
            assert_eq!(map.definition(family, 0), Some(0));
            assert_eq!(map.definition(family, 15), Some(1));
            assert!(
                map.family(family)
                    .unwrap()
                    .iter()
                    .all(|definition| *definition != SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION)
            );
        }
    }
}
