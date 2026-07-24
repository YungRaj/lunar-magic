use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearanceSource {
    Layer1Object(u32),
    Layer2Object(u32),
    Sprite(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityAppearanceRecord {
    pub source: AppearanceSource,
    pub tile_index: u16,
    pub palette_index: u8,
    pub x: i32,
    pub y: i32,
    pub x_flip: bool,
    pub y_flip: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityAppearanceFile {
    pub appearances: Vec<EntityAppearanceRecord>,
}

impl EntityAppearanceFile {
    pub const MAGIC: [u8; 8] = *b"LMENTAPP";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const RECORD_LEN: usize = 20;
    pub const MAX_APPEARANCES: usize = 0x10_0000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + Self::MAX_APPEARANCES * Self::RECORD_LEN;

    /// Encodes resolved object and sprite preview tiles in painter order.
    ///
    /// # Errors
    ///
    /// Returns [`EntityAppearanceFileError`] if the count is excessive, a palette is invalid, or
    /// an output length cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, EntityAppearanceFileError> {
        validate(&self.appearances)?;
        let count = u32::try_from(self.appearances.len())
            .map_err(|_| EntityAppearanceFileError::Overflow)?;
        let capacity = Self::HEADER_LEN
            .checked_add(
                self.appearances
                    .len()
                    .checked_mul(Self::RECORD_LEN)
                    .ok_or(EntityAppearanceFileError::Overflow)?,
            )
            .ok_or(EntityAppearanceFileError::Overflow)?;
        let mut output = Vec::with_capacity(capacity);
        output.extend_from_slice(&Self::MAGIC);
        output.extend_from_slice(&Self::VERSION.to_le_bytes());
        output.extend_from_slice(&[0; 2]);
        output.extend_from_slice(&count.to_le_bytes());
        for appearance in &self.appearances {
            let (kind, index) = match appearance.source {
                AppearanceSource::Layer1Object(index) => (0, index),
                AppearanceSource::Layer2Object(index) => (1, index),
                AppearanceSource::Sprite(index) => (2, index),
            };
            output.push(kind);
            output.push(u8::from(appearance.x_flip) | (u8::from(appearance.y_flip) << 1));
            output.extend_from_slice(&[0; 2]);
            output.extend_from_slice(&index.to_le_bytes());
            output.extend_from_slice(&appearance.tile_index.to_le_bytes());
            output.push(appearance.palette_index);
            output.push(0);
            output.extend_from_slice(&appearance.x.to_le_bytes());
            output.extend_from_slice(&appearance.y.to_le_bytes());
        }
        Ok(output)
    }

    /// Decodes an exactly consumed resolved-appearance file.
    ///
    /// # Errors
    ///
    /// Returns [`EntityAppearanceFileError`] for malformed framing, excessive counts, unknown
    /// source kinds or flags, invalid palettes, reserved bytes, truncation, or trailing data.
    pub fn decode(bytes: &[u8]) -> Result<Self, EntityAppearanceFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(EntityAppearanceFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(EntityAppearanceFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(EntityAppearanceFileError::UnsupportedVersion(version));
        }
        if header[10..12] != [0; 2] {
            return Err(EntityAppearanceFileError::ReservedBytes);
        }
        let count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| EntityAppearanceFileError::Overflow)?;
        if count > Self::MAX_APPEARANCES {
            return Err(EntityAppearanceFileError::TooManyAppearances(count));
        }
        let expected = Self::HEADER_LEN
            .checked_add(
                count
                    .checked_mul(Self::RECORD_LEN)
                    .ok_or(EntityAppearanceFileError::Overflow)?,
            )
            .ok_or(EntityAppearanceFileError::Overflow)?;
        if bytes.len() != expected {
            return Err(EntityAppearanceFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let appearances = bytes[Self::HEADER_LEN..]
            .chunks_exact(Self::RECORD_LEN)
            .map(decode_record)
            .collect::<Result<Vec<_>, _>>()?;
        validate(&appearances)?;
        Ok(Self { appearances })
    }
}

fn decode_record(record: &[u8]) -> Result<EntityAppearanceRecord, EntityAppearanceFileError> {
    if record[2..4] != [0; 2] || record[11] != 0 {
        return Err(EntityAppearanceFileError::ReservedBytes);
    }
    if record[1] & !3 != 0 {
        return Err(EntityAppearanceFileError::UnknownFlags(record[1]));
    }
    let index = u32::from_le_bytes([record[4], record[5], record[6], record[7]]);
    let source = match record[0] {
        0 => AppearanceSource::Layer1Object(index),
        1 => AppearanceSource::Layer2Object(index),
        2 => AppearanceSource::Sprite(index),
        value => return Err(EntityAppearanceFileError::UnknownSource(value)),
    };
    Ok(EntityAppearanceRecord {
        source,
        tile_index: u16::from_le_bytes([record[8], record[9]]),
        palette_index: record[10],
        x: i32::from_le_bytes([record[12], record[13], record[14], record[15]]),
        y: i32::from_le_bytes([record[16], record[17], record[18], record[19]]),
        x_flip: record[1] & 1 != 0,
        y_flip: record[1] & 2 != 0,
    })
}

fn validate(appearances: &[EntityAppearanceRecord]) -> Result<(), EntityAppearanceFileError> {
    if appearances.len() > EntityAppearanceFile::MAX_APPEARANCES {
        return Err(EntityAppearanceFileError::TooManyAppearances(
            appearances.len(),
        ));
    }
    if let Some(appearance) = appearances
        .iter()
        .find(|appearance| appearance.palette_index > 7)
    {
        return Err(EntityAppearanceFileError::PaletteOutOfRange(
            appearance.palette_index,
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntityAppearanceFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    UnknownFlags(u8),
    UnknownSource(u8),
    PaletteOutOfRange(u8),
    TooManyAppearances(usize),
    WrongLength { expected: usize, actual: usize },
    Overflow,
}

impl fmt::Display for EntityAppearanceFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid level entity appearance file: {self:?}")
    }
}

impl std::error::Error for EntityAppearanceFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> EntityAppearanceFile {
        EntityAppearanceFile {
            appearances: vec![EntityAppearanceRecord {
                source: AppearanceSource::Sprite(0x1234),
                tile_index: 0x345,
                palette_index: 7,
                x: -24,
                y: 400,
                x_flip: true,
                y_flip: false,
            }],
        }
    }

    #[test]
    fn exact_round_trip() {
        let file = file();
        assert_eq!(
            EntityAppearanceFile::decode(&file.encode().unwrap()).unwrap(),
            file
        );
    }

    #[test]
    fn every_truncation_trailing_flags_source_palette_and_reserved_data_fail() {
        let encoded = file().encode().unwrap();
        for length in 0..encoded.len() {
            assert!(EntityAppearanceFile::decode(&encoded[..length]).is_err());
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(EntityAppearanceFile::decode(&trailing).is_err());
        for (offset, value) in [(17, 4), (16, 3), (26, 8), (27, 1)] {
            let mut malformed = encoded.clone();
            malformed[offset] = value;
            assert!(EntityAppearanceFile::decode(&malformed).is_err());
        }
    }
}
