use crate::Submap;
use std::fmt;

/// Semantic overworld sprite plus bytes not owned by the portable editor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldSprite {
    pub id: u16,
    pub x: u16,
    pub y: u16,
    pub submap: Submap,
    pub extra: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldSpriteError {
    RecordTooShort(usize),
    Misaligned {
        bytes: usize,
        record_len: usize,
    },
    InvalidSubmap {
        record: usize,
        value: u8,
    },
    ExtraLength {
        record: usize,
        actual: usize,
        expected: usize,
    },
    SizeOverflow,
}

impl fmt::Display for OverworldSpriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid overworld sprite table: {self:?}")
    }
}

impl std::error::Error for OverworldSpriteError {}

impl OverworldSprite {
    pub const OWNED_LEN: usize = 7;

    /// Decodes fixed-size records while retaining every unowned trailing byte.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldSpriteError`] for an undersized/misaligned table or invalid submap.
    pub fn decode_all(bytes: &[u8], record_len: usize) -> Result<Vec<Self>, OverworldSpriteError> {
        if record_len < Self::OWNED_LEN {
            return Err(OverworldSpriteError::RecordTooShort(record_len));
        }
        if bytes.len() % record_len != 0 {
            return Err(OverworldSpriteError::Misaligned {
                bytes: bytes.len(),
                record_len,
            });
        }
        bytes
            .chunks_exact(record_len)
            .enumerate()
            .map(|(record, bytes)| {
                let submap =
                    Submap::decode(bytes[6]).ok_or(OverworldSpriteError::InvalidSubmap {
                        record,
                        value: bytes[6],
                    })?;
                Ok(Self {
                    id: u16::from_le_bytes([bytes[0], bytes[1]]),
                    x: u16::from_le_bytes([bytes[2], bytes[3]]),
                    y: u16::from_le_bytes([bytes[4], bytes[5]]),
                    submap,
                    extra: bytes[Self::OWNED_LEN..].to_vec(),
                })
            })
            .collect()
    }

    /// Encodes fixed-size records and preserves their unowned trailing bytes.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldSpriteError`] when a record has the wrong extension length or the
    /// output size overflows.
    pub fn encode_all(
        sprites: &[Self],
        record_len: usize,
    ) -> Result<Vec<u8>, OverworldSpriteError> {
        if record_len < Self::OWNED_LEN {
            return Err(OverworldSpriteError::RecordTooShort(record_len));
        }
        let expected = record_len - Self::OWNED_LEN;
        let capacity = sprites
            .len()
            .checked_mul(record_len)
            .ok_or(OverworldSpriteError::SizeOverflow)?;
        let mut encoded = Vec::with_capacity(capacity);
        for (record, sprite) in sprites.iter().enumerate() {
            if sprite.extra.len() != expected {
                return Err(OverworldSpriteError::ExtraLength {
                    record,
                    actual: sprite.extra.len(),
                    expected,
                });
            }
            encoded.extend_from_slice(&sprite.id.to_le_bytes());
            encoded.extend_from_slice(&sprite.x.to_le_bytes());
            encoded.extend_from_slice(&sprite.y.to_le_bytes());
            encoded.push(sprite.submap.encoded());
            encoded.extend_from_slice(&sprite.extra);
        }
        Ok(encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_records_preserve_extension_bytes() {
        let bytes = [0x34, 0x12, 2, 0, 3, 0, 6, 0xaa, 0xbb];
        let sprites = OverworldSprite::decode_all(&bytes, 9).unwrap();
        assert_eq!(sprites[0].id, 0x1234);
        assert_eq!(sprites[0].submap, Submap::StarWorld);
        assert_eq!(sprites[0].extra, [0xaa, 0xbb]);
        assert_eq!(OverworldSprite::encode_all(&sprites, 9).unwrap(), bytes);
    }

    #[test]
    fn malformed_records_are_rejected() {
        assert_eq!(
            OverworldSprite::decode_all(&[0; 8], 6),
            Err(OverworldSpriteError::RecordTooShort(6))
        );
        assert!(matches!(
            OverworldSprite::decode_all(&[0; 8], 7),
            Err(OverworldSpriteError::Misaligned { .. })
        ));
        let mut invalid = [0; 7];
        invalid[6] = 7;
        assert!(matches!(
            OverworldSprite::decode_all(&invalid, 7),
            Err(OverworldSpriteError::InvalidSubmap { .. })
        ));
    }
}
