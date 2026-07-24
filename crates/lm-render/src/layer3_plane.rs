//! Provider-resolved Layer 3 tile planes for deterministic rendering.

use crate::TileInstance;
use std::fmt;

const MAGIC: &[u8; 8] = b"LML3FRM1";
const HEADER_LEN: usize = 48;
const ENTRY_LEN: usize = 16;
const MAX_INSTANCES: usize = 0x1_0000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Layer3Placement {
    BehindLayer2,
    BetweenLayer2AndLayer1,
    AboveLayer1,
    AboveEntities,
}

impl Layer3Placement {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::BehindLayer2),
            1 => Some(Self::BetweenLayer2AndLayer1),
            2 => Some(Self::AboveLayer1),
            3 => Some(Self::AboveEntities),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedLayer3Plane {
    /// SHA-256 of the canonical `LMLAY3V1` source used by the provider.
    pub source_digest: [u8; 32],
    pub placement: Layer3Placement,
    pub instances: Vec<TileInstance>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializedLayer3Error {
    Truncated,
    WrongMagic,
    NonZeroReserved,
    UnknownPlacement(u8),
    TooManyInstances(usize),
    WrongLength { expected: usize, actual: usize },
    InvalidFlags { index: usize, flags: u8 },
    PaletteOutOfRange { index: usize, palette: usize },
    Overflow,
}

impl fmt::Display for MaterializedLayer3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "materialized Layer 3 plane error: {self:?}")
    }
}

impl std::error::Error for MaterializedLayer3Error {}

impl MaterializedLayer3Plane {
    pub const MAX_FILE_LEN: usize = HEADER_LEN + MAX_INSTANCES * ENTRY_LEN;

    /// Encodes a canonical bounded `LML3FRM1` artifact in painter order.
    ///
    /// # Errors
    ///
    /// Returns [`MaterializedLayer3Error`] for excessive instances, palette indexes that do not
    /// fit the portable format, or arithmetic overflow.
    pub fn encode(&self) -> Result<Vec<u8>, MaterializedLayer3Error> {
        validate_count(self.instances.len())?;
        let expected = encoded_len(self.instances.len())?;
        let mut output = Vec::with_capacity(expected);
        output.extend_from_slice(MAGIC);
        output.push(self.placement as u8);
        output.extend_from_slice(&[0; 3]);
        output.extend_from_slice(
            &u32::try_from(self.instances.len())
                .map_err(|_| MaterializedLayer3Error::Overflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(&self.source_digest);
        for (index, instance) in self.instances.iter().enumerate() {
            output.extend_from_slice(
                &u32::try_from(instance.tile_index)
                    .map_err(|_| MaterializedLayer3Error::Overflow)?
                    .to_le_bytes(),
            );
            let palette = u16::try_from(instance.palette_index).map_err(|_| {
                MaterializedLayer3Error::PaletteOutOfRange {
                    index,
                    palette: instance.palette_index,
                }
            })?;
            output.extend_from_slice(&palette.to_le_bytes());
            output.extend_from_slice(&instance.x.to_le_bytes());
            output.extend_from_slice(&instance.y.to_le_bytes());
            output.push(u8::from(instance.x_flip) | u8::from(instance.y_flip) << 1);
            output.push(0);
        }
        Ok(output)
    }

    /// Decodes one complete `LML3FRM1` artifact.
    ///
    /// # Errors
    ///
    /// Returns [`MaterializedLayer3Error`] for malformed framing, unknown placement/flag values,
    /// excessive counts, length overflow, or trailing/truncated data.
    pub fn decode(bytes: &[u8]) -> Result<Self, MaterializedLayer3Error> {
        let header = bytes
            .get(..HEADER_LEN)
            .ok_or(MaterializedLayer3Error::Truncated)?;
        if &header[..8] != MAGIC {
            return Err(MaterializedLayer3Error::WrongMagic);
        }
        let placement = Layer3Placement::from_byte(header[8])
            .ok_or(MaterializedLayer3Error::UnknownPlacement(header[8]))?;
        if header[9..12] != [0; 3] {
            return Err(MaterializedLayer3Error::NonZeroReserved);
        }
        let count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| MaterializedLayer3Error::Overflow)?;
        validate_count(count)?;
        let expected = encoded_len(count)?;
        if bytes.len() != expected {
            return Err(MaterializedLayer3Error::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let mut source_digest = [0; 32];
        source_digest.copy_from_slice(&header[16..48]);
        let mut instances = Vec::with_capacity(count);
        for (index, entry) in bytes[HEADER_LEN..].chunks_exact(ENTRY_LEN).enumerate() {
            let flags = entry[14];
            if flags & !3 != 0 {
                return Err(MaterializedLayer3Error::InvalidFlags { index, flags });
            }
            if entry[15] != 0 {
                return Err(MaterializedLayer3Error::NonZeroReserved);
            }
            instances.push(TileInstance {
                tile_index: usize::try_from(read_u32(entry, 0))
                    .map_err(|_| MaterializedLayer3Error::Overflow)?,
                palette_index: usize::from(u16::from_le_bytes([entry[4], entry[5]])),
                x: i32::from_le_bytes([entry[6], entry[7], entry[8], entry[9]]),
                y: i32::from_le_bytes([entry[10], entry[11], entry[12], entry[13]]),
                x_flip: flags & 1 != 0,
                y_flip: flags & 2 != 0,
            });
        }
        Ok(Self {
            source_digest,
            placement,
            instances,
        })
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn validate_count(count: usize) -> Result<(), MaterializedLayer3Error> {
    if count > MAX_INSTANCES {
        Err(MaterializedLayer3Error::TooManyInstances(count))
    } else {
        Ok(())
    }
}

fn encoded_len(count: usize) -> Result<usize, MaterializedLayer3Error> {
    count
        .checked_mul(ENTRY_LEN)
        .and_then(|len| HEADER_LEN.checked_add(len))
        .ok_or(MaterializedLayer3Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> MaterializedLayer3Plane {
        MaterializedLayer3Plane {
            source_digest: [0x5a; 32],
            placement: Layer3Placement::BetweenLayer2AndLayer1,
            instances: vec![
                TileInstance {
                    tile_index: 0x123,
                    palette_index: 7,
                    x: -8,
                    y: 16,
                    x_flip: true,
                    y_flip: false,
                },
                TileInstance {
                    tile_index: 0x456,
                    palette_index: 9,
                    x: 24,
                    y: -32,
                    x_flip: false,
                    y_flip: true,
                },
            ],
        }
    }

    #[test]
    fn exact_signed_painter_order_round_trips() {
        let expected = plane();
        let bytes = expected.encode().unwrap();
        assert_eq!(MaterializedLayer3Plane::decode(&bytes).unwrap(), expected);
        assert_eq!(
            MaterializedLayer3Plane::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn truncation_trailing_reserved_unknown_and_flags_fail() {
        let bytes = plane().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(MaterializedLayer3Plane::decode(&bytes[..end]).is_err());
        }
        let mut malformed = bytes.clone();
        malformed.push(0);
        assert!(matches!(
            MaterializedLayer3Plane::decode(&malformed),
            Err(MaterializedLayer3Error::WrongLength { .. })
        ));
        malformed = bytes.clone();
        malformed[8] = 0xff;
        assert_eq!(
            MaterializedLayer3Plane::decode(&malformed),
            Err(MaterializedLayer3Error::UnknownPlacement(0xff))
        );
        malformed = bytes;
        malformed[HEADER_LEN + 14] = 4;
        assert_eq!(
            MaterializedLayer3Plane::decode(&malformed),
            Err(MaterializedLayer3Error::InvalidFlags { index: 0, flags: 4 })
        );
    }
}
