use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_overworld::{OverworldMessage, OverworldSprite};

impl ClipboardPayload {
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
