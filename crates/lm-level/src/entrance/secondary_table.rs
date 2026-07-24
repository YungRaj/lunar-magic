use super::validation::validate_secondary_exit;
use super::{SecondaryExit, SecondaryExitEncodingError, SecondaryExitTable};

impl SecondaryExitTable {
    pub const ENTRY_COUNT: usize = 0x2000;
    pub const PLANE_COUNT: usize = 6;

    /// Decodes Lunar Magic's six parallel 8,192-byte secondary-exit planes.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless the complete native table is present.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() != Self::ENTRY_COUNT * Self::PLANE_COUNT {
            return Err(bytes.len());
        }
        let plane = |number: usize, index: usize| bytes[number * Self::ENTRY_COUNT + index];
        let entries = (0..Self::ENTRY_COUNT)
            .map(|index| {
                let high_and_flags = plane(3, index);
                SecondaryExit {
                    destination_level: u16::from(plane(0, index))
                        | (u16::from(high_and_flags & 8) << 5),
                    position_and_method: plane(1, index),
                    screen: plane(2, index) >> 4,
                    y: plane(2, index) & 0x0f,
                    x: plane(4, index) & 0x0f,
                    destination_flags: high_and_flags & !8,
                    x_and_overworld_flags: plane(4, index) & 0xf0,
                    additional_flags: plane(5, index),
                }
            })
            .collect();
        Ok(Self { entries })
    }

    /// Encodes the exact native six-plane table without truncating public fields.
    ///
    /// # Errors
    ///
    /// Rejects a non-native entry count or any field outside its packed representation.
    pub fn encode(&self) -> Result<Vec<u8>, SecondaryExitEncodingError> {
        if self.entries.len() != Self::ENTRY_COUNT {
            return Err(SecondaryExitEncodingError::WrongEntryCount {
                actual: self.entries.len(),
                expected: Self::ENTRY_COUNT,
            });
        }
        let mut bytes = vec![0; Self::ENTRY_COUNT * Self::PLANE_COUNT];
        for (index, entry) in self.entries.iter().enumerate() {
            validate_secondary_exit(entry, index)?;
            bytes[index] = entry.destination_level.to_le_bytes()[0];
            bytes[Self::ENTRY_COUNT + index] = entry.position_and_method;
            bytes[Self::ENTRY_COUNT * 2 + index] = entry.screen << 4 | entry.y;
            bytes[Self::ENTRY_COUNT * 3 + index] = entry.destination_flags
                | if entry.destination_level & 0x100 != 0 {
                    8
                } else {
                    0
                };
            bytes[Self::ENTRY_COUNT * 4 + index] = entry.x_and_overworld_flags | entry.x;
            bytes[Self::ENTRY_COUNT * 5 + index] = entry.additional_flags;
        }
        Ok(bytes)
    }
}
