use std::fmt;

mod validation;

use validation::{encoded_len, validate};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpriteAppearancePart {
    pub tile_index: u16,
    pub palette_index: u8,
    pub x_offset: i16,
    pub y_offset: i16,
    pub x_flip: bool,
    pub y_flip: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteAppearanceDefinition {
    pub sprite_id: u16,
    pub parts: Vec<SpriteAppearancePart>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteAppearanceFile {
    pub definitions: Vec<SpriteAppearanceDefinition>,
}

impl SpriteAppearanceFile {
    pub const MAGIC: [u8; 8] = *b"LMOWAPP1";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 20;
    const DEFINITION_LEN: usize = 12;
    const PART_LEN: usize = 12;
    pub const MAX_DEFINITIONS: usize = 256;
    pub const MAX_PARTS: usize = 0x1_0000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN
        + Self::MAX_DEFINITIONS * Self::DEFINITION_LEN
        + Self::MAX_PARTS * Self::PART_LEN;

    /// Encodes canonical definition and part tables.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteAppearanceFileError`] for duplicate IDs, invalid palettes, excessive
    /// counts, or arithmetic overflow.
    pub fn encode(&self) -> Result<Vec<u8>, SpriteAppearanceFileError> {
        validate(&self.definitions)?;
        let part_count = self
            .definitions
            .iter()
            .try_fold(0_usize, |total, definition| {
                total
                    .checked_add(definition.parts.len())
                    .ok_or(SpriteAppearanceFileError::Overflow)
            })?;
        let mut output = Vec::with_capacity(encoded_len(self.definitions.len(), part_count)?);
        output.extend_from_slice(&Self::MAGIC);
        output.extend_from_slice(&Self::VERSION.to_le_bytes());
        output.extend_from_slice(
            &u16::try_from(self.definitions.len())
                .map_err(|_| SpriteAppearanceFileError::Overflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(
            &u32::try_from(part_count)
                .map_err(|_| SpriteAppearanceFileError::Overflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(&[0; 4]);
        let mut first_part = 0_u32;
        for definition in &self.definitions {
            output.extend_from_slice(&definition.sprite_id.to_le_bytes());
            output.extend_from_slice(&[0; 2]);
            output.extend_from_slice(&first_part.to_le_bytes());
            let definition_part_count = u16::try_from(definition.parts.len()).map_err(|_| {
                SpriteAppearanceFileError::TooManyDefinitionParts {
                    sprite_id: definition.sprite_id,
                    count: definition.parts.len(),
                }
            })?;
            output.extend_from_slice(&definition_part_count.to_le_bytes());
            output.extend_from_slice(&[0; 2]);
            first_part = first_part
                .checked_add(u32::from(definition_part_count))
                .ok_or(SpriteAppearanceFileError::Overflow)?;
        }
        for part in self
            .definitions
            .iter()
            .flat_map(|definition| &definition.parts)
        {
            output.extend_from_slice(&part.x_offset.to_le_bytes());
            output.extend_from_slice(&part.y_offset.to_le_bytes());
            output.extend_from_slice(&part.tile_index.to_le_bytes());
            output.push(part.palette_index);
            output.push(u8::from(part.x_flip) | (u8::from(part.y_flip) << 1));
            output.extend_from_slice(&[0; 4]);
        }
        Ok(output)
    }

    /// Decodes an exactly consumed canonical appearance file.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteAppearanceFileError`] for malformed framing, noncanonical ranges, invalid
    /// flags or palettes, duplicate IDs, excessive counts, or overflow.
    pub fn decode(bytes: &[u8]) -> Result<Self, SpriteAppearanceFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(SpriteAppearanceFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(SpriteAppearanceFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(SpriteAppearanceFileError::UnsupportedVersion(version));
        }
        if header[16..20] != [0; 4] {
            return Err(SpriteAppearanceFileError::ReservedBytes);
        }
        let definition_count = usize::from(u16::from_le_bytes([header[10], header[11]]));
        let part_count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| SpriteAppearanceFileError::Overflow)?;
        if definition_count > Self::MAX_DEFINITIONS {
            return Err(SpriteAppearanceFileError::TooManyDefinitions(
                definition_count,
            ));
        }
        if part_count > Self::MAX_PARTS {
            return Err(SpriteAppearanceFileError::TooManyParts(part_count));
        }
        let definitions_len = definition_count
            .checked_mul(Self::DEFINITION_LEN)
            .ok_or(SpriteAppearanceFileError::Overflow)?;
        let expected = encoded_len(definition_count, part_count)?;
        if bytes.len() != expected {
            return Err(SpriteAppearanceFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let definition_bytes = &bytes[Self::HEADER_LEN..Self::HEADER_LEN + definitions_len];
        let part_bytes = &bytes[Self::HEADER_LEN + definitions_len..];
        let parts = part_bytes
            .chunks_exact(Self::PART_LEN)
            .map(|record| {
                if record[8..12] != [0; 4] {
                    return Err(SpriteAppearanceFileError::ReservedBytes);
                }
                if record[7] & !3 != 0 {
                    return Err(SpriteAppearanceFileError::UnknownFlags(record[7]));
                }
                if record[6] > 7 {
                    return Err(SpriteAppearanceFileError::PaletteOutOfRange(record[6]));
                }
                Ok(SpriteAppearancePart {
                    x_offset: i16::from_le_bytes([record[0], record[1]]),
                    y_offset: i16::from_le_bytes([record[2], record[3]]),
                    tile_index: u16::from_le_bytes([record[4], record[5]]),
                    palette_index: record[6],
                    x_flip: record[7] & 1 != 0,
                    y_flip: record[7] & 2 != 0,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut definitions = Vec::with_capacity(definition_count);
        let mut expected_first = 0_usize;
        for record in definition_bytes.chunks_exact(Self::DEFINITION_LEN) {
            if record[2..4] != [0; 2] || record[10..12] != [0; 2] {
                return Err(SpriteAppearanceFileError::ReservedBytes);
            }
            let first = usize::try_from(u32::from_le_bytes([
                record[4], record[5], record[6], record[7],
            ]))
            .map_err(|_| SpriteAppearanceFileError::Overflow)?;
            let count = usize::from(u16::from_le_bytes([record[8], record[9]]));
            if first != expected_first {
                return Err(SpriteAppearanceFileError::NonCanonicalPartRange {
                    expected: expected_first,
                    actual: first,
                });
            }
            let end = first
                .checked_add(count)
                .filter(|end| *end <= parts.len())
                .ok_or(SpriteAppearanceFileError::PartRangeOutOfBounds)?;
            definitions.push(SpriteAppearanceDefinition {
                sprite_id: u16::from_le_bytes([record[0], record[1]]),
                parts: parts[first..end].to_vec(),
            });
            expected_first = end;
        }
        if expected_first != parts.len() {
            return Err(SpriteAppearanceFileError::UnclaimedParts);
        }
        validate(&definitions)?;
        Ok(Self { definitions })
    }

    #[must_use]
    pub fn definition(&self, sprite_id: u16) -> Option<&SpriteAppearanceDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.sprite_id == sprite_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteAppearanceFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    UnknownFlags(u8),
    TooManyDefinitions(usize),
    TooManyParts(usize),
    TooManyDefinitionParts { sprite_id: u16, count: usize },
    DuplicateSpriteId(u16),
    PaletteOutOfRange(u8),
    NonCanonicalPartRange { expected: usize, actual: usize },
    PartRangeOutOfBounds,
    UnclaimedParts,
    WrongLength { expected: usize, actual: usize },
    Overflow,
}

impl fmt::Display for SpriteAppearanceFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid overworld sprite appearance file: {self:?}"
        )
    }
}
impl std::error::Error for SpriteAppearanceFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> SpriteAppearanceFile {
        SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 7,
                parts: vec![SpriteAppearancePart {
                    tile_index: 0x123,
                    palette_index: 4,
                    x_offset: -8,
                    y_offset: 16,
                    x_flip: true,
                    y_flip: false,
                }],
            }],
        }
    }

    #[test]
    fn exact_round_trip_and_lookup() {
        let file = file();
        let encoded = file.encode().unwrap();
        let decoded = SpriteAppearanceFile::decode(&encoded).unwrap();
        assert_eq!(decoded, file);
        assert_eq!(decoded.definition(7).unwrap().parts[0].x_offset, -8);
    }

    #[test]
    fn encoded_length_checks_each_product_and_the_sum() {
        assert_eq!(encoded_len(1, 1).unwrap(), 44);
        assert_eq!(
            encoded_len(usize::MAX / SpriteAppearanceFile::DEFINITION_LEN + 1, 0),
            Err(SpriteAppearanceFileError::Overflow)
        );
        assert_eq!(
            encoded_len(0, usize::MAX / SpriteAppearanceFile::PART_LEN + 1),
            Err(SpriteAppearanceFileError::Overflow)
        );
        assert_eq!(
            encoded_len(usize::MAX / SpriteAppearanceFile::DEFINITION_LEN, 1),
            Err(SpriteAppearanceFileError::Overflow)
        );
    }

    #[test]
    fn framing_duplicates_flags_palette_and_ranges_are_rejected() {
        let encoded = file().encode().unwrap();
        for length in 0..encoded.len() {
            assert!(SpriteAppearanceFile::decode(&encoded[..length]).is_err());
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(SpriteAppearanceFile::decode(&trailing).is_err());
        let mut flags = encoded.clone();
        flags[20 + 12 + 7] = 4;
        assert!(matches!(
            SpriteAppearanceFile::decode(&flags),
            Err(SpriteAppearanceFileError::UnknownFlags(4))
        ));
        let mut palette = encoded.clone();
        palette[20 + 12 + 6] = 8;
        assert!(matches!(
            SpriteAppearanceFile::decode(&palette),
            Err(SpriteAppearanceFileError::PaletteOutOfRange(8))
        ));
        let duplicate = SpriteAppearanceFile {
            definitions: vec![file().definitions[0].clone(), file().definitions[0].clone()],
        };
        assert!(matches!(
            duplicate.encode(),
            Err(SpriteAppearanceFileError::DuplicateSpriteId(7))
        ));
    }
}
