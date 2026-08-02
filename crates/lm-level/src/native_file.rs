use crate::{
    LevelObjectData, NativeSpriteEncodingError, NativeSpriteStream, ObjectStreamError,
    SpriteLengthTable, SpriteStreamError,
};
use std::fmt;

/// A versioned, decoded interchange file for one native level slot.
///
/// This deliberately contains only the two streams represented by [`LevelObjectData`] and
/// [`NativeSpriteStream`]. Revision-specific entrances, exits, Map16, palettes, and animations
/// remain separate project resources instead of being guessed from a ROM layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelFile {
    pub source_level: u16,
    pub layer1: LevelObjectData,
    pub sprites: NativeSpriteStream,
}

impl NativeLevelFile {
    pub const MAGIC: [u8; 8] = *b"LMLVL1\0\0";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 24;
    pub const MAX_STREAM_LEN: usize = 0x8000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + 2 * Self::MAX_STREAM_LEN;

    /// Encodes a canonical file containing the native layer-1 and sprite streams.
    ///
    /// # Errors
    ///
    /// Returns [`NativeLevelFileError`] if either stream exceeds its native single-bank limit.
    pub fn encode(&self) -> Result<Vec<u8>, NativeLevelFileError> {
        let layer1 = self.layer1.encode_banked()?;
        let mut canonical_sprites = self.sprites.clone();
        canonical_sprites.canonicalize_framing();
        let sprites = canonical_sprites.encode_checked()?;
        validate_len(StreamKind::Sprites, sprites.len())?;
        let layer1_len = u32::try_from(layer1.len()).map_err(|_| NativeLevelFileError::Overflow)?;
        let sprite_len =
            u32::try_from(sprites.len()).map_err(|_| NativeLevelFileError::Overflow)?;

        let mut bytes = Vec::with_capacity(Self::HEADER_LEN + layer1.len() + sprites.len());
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&u16::from(canonical_sprites.expanded).to_le_bytes());
        bytes.extend_from_slice(&self.source_level.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&layer1_len.to_le_bytes());
        bytes.extend_from_slice(&sprite_len.to_le_bytes());
        bytes.extend_from_slice(&layer1);
        bytes.extend_from_slice(&sprites);
        Ok(bytes)
    }

    /// Decodes and validates a native-level interchange file.
    ///
    /// `sprite_lengths` is revision/tool specific and is supplied explicitly so custom sprite
    /// extra bytes are never silently interpreted using an invented table.
    ///
    /// # Errors
    ///
    /// Returns [`NativeLevelFileError`] for framing, size, flag, object, or sprite errors.
    pub fn decode(
        bytes: &[u8],
        sprite_lengths: &SpriteLengthTable,
    ) -> Result<Self, NativeLevelFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(NativeLevelFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(NativeLevelFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(NativeLevelFileError::UnsupportedVersion(version));
        }
        let flags = u16::from_le_bytes([header[10], header[11]]);
        if flags & !1 != 0 {
            return Err(NativeLevelFileError::UnknownFlags(flags));
        }
        if header[14] != 0 || header[15] != 0 {
            return Err(NativeLevelFileError::ReservedBytes);
        }
        let source_level = u16::from_le_bytes([header[12], header[13]]);
        let layer1_len = read_len(header, 16)?;
        let sprite_len = read_len(header, 20)?;
        validate_len(StreamKind::Layer1, layer1_len)?;
        validate_len(StreamKind::Sprites, sprite_len)?;
        let expected = Self::HEADER_LEN
            .checked_add(layer1_len)
            .and_then(|len| len.checked_add(sprite_len))
            .ok_or(NativeLevelFileError::Overflow)?;
        if bytes.len() != expected {
            return Err(NativeLevelFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let split = Self::HEADER_LEN + layer1_len;
        let layer1_bytes = &bytes[Self::HEADER_LEN..split];
        let sprite_bytes = &bytes[split..];
        let layer1 = LevelObjectData::parse(layer1_bytes)?;
        if layer1.encode()? != layer1_bytes {
            return Err(NativeLevelFileError::NonCanonicalStream(StreamKind::Layer1));
        }
        let sprites = NativeSpriteStream::parse(sprite_bytes, flags & 1 != 0, sprite_lengths)?;
        if sprites.encode_checked()? != sprite_bytes {
            return Err(NativeLevelFileError::NonCanonicalStream(
                StreamKind::Sprites,
            ));
        }
        Ok(Self {
            source_level,
            layer1,
            sprites,
        })
    }
}

fn read_len(bytes: &[u8], offset: usize) -> Result<usize, NativeLevelFileError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(NativeLevelFileError::Truncated)?;
    usize::try_from(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
        .map_err(|_| NativeLevelFileError::Overflow)
}

fn validate_len(kind: StreamKind, len: usize) -> Result<(), NativeLevelFileError> {
    if len > NativeLevelFile::MAX_STREAM_LEN {
        Err(NativeLevelFileError::StreamTooLarge { kind, len })
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamKind {
    Layer1,
    Sprites,
}

#[derive(Debug)]
pub enum NativeLevelFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    UnknownFlags(u16),
    ReservedBytes,
    StreamTooLarge { kind: StreamKind, len: usize },
    WrongLength { expected: usize, actual: usize },
    NonCanonicalStream(StreamKind),
    Overflow,
    Objects(ObjectStreamError),
    Sprites(SpriteStreamError),
    SpriteEncoding(NativeSpriteEncodingError),
}

impl fmt::Display for NativeLevelFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native level file: {self:?}")
    }
}

impl std::error::Error for NativeLevelFileError {}

impl From<ObjectStreamError> for NativeLevelFileError {
    fn from(value: ObjectStreamError) -> Self {
        Self::Objects(value)
    }
}

impl From<SpriteStreamError> for NativeLevelFileError {
    fn from(value: SpriteStreamError) -> Self {
        Self::Sprites(value)
    }
}

impl From<NativeSpriteEncodingError> for NativeLevelFileError {
    fn from(value: NativeSpriteEncodingError) -> Self {
        Self::SpriteEncoding(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(expanded: bool) -> NativeLevelFile {
        let sprite_bytes: &[u8] = if expanded {
            &[0x30, 0xff, 1, 0x00, 0x20, 0x01, 0xff, 0xfe]
        } else {
            &[0x10, 0x00, 0x20, 0x01, 0xff]
        };
        NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 9, 8, 7, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                sprite_bytes,
                expanded,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn legacy_and_expanded_files_round_trip() {
        for expanded in [false, true] {
            let file = file(expanded);
            assert_eq!(
                NativeLevelFile::decode(&file.encode().unwrap(), &SpriteLengthTable::standard())
                    .unwrap(),
                file
            );
        }
    }

    #[test]
    fn rejects_unknown_flags_reserved_bytes_and_trailing_data() {
        let bytes = file(false).encode().unwrap();
        for (offset, value, expected) in [(10, 2, "UnknownFlags"), (14, 1, "ReservedBytes")] {
            let mut malformed = bytes.clone();
            malformed[offset] = value;
            assert!(
                format!(
                    "{:?}",
                    NativeLevelFile::decode(&malformed, &SpriteLengthTable::standard())
                        .unwrap_err()
                )
                .contains(expected)
            );
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            NativeLevelFile::decode(&trailing, &SpriteLengthTable::standard()),
            Err(NativeLevelFileError::WrongLength { .. })
        ));
    }

    #[test]
    fn rejects_declared_streams_above_native_bank_limit_before_slicing() {
        let mut bytes = file(false).encode().unwrap();
        bytes[16..20].copy_from_slice(&0x8001_u32.to_le_bytes());
        assert!(matches!(
            NativeLevelFile::decode(&bytes, &SpriteLengthTable::standard()),
            Err(NativeLevelFileError::StreamTooLarge {
                kind: StreamKind::Layer1,
                len: 0x8001
            })
        ));
    }

    #[test]
    fn encoding_rejects_public_models_that_would_emit_early_sprite_terminators() {
        let mut invalid = file(true);
        invalid
            .sprites
            .tokens
            .push(crate::SpriteToken::Control(0xfe));
        assert!(matches!(
            invalid.encode(),
            Err(NativeLevelFileError::SpriteEncoding(
                NativeSpriteEncodingError::InvalidControl { value: 0xfe, .. }
            ))
        ));
    }

    #[test]
    fn decode_rejects_bytes_hidden_after_an_inner_stream_terminator() {
        let canonical = file(false).encode().unwrap();
        for kind in [StreamKind::Layer1, StreamKind::Sprites] {
            let mut malformed = canonical.clone();
            match kind {
                StreamKind::Layer1 => {
                    let layer_len = u32::from_le_bytes(malformed[16..20].try_into().unwrap());
                    let insertion =
                        NativeLevelFile::HEADER_LEN + usize::try_from(layer_len).unwrap();
                    malformed.insert(insertion, 0xaa);
                    malformed[16..20].copy_from_slice(&(layer_len + 1).to_le_bytes());
                }
                StreamKind::Sprites => {
                    malformed.push(0xaa);
                    let sprite_len = u32::from_le_bytes(malformed[20..24].try_into().unwrap());
                    malformed[20..24].copy_from_slice(&(sprite_len + 1).to_le_bytes());
                }
            }
            assert!(matches!(
                NativeLevelFile::decode(&malformed, &SpriteLengthTable::standard()),
                Err(NativeLevelFileError::NonCanonicalStream(actual)) if actual == kind
            ));
        }
    }
}
