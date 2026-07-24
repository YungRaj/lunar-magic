use crate::{Mapper, RomImage, SnesChecksum, compute_snes_checksum, mapper_supports_image_len};
use std::fmt;

const LOW_HEADER_OFFSET: usize = 0x7fc0;
const HIGH_HEADER_OFFSET: usize = 0xffc0;
const INTERNAL_HEADER_LEN: usize = 0x20;
const TITLE_LEN: usize = 21;
const SMW_TITLE: &[u8; TITLE_LEN] = b"SUPER MARIOWORLD     ";
const ALL_STARS_WORLD_TITLE: &[u8; TITLE_LEN] = b"ALL_STARS + WORLD    ";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportedGame {
    SuperMarioWorld,
    AllStarsAndWorld,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Japan,
    NorthAmerica,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomIdentity {
    pub game: SupportedGame,
    pub mapper: Mapper,
    pub region: Region,
    pub revision: u8,
    pub map_mode: u8,
    pub cartridge_type: u8,
    pub internal_header_offset: usize,
    pub stored_checksum: SnesChecksum,
    pub computed_checksum: SnesChecksum,
}

impl RomIdentity {
    #[must_use]
    pub const fn checksum_matches(&self) -> bool {
        self.stored_checksum.checksum == self.computed_checksum.checksum
            && self.stored_checksum.is_complementary()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityError {
    MissingInternalHeader,
    AmbiguousInternalHeader,
    UnsupportedTitle([u8; TITLE_LEN]),
    UnsupportedRegion(u8),
    UnsupportedRevision(u8),
    UnsupportedMapMode(u8),
    UnsupportedRomSize { mapper: Mapper, logical_len: usize },
    InvalidChecksumField,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unsupported Lunar Magic ROM identity: {self:?}")
    }
}

impl std::error::Error for IdentityError {}

/// Applies the ROM identity rules recovered from `ValidateAndInitializeOpenedRom`.
///
/// # Errors
///
/// Returns [`IdentityError`] unless the internal header identifies a supported SMW image,
/// supported region/revision, and an accessible checksum field.
pub fn detect_identity(rom: &RomImage) -> Result<RomIdentity, IdentityError> {
    let bytes = rom.logical_bytes();
    let (internal_header_offset, exlorom) = locate_internal_header(bytes)?;
    let header = bytes
        .get(internal_header_offset..internal_header_offset + INTERNAL_HEADER_LEN)
        .ok_or(IdentityError::MissingInternalHeader)?;
    let title: [u8; TITLE_LEN] = header[..TITLE_LEN]
        .try_into()
        .map_err(|_| IdentityError::MissingInternalHeader)?;
    let game = match &title {
        SMW_TITLE => SupportedGame::SuperMarioWorld,
        ALL_STARS_WORLD_TITLE => SupportedGame::AllStarsAndWorld,
        _ => return Err(IdentityError::UnsupportedTitle(title)),
    };
    let region_byte = header[0x19];
    let region = match region_byte {
        0 if game == SupportedGame::SuperMarioWorld => Region::Japan,
        1 => Region::NorthAmerica,
        _ => return Err(IdentityError::UnsupportedRegion(region_byte)),
    };
    let revision = header[0x1b];
    if revision != 0 {
        return Err(IdentityError::UnsupportedRevision(revision));
    }
    let map_mode = header[0x15];
    let mapper = match map_mode {
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        0x20 | 0x30 if exlorom => Mapper::ExLoRom,
        0x20 | 0x30 => Mapper::LoRom,
        value => return Err(IdentityError::UnsupportedMapMode(value)),
    };
    if !mapper_supports_image_len(mapper, bytes.len()) {
        return Err(IdentityError::UnsupportedRomSize {
            mapper,
            logical_len: bytes.len(),
        });
    }
    let checksum_offset = internal_header_offset + 0x1c;
    let stored_checksum = SnesChecksum::decode(bytes, checksum_offset)
        .map_err(|_| IdentityError::InvalidChecksumField)?;
    let computed_checksum = compute_snes_checksum(bytes, checksum_offset)
        .map_err(|_| IdentityError::InvalidChecksumField)?;
    Ok(RomIdentity {
        game,
        mapper,
        region,
        revision,
        map_mode,
        cartridge_type: header[0x16],
        internal_header_offset,
        stored_checksum,
        computed_checksum,
    })
}

fn locate_internal_header(bytes: &[u8]) -> Result<(usize, bool), IdentityError> {
    if bytes.len() <= 0x40_0000 {
        return Ok((LOW_HEADER_OFFSET, false));
    }

    let low_is_supported = has_supported_header(bytes, LOW_HEADER_OFFSET);
    let high_is_supported = has_supported_header(bytes, HIGH_HEADER_OFFSET);
    match (low_is_supported, high_is_supported) {
        (true, false) => Ok((LOW_HEADER_OFFSET, true)),
        (false, true) => Ok((HIGH_HEADER_OFFSET, false)),
        (false, false) => {
            // Preserve the recovered Lunar Magic fallback so callers receive the useful
            // unsupported-title diagnostic from the high header.
            Ok((HIGH_HEADER_OFFSET, false))
        }
        (true, true) => {
            let low_valid = checksum_is_complementary(bytes, LOW_HEADER_OFFSET);
            let high_valid = checksum_is_complementary(bytes, HIGH_HEADER_OFFSET);
            match (low_valid, high_valid) {
                (true, false) => Ok((LOW_HEADER_OFFSET, true)),
                (false, true) => Ok((HIGH_HEADER_OFFSET, false)),
                _ => Err(IdentityError::AmbiguousInternalHeader),
            }
        }
    }
}

fn has_supported_header(bytes: &[u8], offset: usize) -> bool {
    let Some(header) = bytes.get(offset..offset + INTERNAL_HEADER_LEN) else {
        return false;
    };
    let title = &header[..TITLE_LEN];
    let game = if title == SMW_TITLE {
        SupportedGame::SuperMarioWorld
    } else if title == ALL_STARS_WORLD_TITLE {
        SupportedGame::AllStarsAndWorld
    } else {
        return false;
    };
    (matches!(header[0x19], 1) || (game == SupportedGame::SuperMarioWorld && header[0x19] == 0))
        && header[0x1b] == 0
}

fn checksum_is_complementary(bytes: &[u8], header_offset: usize) -> bool {
    SnesChecksum::decode(bytes, header_offset + 0x1c).is_ok_and(SnesChecksum::is_complementary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smw_rom(len: usize, map_mode: u8) -> RomImage {
        let mut bytes = vec![0; len];
        bytes[LOW_HEADER_OFFSET..LOW_HEADER_OFFSET + TITLE_LEN].copy_from_slice(SMW_TITLE);
        bytes[LOW_HEADER_OFFSET + 0x15] = map_mode;
        bytes[LOW_HEADER_OFFSET + 0x19] = 1;
        bytes[LOW_HEADER_OFFSET + 0x1b] = 0;
        let checksum = compute_snes_checksum(&bytes, LOW_HEADER_OFFSET + 0x1c).unwrap();
        bytes[LOW_HEADER_OFFSET + 0x1c..LOW_HEADER_OFFSET + 0x20]
            .copy_from_slice(&checksum.encoded());
        RomImage::from_bytes(bytes).unwrap()
    }

    #[test]
    fn recognizes_lorom_and_sa1_headers() {
        let lorom = detect_identity(&smw_rom(0x8000, 0x20)).unwrap();
        assert_eq!(lorom.mapper, Mapper::LoRom);
        assert!(lorom.checksum_matches());

        let sa1 = detect_identity(&smw_rom(0x8000, 0x23)).unwrap();
        assert_eq!(sa1.mapper, Mapper::Sa1);

        let fast_lorom = detect_identity(&smw_rom(0x8000, 0x30)).unwrap();
        assert_eq!(fast_lorom.mapper, Mapper::LoRom);
    }

    #[test]
    fn map_mode_explicitly_selects_small_exlorom_and_rejects_hirom() {
        let exlorom = detect_identity(&smw_rom(0x8000, 0x32)).unwrap();
        assert_eq!(exlorom.mapper, Mapper::ExLoRom);
        assert_eq!(
            detect_identity(&smw_rom(0x8000, 0x21)),
            Err(IdentityError::UnsupportedMapMode(0x21))
        );
    }

    #[test]
    fn mapper_must_represent_the_complete_logical_image() {
        let oversized_sa1 = smw_rom(0x40_8000, 0x23);
        assert_eq!(
            detect_identity(&oversized_sa1),
            Err(IdentityError::UnsupportedRomSize {
                mapper: Mapper::Sa1,
                logical_len: 0x40_8000,
            })
        );

        let mut headered = vec![0x55; 0x200];
        headered.extend_from_slice(smw_rom(0x8000, 0x20).logical_bytes());
        assert_eq!(
            detect_identity(&RomImage::from_bytes(headered).unwrap())
                .unwrap()
                .mapper,
            Mapper::LoRom
        );
    }

    #[test]
    fn identity_rejects_partial_mapper_banks() {
        for (map_mode, mapper) in [
            (0x20, Mapper::LoRom),
            (0x30, Mapper::LoRom),
            (0x32, Mapper::ExLoRom),
            (0x23, Mapper::Sa1),
        ] {
            let rom = smw_rom(0x8001, map_mode);
            assert_eq!(
                detect_identity(&rom),
                Err(IdentityError::UnsupportedRomSize {
                    mapper,
                    logical_len: 0x8001,
                })
            );
        }
    }

    #[test]
    fn complementary_low_header_selects_exlorom() {
        let identity = detect_identity(&smw_rom(0x40_8000, 0x32)).unwrap();
        assert_eq!(identity.mapper, Mapper::ExLoRom);
        assert_eq!(identity.internal_header_offset, LOW_HEADER_OFFSET);
    }

    #[test]
    fn damaged_exlorom_checksum_does_not_redirect_header_detection() {
        let mut rom = smw_rom(0x40_8000, 0x32);
        rom.write(LOW_HEADER_OFFSET + 0x1c, &[0, 0, 0, 0]).unwrap();
        let identity = detect_identity(&rom).unwrap();
        assert_eq!(identity.mapper, Mapper::ExLoRom);
        assert_eq!(identity.internal_header_offset, LOW_HEADER_OFFSET);
        assert!(!identity.checksum_matches());
    }

    #[test]
    fn ambiguous_expanded_headers_are_rejected_without_guessing() {
        let mut bytes = smw_rom(0x40_8000, 0x32).logical_bytes().to_vec();
        bytes[HIGH_HEADER_OFFSET..HIGH_HEADER_OFFSET + TITLE_LEN].copy_from_slice(SMW_TITLE);
        bytes[HIGH_HEADER_OFFSET + 0x19] = 1;
        bytes[HIGH_HEADER_OFFSET + 0x1b] = 0;
        bytes[LOW_HEADER_OFFSET + 0x1c..LOW_HEADER_OFFSET + 0x20].fill(0);
        bytes[HIGH_HEADER_OFFSET + 0x1c..HIGH_HEADER_OFFSET + 0x20].fill(0);
        assert_eq!(
            detect_identity(&RomImage::from_bytes(bytes).unwrap()),
            Err(IdentityError::AmbiguousInternalHeader)
        );
    }

    #[test]
    fn arbitrary_rom_is_rejected() {
        assert!(matches!(
            detect_identity(&RomImage::from_bytes(vec![0; 0x8000]).unwrap()),
            Err(IdentityError::UnsupportedTitle(_))
        ));
    }
}
