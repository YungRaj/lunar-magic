use crate::{RomError, RomImage};

#[must_use]
pub fn additive_checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)))
}

#[must_use]
pub const fn checksum_complement(checksum: u16) -> u16 {
    !checksum
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnesChecksum {
    pub complement: u16,
    pub checksum: u16,
}

impl SnesChecksum {
    pub const ENCODED_LEN: usize = 4;

    #[must_use]
    pub const fn encoded(self) -> [u8; Self::ENCODED_LEN] {
        let complement = self.complement.to_le_bytes();
        let checksum = self.checksum.to_le_bytes();
        [complement[0], complement[1], checksum[0], checksum[1]]
    }

    /// Parses the complement/checksum pair from an internal SNES header.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::RangeOutOfBounds`] unless four bytes are available.
    pub fn decode(bytes: &[u8], offset: usize) -> Result<Self, RomError> {
        let end = offset
            .checked_add(Self::ENCODED_LEN)
            .ok_or(RomError::RangeOutOfBounds {
                offset,
                len: Self::ENCODED_LEN,
                image_len: bytes.len(),
            })?;
        let fields = bytes.get(offset..end).ok_or(RomError::RangeOutOfBounds {
            offset,
            len: Self::ENCODED_LEN,
            image_len: bytes.len(),
        })?;
        Ok(Self {
            complement: u16::from_le_bytes([fields[0], fields[1]]),
            checksum: u16::from_le_bytes([fields[2], fields[3]]),
        })
    }

    #[must_use]
    pub const fn is_complementary(self) -> bool {
        self.complement ^ self.checksum == 0xffff
    }
}

/// Computes the checksum pair that should be stored at `field_offset`.
///
/// The four checksum bytes are normalized to a complementary placeholder first. Every valid
/// checksum/complement pair has the same additive contribution, making the result independent of
/// stale values currently stored in the ROM.
///
/// # Errors
///
/// Returns [`RomError::RangeOutOfBounds`] if the four-byte field is outside the image.
pub fn compute_snes_checksum(bytes: &[u8], field_offset: usize) -> Result<SnesChecksum, RomError> {
    let end =
        field_offset
            .checked_add(SnesChecksum::ENCODED_LEN)
            .ok_or(RomError::RangeOutOfBounds {
                offset: field_offset,
                len: SnesChecksum::ENCODED_LEN,
                image_len: bytes.len(),
            })?;
    if end > bytes.len() {
        return Err(RomError::RangeOutOfBounds {
            offset: field_offset,
            len: SnesChecksum::ENCODED_LEN,
            image_len: bytes.len(),
        });
    }
    let mut normalized = bytes.to_vec();
    normalized[field_offset..end].copy_from_slice(&[0xff, 0xff, 0x00, 0x00]);
    let checksum = mirrored_checksum(&normalized);
    Ok(SnesChecksum {
        complement: checksum_complement(checksum),
        checksum,
    })
}

impl RomImage {
    /// Recomputes and writes an internal-header checksum at a logical ROM offset.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::RangeOutOfBounds`] when the checksum field is outside the ROM.
    pub fn update_snes_checksum(
        &mut self,
        logical_field_offset: usize,
    ) -> Result<SnesChecksum, RomError> {
        let checksum = compute_snes_checksum(self.logical_bytes(), logical_field_offset)?;
        self.write(logical_field_offset, &checksum.encoded())?;
        Ok(checksum)
    }
}

/// Computes the SNES mirrored checksum for non-power-of-two ROM sizes.
#[must_use]
pub fn mirrored_checksum(bytes: &[u8]) -> u16 {
    if bytes.is_empty() {
        return 0;
    }
    let power = bytes.len().next_power_of_two() >> 1;
    if power == 0 || power == bytes.len() {
        return additive_checksum(bytes);
    }
    let remainder = &bytes[power..];
    let mirrored_span = remainder.len().next_power_of_two();
    let repeats = power / mirrored_span;
    // The checksum is a wrapping u16 sum, so only the low 16 bits of the conceptual repeat count
    // contribute. Express that modular arithmetic directly instead of treating a large, valid
    // repeat count as a failed integer conversion.
    let repeats_modulo = u16::try_from(repeats & usize::from(u16::MAX)).unwrap_or_default();
    additive_checksum(&bytes[..power])
        .wrapping_add(mirrored_checksum(remainder).wrapping_mul(repeats_modulo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_mirrored_bytes(bytes: &[u8]) -> Vec<u8> {
        if bytes.is_empty() || bytes.len().is_power_of_two() {
            return bytes.to_vec();
        }
        let leading_len = bytes.len().next_power_of_two() >> 1;
        let mut output = bytes[..leading_len].to_vec();
        let tail = reference_mirrored_bytes(&bytes[leading_len..]);
        while output.len() < bytes.len().next_power_of_two() {
            output.extend_from_slice(&tail);
        }
        output
    }
    #[test]
    fn complement_pairs() {
        let sum = additive_checksum(&[1, 2, 3]);
        assert_eq!(sum ^ checksum_complement(sum), 0xffff);
    }

    #[test]
    fn checksum_fields_are_stable_and_header_transparent() {
        let mut bytes = vec![0x12; 0x8000];
        bytes[0x7fdc..0x7fe0].copy_from_slice(&[1, 2, 3, 4]);
        let expected = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&expected.encoded());
        assert_eq!(compute_snes_checksum(&bytes, 0x7fdc).unwrap(), expected);

        let mut headered = vec![0; 0x200];
        headered.extend_from_slice(&bytes);
        let mut rom = RomImage::from_bytes(headered).unwrap();
        assert_eq!(rom.update_snes_checksum(0x7fdc).unwrap(), expected);
        assert_eq!(rom.read(0x7fdc, 4).unwrap(), expected.encoded());
    }

    #[test]
    fn irregular_tail_is_recursively_mirrored() {
        // 1,2,3,4 | 5,6,7 is mirrored conceptually as 1,2,3,4 | 5,6,7,7.
        assert_eq!(mirrored_checksum(&[1, 2, 3, 4, 5, 6, 7]), 35);
    }

    #[test]
    fn every_small_irregular_size_matches_an_independent_virtual_rom() {
        for len in 0_usize..=4096 {
            let bytes = (0..len)
                .map(|index| u8::try_from(index.wrapping_mul(73).wrapping_add(19) & 0xff).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                mirrored_checksum(&bytes),
                additive_checksum(&reference_mirrored_bytes(&bytes)),
                "logical length {len:#x}"
            );
        }
    }

    #[test]
    fn normalized_checksum_is_stable_when_the_field_lies_in_a_mirrored_tail() {
        let mut bytes = (0_usize..0x18_000)
            .map(|index| u8::try_from(index.wrapping_mul(29).wrapping_add(7) & 0xff).unwrap())
            .collect::<Vec<_>>();
        let field = 0x17_ffc;
        let expected = compute_snes_checksum(&bytes, field).unwrap();
        bytes[field..field + SnesChecksum::ENCODED_LEN].copy_from_slice(&expected.encoded());
        assert_eq!(compute_snes_checksum(&bytes, field), Ok(expected));
        assert_eq!(SnesChecksum::decode(&bytes, field), Ok(expected));
    }
}
