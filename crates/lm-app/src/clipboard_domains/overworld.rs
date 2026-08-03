use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_overworld::{OverworldMessage, OverworldSprite, SpriteAppearancePart};

const APPEARANCE_PART_LEN: usize = 8;
const APPEARANCE_X_FLIP: u8 = 1 << 0;
const APPEARANCE_Y_FLIP: u8 = 1 << 1;
const APPEARANCE_KNOWN_FLAGS: u8 = APPEARANCE_X_FLIP | APPEARANCE_Y_FLIP;

impl ClipboardPayload {
    /// Encodes complete painter-ordered overworld sprite appearance parts.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for invalid palette indexes or clipboard bounds.
    pub fn from_overworld_appearance_parts(
        parts: &[SpriteAppearancePart],
    ) -> Result<Self, ClipboardError> {
        let records = parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                if part.palette_index > 7 {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: APPEARANCE_PART_LEN,
                    });
                }
                let mut record = Vec::with_capacity(APPEARANCE_PART_LEN);
                record.extend_from_slice(&part.tile_index.to_le_bytes());
                record.push(part.palette_index);
                record.extend_from_slice(&part.x_offset.to_le_bytes());
                record.extend_from_slice(&part.y_offset.to_le_bytes());
                record.push(
                    (u8::from(part.x_flip) * APPEARANCE_X_FLIP)
                        | (u8::from(part.y_flip) * APPEARANCE_Y_FLIP),
                );
                Ok(record)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(ClipboardKind::OverworldAppearanceParts, records)
    }

    /// Decodes complete overworld sprite appearance parts.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain, invalid record widths, palette indexes,
    /// or unknown flags.
    pub fn to_overworld_appearance_parts(
        &self,
    ) -> Result<Vec<SpriteAppearancePart>, ClipboardError> {
        self.require_kind(ClipboardKind::OverworldAppearanceParts)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let Some(bytes) = record.as_slice().try_into().ok() else {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                };
                let bytes: [u8; APPEARANCE_PART_LEN] = bytes;
                let palette_index = bytes[2];
                let flags = bytes[7];
                if palette_index > 7 || flags & !APPEARANCE_KNOWN_FLAGS != 0 {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                }
                Ok(SpriteAppearancePart {
                    tile_index: u16::from_le_bytes([bytes[0], bytes[1]]),
                    palette_index,
                    x_offset: i16::from_le_bytes([bytes[3], bytes[4]]),
                    y_offset: i16::from_le_bytes([bytes[5], bytes[6]]),
                    x_flip: flags & APPEARANCE_X_FLIP != 0,
                    y_flip: flags & APPEARANCE_Y_FLIP != 0,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn from_overworld_messages(messages: &[OverworldMessage]) -> Self {
        Self {
            kind: ClipboardKind::OverworldMessages,
            records: messages
                .iter()
                .map(|message| message.encoded().to_vec())
                .collect(),
        }
    }

    /// Decodes complete overworld message tilemaps.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or incorrectly sized records.
    pub fn to_overworld_messages(&self) -> Result<Vec<OverworldMessage>, ClipboardError> {
        self.require_kind(ClipboardKind::OverworldMessages)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                OverworldMessage::decode(record).map_err(|_| ClipboardError::InvalidRecord {
                    index,
                    length: record.len(),
                })
            })
            .collect()
    }

    /// Encodes each overworld sprite with its own preserved extension length.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] if a record cannot be represented within clipboard limits.
    pub fn from_overworld_sprites(sprites: &[OverworldSprite]) -> Result<Self, ClipboardError> {
        let records = sprites
            .iter()
            .enumerate()
            .map(|(index, sprite)| {
                let record_len = OverworldSprite::OWNED_LEN
                    .checked_add(sprite.extra.len())
                    .ok_or(ClipboardError::RecordTooLarge(usize::MAX))?;
                OverworldSprite::encode_all(std::slice::from_ref(sprite), record_len).map_err(
                    |_| ClipboardError::InvalidRecord {
                        index,
                        length: record_len,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(ClipboardKind::OverworldSprites, records)
    }

    /// Decodes overworld sprite records and preserves each extension payload.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or malformed records.
    pub fn to_overworld_sprites(&self) -> Result<Vec<OverworldSprite>, ClipboardError> {
        self.require_kind(ClipboardKind::OverworldSprites)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                if record.len() < OverworldSprite::OWNED_LEN {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                }
                OverworldSprite::decode_all(record, record.len())
                    .map_err(|_| ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    })?
                    .into_iter()
                    .next()
                    .ok_or(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    })
            })
            .collect()
    }
}
