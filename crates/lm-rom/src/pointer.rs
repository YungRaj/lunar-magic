use crate::{Mapper, RomError, snes_to_pc};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SnesPointer24(u32);

impl SnesPointer24 {
    pub const ENCODED_LEN: usize = 3;

    /// Constructs a 24-bit bus pointer.
    ///
    /// # Errors
    ///
    /// Returns the supplied value if it exceeds 24 bits.
    pub const fn new(value: u32) -> Result<Self, u32> {
        if value <= 0x00ff_ffff {
            Ok(Self(value))
        } else {
            Err(value)
        }
    }

    /// Decodes a little-endian 24-bit pointer.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless exactly three bytes are provided.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(bytes.len());
        }
        Ok(Self(
            u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16,
        ))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn encode(self) -> [u8; Self::ENCODED_LEN] {
        let bytes = self.0.to_le_bytes();
        [bytes[0], bytes[1], bytes[2]]
    }

    /// Resolves the pointer through the centralized mapper implementation.
    ///
    /// # Errors
    ///
    /// Returns [`RomError`] for WRAM, hardware, or unmapped addresses.
    pub fn to_pc(self, mapper: Mapper) -> Result<usize, RomError> {
        snes_to_pc(mapper, self.0)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PointerTable24 {
    pub pointers: Vec<SnesPointer24>,
}

impl PointerTable24 {
    /// Decodes a complete three-byte pointer table.
    ///
    /// # Errors
    ///
    /// Returns the supplied length if it contains a partial entry.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() % SnesPointer24::ENCODED_LEN != 0 {
            return Err(bytes.len());
        }
        Ok(Self {
            pointers: bytes
                .chunks_exact(SnesPointer24::ENCODED_LEN)
                .map(SnesPointer24::decode)
                .collect::<Result<_, _>>()?,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.pointers
            .iter()
            .flat_map(|pointer| pointer.encode())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_table_round_trips_and_resolves() {
        let bytes = [0x00, 0x80, 0x80, 0xff, 0xff, 0x81];
        let table = PointerTable24::decode(&bytes).unwrap();
        assert_eq!(table.encode(), bytes);
        assert_eq!(table.pointers[0].to_pc(Mapper::LoRom).unwrap(), 0);
        assert_eq!(table.pointers[1].to_pc(Mapper::LoRom).unwrap(), 0xffff);
    }
}
