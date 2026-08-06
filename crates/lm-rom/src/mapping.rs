use crate::RomError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mapper {
    LoRom,
    ExLoRom,
    Sa1,
}

pub const MAPPER_BANK_LEN: usize = 0x8000;

/// Reports whether a complete logical ROM image has valid bank shape and mapper addressability.
#[must_use]
pub fn mapper_supports_image_len(mapper: Mapper, logical_len: usize) -> bool {
    logical_len != 0
        && logical_len % MAPPER_BANK_LEN == 0
        && match mapper {
            // A 64-Mbit ExLoROM file physically includes the final two 32-KiB banks even though
            // CPU banks $7E/$7F are Work RAM and those offsets cannot become payload pointers.
            // Lunar Magic reserves both physical banks with explicit NULL-bank RATS locks.
            Mapper::ExLoRom => logical_len <= 0x0080_0000,
            Mapper::LoRom | Mapper::Sa1 => pc_to_snes(mapper, logical_len - 1).is_ok(),
        }
}

/// Converts a 24-bit SNES bus address to a headerless file offset.
///
/// # Errors
///
/// Returns [`RomError`] when the address is invalid for the selected mapping.
pub fn snes_to_pc(mapper: Mapper, address: u32) -> Result<usize, RomError> {
    if address > 0x00ff_ffff {
        return Err(RomError::InvalidSnesAddress(address));
    }
    let bank = (address >> 16) & 0xff;
    if bank == 0x7e || bank == 0x7f {
        return Err(RomError::InvalidSnesAddress(address));
    }
    let word = address & 0xffff;
    match mapper {
        Mapper::LoRom => {
            if word < 0x8000 {
                return Err(RomError::InvalidSnesAddress(address));
            }
            Ok((usize::try_from((address >> 16) & 0x7f).unwrap_or(0) << 15)
                | usize::try_from(word & 0x7fff).unwrap_or(0))
        }
        Mapper::ExLoRom => {
            if word < 0x8000 {
                return Err(RomError::InvalidSnesAddress(address));
            }
            let bank = (address >> 16) & 0xff;
            let low = (usize::try_from(bank & 0x7f).unwrap_or(0) << 15)
                | usize::try_from(word & 0x7fff).unwrap_or(0);
            Ok(if bank < 0x80 { low + 0x0040_0000 } else { low })
        }
        Mapper::Sa1 => {
            if bank >= 0xc0 {
                Ok(usize::try_from(address & 0x003f_ffff).unwrap_or(0))
            } else {
                if (0x40..=0x7d).contains(&bank) {
                    return Err(RomError::InvalidSnesAddress(address));
                }
                if word < 0x8000 {
                    return Err(RomError::InvalidSnesAddress(address));
                }
                Ok(usize::try_from((address >> 1) & 0x001f_8000 | address & 0x7fff).unwrap_or(0))
            }
        }
    }
}

/// Converts a headerless file offset to a canonical 24-bit SNES address.
///
/// # Errors
///
/// Returns [`RomError`] when the offset cannot be represented by the selected mapping.
pub fn pc_to_snes(mapper: Mapper, offset: usize) -> Result<u32, RomError> {
    let offset32 = u32::try_from(offset).map_err(|_| RomError::UnrepresentablePcOffset(offset))?;
    match mapper {
        Mapper::LoRom => {
            if offset >= 0x0040_0000 {
                return Err(RomError::UnrepresentablePcOffset(offset));
            }
            Ok((((offset32 >> 15) | 0x80) << 16) | 0x8000 | (offset32 & 0x7fff))
        }
        Mapper::ExLoRom => {
            if offset >= 0x0080_0000 {
                return Err(RomError::UnrepresentablePcOffset(offset));
            }
            // The final 64 KiB of the nominal 8 MiB layout would require CPU banks 7E/7F,
            // which are Work RAM rather than cartridge space.
            if offset >= 0x007f_0000 {
                return Err(RomError::UnrepresentablePcOffset(offset));
            }
            let (base, local) = if offset < 0x0040_0000 {
                (0x80, offset32)
            } else {
                (0, offset32 - 0x0040_0000)
            };
            Ok(((base | (local >> 15)) << 16) | 0x8000 | (local & 0x7fff))
        }
        Mapper::Sa1 => {
            if offset >= 0x0040_0000 {
                return Err(RomError::UnrepresentablePcOffset(offset));
            }
            if offset < 0x0020_0000 {
                Ok(((offset32 & 0x001f_8000) << 1) | 0x8000 | (offset32 & 0x7fff))
            } else {
                Ok(0x00c0_0000 | offset32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOROM_LEN: usize = 0x0040_0000;
    const EXLOROM_ADDRESSABLE_LEN: usize = 0x007f_0000;

    #[test]
    fn mappings_round_trip() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            let max = if mapper == Mapper::ExLoRom {
                0x007e_ffff
            } else {
                0x003f_ffff
            };
            for offset in [0, 0x7fff, 0x8000, 0x001f_ffff, 0x0020_0000, max] {
                assert_eq!(
                    snes_to_pc(mapper, pc_to_snes(mapper, offset).unwrap()).unwrap(),
                    offset
                );
            }
        }
    }

    #[test]
    fn rejects_wram_hardware_and_mapping_edges() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            assert!(snes_to_pc(mapper, 0x7e_8000).is_err());
            assert!(snes_to_pc(mapper, 0x7f_ffff).is_err());
        }
        assert!(snes_to_pc(Mapper::LoRom, 0x00_7fff).is_err());
        assert!(snes_to_pc(Mapper::ExLoRom, 0x80_0000).is_err());
        assert_eq!(snes_to_pc(Mapper::Sa1, 0xc0_0000).unwrap(), 0);
        assert_eq!(snes_to_pc(Mapper::Sa1, 0xff_ffff).unwrap(), 0x3f_ffff);
        assert_eq!(snes_to_pc(Mapper::Sa1, 0x80_8000).unwrap(), 0);
        for address in [0x40_0000, 0x40_8000, 0x60_ffff, 0x7d_ffff] {
            assert_eq!(
                snes_to_pc(Mapper::Sa1, address),
                Err(RomError::InvalidSnesAddress(address))
            );
        }
        assert!(pc_to_snes(Mapper::LoRom, 0x40_0000).is_err());
        assert!(pc_to_snes(Mapper::ExLoRom, 0x80_0000).is_err());
        assert!(pc_to_snes(Mapper::ExLoRom, 0x7f_0000).is_err());
        assert!(pc_to_snes(Mapper::Sa1, 0x40_0000).is_err());
    }

    #[test]
    fn every_representable_pc_offset_round_trips_through_its_canonical_address() {
        for (mapper, len) in [
            (Mapper::LoRom, LOROM_LEN),
            (Mapper::ExLoRom, EXLOROM_ADDRESSABLE_LEN),
            (Mapper::Sa1, LOROM_LEN),
        ] {
            for offset in 0..len {
                let address = pc_to_snes(mapper, offset).unwrap();
                assert_eq!(snes_to_pc(mapper, address), Ok(offset));
            }
        }
    }

    #[test]
    fn canonical_addresses_use_the_expected_bank_windows() {
        for offset in [0, 1, 0x7fff, 0x8000, 0x3f_ffff] {
            // FastROM map modes use the same physical LoROM address conversion.  The canonical
            // form deliberately selects the fast 80-FF bank mirror.
            let address = pc_to_snes(Mapper::LoRom, offset).unwrap();
            assert!((0x80..=0xff).contains(&(address >> 16)));
            assert!(address & 0xffff >= 0x8000);
        }

        for offset in [0, 0x3f_ffff] {
            assert!((0x80..=0xff).contains(&(pc_to_snes(Mapper::ExLoRom, offset).unwrap() >> 16)));
        }
        for offset in [0x40_0000, 0x7e_ffff] {
            assert!((0x00..=0x7d).contains(&(pc_to_snes(Mapper::ExLoRom, offset).unwrap() >> 16)));
        }

        assert_eq!(pc_to_snes(Mapper::Sa1, 0x1f_ffff), Ok(0x3f_ffff));
        assert_eq!(pc_to_snes(Mapper::Sa1, 0x20_0000), Ok(0xe0_0000));
    }

    #[test]
    fn all_bus_addresses_either_reject_or_resolve_to_representable_storage() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            for address in 0..=0x00ff_ffff {
                if let Ok(offset) = snes_to_pc(mapper, address) {
                    assert!(pc_to_snes(mapper, offset).is_ok());
                }
            }
        }
    }

    #[test]
    fn exlorom_stops_before_the_wram_backed_nominal_tail() {
        assert_eq!(
            pc_to_snes(Mapper::ExLoRom, EXLOROM_ADDRESSABLE_LEN - 1),
            Ok(0x7d_ffff)
        );
        for offset in EXLOROM_ADDRESSABLE_LEN..0x0080_0000 {
            assert_eq!(
                pc_to_snes(Mapper::ExLoRom, offset),
                Err(RomError::UnrepresentablePcOffset(offset))
            );
        }
    }

    #[test]
    fn complete_image_lengths_require_whole_addressable_banks() {
        for mapper in [Mapper::LoRom, Mapper::ExLoRom, Mapper::Sa1] {
            assert!(!mapper_supports_image_len(mapper, 0));
            assert!(!mapper_supports_image_len(mapper, 0x8001));
            assert!(mapper_supports_image_len(mapper, 0x8000));
        }
        assert!(mapper_supports_image_len(
            Mapper::ExLoRom,
            EXLOROM_ADDRESSABLE_LEN
        ));
        assert!(mapper_supports_image_len(
            Mapper::ExLoRom,
            EXLOROM_ADDRESSABLE_LEN + MAPPER_BANK_LEN
        ));
        assert!(mapper_supports_image_len(Mapper::ExLoRom, 0x0080_0000));
        assert!(!mapper_supports_image_len(Mapper::ExLoRom, 0x0080_8000));
    }
}
