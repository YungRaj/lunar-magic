//! Lunar Magic's seven packed per-slot `ExAnimation` option bytes.

use std::fmt;

pub const EXANIMATION_LEVEL_SLOT_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationSlotOptions {
    /// Opaque low nibble retained verbatim by Lunar Magic's encoder.
    pub preserved_low_nibble: u8,
    /// Positive editor option states corresponding to packed bits 4, 5, 6, and 7.
    pub enabled: [bool; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationSlotOptionTable {
    pub slots: [ExAnimationSlotOptions; EXANIMATION_LEVEL_SLOT_COUNT],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExAnimationSlotOptionError {
    WrongLength(usize),
    LowNibbleOutOfRange { slot: usize, value: u8 },
}

impl fmt::Display for ExAnimationSlotOptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid ExAnimation slot-option table: {self:?}")
    }
}

impl std::error::Error for ExAnimationSlotOptionError {}

impl ExAnimationSlotOptionTable {
    /// Decodes the four inverted high option bits while retaining each low nibble.
    ///
    /// # Errors
    ///
    /// Requires Lunar Magic's exact seven-byte table.
    pub fn decode(bytes: &[u8]) -> Result<Self, ExAnimationSlotOptionError> {
        let bytes: &[u8; EXANIMATION_LEVEL_SLOT_COUNT] = bytes
            .try_into()
            .map_err(|_| ExAnimationSlotOptionError::WrongLength(bytes.len()))?;
        Ok(Self {
            slots: std::array::from_fn(|slot| {
                let packed = bytes[slot];
                ExAnimationSlotOptions {
                    preserved_low_nibble: packed & 0x0f,
                    enabled: std::array::from_fn(|option| packed & (0x10 << option) == 0),
                }
            }),
        })
    }

    /// Rebuilds the packed bytes using Lunar Magic's inverted high-bit convention.
    ///
    /// # Errors
    ///
    /// Rejects a preserved low-nibble value containing any high bits.
    pub fn encode(&self) -> Result<[u8; EXANIMATION_LEVEL_SLOT_COUNT], ExAnimationSlotOptionError> {
        let mut packed = [0; EXANIMATION_LEVEL_SLOT_COUNT];
        for (slot, options) in self.slots.iter().enumerate() {
            if options.preserved_low_nibble & 0xf0 != 0 {
                return Err(ExAnimationSlotOptionError::LowNibbleOutOfRange {
                    slot,
                    value: options.preserved_low_nibble,
                });
            }
            packed[slot] = options.preserved_low_nibble
                | u8::from(!options.enabled[0]) << 4
                | u8::from(!options.enabled[1]) << 5
                | u8::from(!options.enabled[2]) << 6
                | u8::from(!options.enabled[3]) << 7;
        }
        Ok(packed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_high_bit_combinations_and_low_nibbles_round_trip() {
        for packed in 0_u8..=u8::MAX {
            let bytes = [packed; EXANIMATION_LEVEL_SLOT_COUNT];
            let decoded = ExAnimationSlotOptionTable::decode(&bytes).unwrap();
            assert_eq!(decoded.encode().unwrap(), bytes);
        }
    }

    #[test]
    fn option_bits_use_the_recovered_inverted_polarity() {
        let decoded = ExAnimationSlotOptionTable::decode(&[0x50; 7]).unwrap();
        let first = decoded.slots[0];
        assert_eq!(first.enabled, [false, true, false, true]);
    }
}
