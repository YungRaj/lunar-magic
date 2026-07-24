use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_level::{ObjectRecord, SpriteRecord};

impl ClipboardPayload {
    #[must_use]
    pub fn from_level_objects(objects: &[ObjectRecord]) -> Self {
        Self {
            kind: ClipboardKind::LevelObjects,
            records: objects
                .iter()
                .map(|object| object.encoded().to_vec())
                .collect(),
        }
    }

    /// Converts a level-object payload back into validated records.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or malformed record bytes.
    pub fn to_level_objects(&self) -> Result<Vec<ObjectRecord>, ClipboardError> {
        self.require_kind(ClipboardKind::LevelObjects)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                ObjectRecord::new(record.clone()).map_err(|_| ClipboardError::InvalidRecord {
                    index,
                    length: record.len(),
                })
            })
            .collect()
    }

    #[must_use]
    pub fn from_level_sprites(sprites: &[SpriteRecord]) -> Self {
        Self {
            kind: ClipboardKind::LevelSprites,
            records: sprites
                .iter()
                .map(|sprite| sprite.encoded.clone())
                .collect(),
        }
    }

    /// Returns lossless native sprite records; their variant-specific validation occurs when the
    /// destination editor inserts them using its active sprite length table.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong clipboard domain or records shorter than the
    /// native three-byte base shape.
    pub fn to_level_sprites(&self) -> Result<Vec<SpriteRecord>, ClipboardError> {
        self.require_kind(ClipboardKind::LevelSprites)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                if record.len() < 3 {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                }
                Ok(SpriteRecord {
                    encoded: record.clone(),
                })
            })
            .collect()
    }
}
