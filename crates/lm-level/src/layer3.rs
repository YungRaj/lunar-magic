//! Lossless semantic Layer 3 state and portable interchange.

use crate::{BinaryError, ByteCursor};

const MAGIC: &[u8; 8] = b"LMLAY3V1";
const MAX_TILEMAP_BYTES: usize = 0x2000;
const MAX_REMAP_BYTES: usize = 0x1_0000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3Settings {
    /// Revision-specific selector byte. Unknown values are intentionally preserved.
    pub start_position: u8,
    /// Revision-specific tilemap-size selector byte.
    pub tilemap_size: u8,
    /// Revision-specific liquid/type selector byte.
    pub liquid_type: u8,
    /// Feature/configuration bits not yet assigned stable semantic names.
    pub flags: u8,
    /// Four recovered 12-bit Layer 3 graphics-file identifiers.
    pub graphics_files: [u16; 4],
    /// Lossless revision-specific settings bytes.
    pub reserved: [u8; 16],
}

impl Default for Layer3Settings {
    fn default() -> Self {
        Self {
            start_position: 0,
            tilemap_size: 0,
            liquid_type: 0,
            flags: 0,
            graphics_files: [0xfff; 4],
            reserved: [0; 16],
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Layer3Data {
    pub settings: Layer3Settings,
    /// Raw decoded tilemap workspace; Lunar Magic's recovered buffer is at most 0x2000 bytes.
    pub tilemap: Vec<u8>,
    /// Literal/repeated remap command bytes retained until every opcode is oracle-verified.
    pub remap_commands: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3File(pub Layer3Data);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3Error {
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    TilemapTooLarge(usize),
    RemapTooLarge(usize),
    GraphicsFileOutOfRange { slot: usize, value: u16 },
    Binary(BinaryError),
    Overflow,
}

impl std::fmt::Display for Layer3Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Layer 3 interchange error: {self:?}")
    }
}

impl std::error::Error for Layer3Error {}

impl From<BinaryError> for Layer3Error {
    fn from(value: BinaryError) -> Self {
        Self::Binary(value)
    }
}

impl Layer3Data {
    /// Validates recovered native limits while preserving unknown selectors and flags.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3Error`] for oversized buffers or a graphics identifier above 12 bits.
    pub fn validate(&self) -> Result<(), Layer3Error> {
        if self.tilemap.len() > MAX_TILEMAP_BYTES {
            return Err(Layer3Error::TilemapTooLarge(self.tilemap.len()));
        }
        if self.remap_commands.len() > MAX_REMAP_BYTES {
            return Err(Layer3Error::RemapTooLarge(self.remap_commands.len()));
        }
        for (slot, value) in self.settings.graphics_files.iter().copied().enumerate() {
            if value > 0xfff {
                return Err(Layer3Error::GraphicsFileOutOfRange { slot, value });
            }
        }
        Ok(())
    }
}

impl Layer3File {
    pub const MAX_ENCODED_LEN: usize =
        MAGIC.len() + 4 + 8 + 16 + 4 + MAX_TILEMAP_BYTES + 4 + MAX_REMAP_BYTES;

    /// Encodes a canonical standalone `LMLAY3V1` artifact.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3Error`] when recovered limits are exceeded.
    pub fn encode(&self) -> Result<Vec<u8>, Layer3Error> {
        self.0.validate()?;
        let mut output = MAGIC.to_vec();
        let settings = &self.0.settings;
        output.extend_from_slice(&[
            settings.start_position,
            settings.tilemap_size,
            settings.liquid_type,
            settings.flags,
        ]);
        for file in settings.graphics_files {
            output.extend_from_slice(&file.to_le_bytes());
        }
        output.extend_from_slice(&settings.reserved);
        push_len(&mut output, self.0.tilemap.len())?;
        output.extend_from_slice(&self.0.tilemap);
        push_len(&mut output, self.0.remap_commands.len())?;
        output.extend_from_slice(&self.0.remap_commands);
        Ok(output)
    }

    /// Decodes one complete bounded `LMLAY3V1` artifact.
    ///
    /// # Errors
    ///
    /// Returns [`Layer3Error`] for malformed framing, truncation, trailing bytes, invalid graphics
    /// identifiers, or exceeded recovered limits.
    pub fn decode(bytes: &[u8]) -> Result<Self, Layer3Error> {
        if bytes.len() > Self::MAX_ENCODED_LEN {
            return Err(Layer3Error::RemapTooLarge(bytes.len()));
        }
        let mut input = ByteCursor::new(bytes);
        if input.take(MAGIC.len())? != MAGIC {
            return Err(Layer3Error::WrongMagic);
        }
        let mut graphics_files = [0; 4];
        let start_position = input.u8()?;
        let tilemap_size = input.u8()?;
        let liquid_type = input.u8()?;
        let flags = input.u8()?;
        for file in &mut graphics_files {
            *file = input.u16_le()?;
        }
        let mut reserved = [0; 16];
        reserved.copy_from_slice(input.take(16)?);
        let tilemap_len = read_len(&mut input)?;
        if tilemap_len > MAX_TILEMAP_BYTES {
            return Err(Layer3Error::TilemapTooLarge(tilemap_len));
        }
        let tilemap = input.take(tilemap_len)?.to_vec();
        let remap_len = read_len(&mut input)?;
        if remap_len > MAX_REMAP_BYTES {
            return Err(Layer3Error::RemapTooLarge(remap_len));
        }
        let remap_commands = input.take(remap_len)?.to_vec();
        if input.remaining() != 0 {
            return Err(Layer3Error::TrailingBytes(input.remaining()));
        }
        let value = Self(Layer3Data {
            settings: Layer3Settings {
                start_position,
                tilemap_size,
                liquid_type,
                flags,
                graphics_files,
                reserved,
            },
            tilemap,
            remap_commands,
        });
        value.0.validate()?;
        Ok(value)
    }
}

fn push_len(output: &mut Vec<u8>, len: usize) -> Result<(), Layer3Error> {
    output.extend_from_slice(
        &u32::try_from(len)
            .map_err(|_| Layer3Error::Overflow)?
            .to_le_bytes(),
    );
    Ok(())
}

fn read_len(input: &mut ByteCursor<'_>) -> Result<usize, Layer3Error> {
    usize::try_from(input.u32_le()?).map_err(|_| Layer3Error::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Layer3Data {
        Layer3Data {
            settings: Layer3Settings {
                start_position: 0xfe,
                tilemap_size: 3,
                liquid_type: 0x81,
                flags: 0xa5,
                graphics_files: [0, 0x123, 0xabc, 0xfff],
                reserved: [0x5a; 16],
            },
            tilemap: (0..=255).cycle().take(0x2000).collect(),
            remap_commands: vec![0, 1, 0xff, 0x80],
        }
    }

    #[test]
    fn exact_maximum_tilemap_and_unknown_fields_round_trip() {
        let expected = Layer3File(data());
        let bytes = expected.encode().unwrap();
        assert_eq!(Layer3File::decode(&bytes).unwrap(), expected);
        assert_eq!(Layer3File::decode(&bytes).unwrap().encode().unwrap(), bytes);
    }

    #[test]
    fn every_truncation_and_trailing_data_is_rejected() {
        let bytes = Layer3File(data()).encode().unwrap();
        for end in 0..bytes.len() {
            assert!(Layer3File::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            Layer3File::decode(&trailing),
            Err(Layer3Error::TrailingBytes(1))
        );
    }

    #[test]
    fn recovered_limits_are_enforced_before_encoding() {
        let mut invalid = data();
        invalid.settings.graphics_files[2] = 0x1000;
        assert_eq!(
            invalid.validate(),
            Err(Layer3Error::GraphicsFileOutOfRange {
                slot: 2,
                value: 0x1000
            })
        );
        invalid = data();
        invalid.tilemap.push(0);
        assert_eq!(
            invalid.validate(),
            Err(Layer3Error::TilemapTooLarge(0x2001))
        );
    }
}
