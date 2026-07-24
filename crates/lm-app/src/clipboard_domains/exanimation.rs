use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_graphics::{ExAnimationFrame, ExAnimationRecord};

impl ClipboardPayload {
    #[must_use]
    pub fn from_exanimation_records(records: &[ExAnimationRecord]) -> Self {
        Self {
            kind: ClipboardKind::ExAnimationRecords,
            records: records
                .iter()
                .map(|record| record.encoded().to_vec())
                .collect(),
        }
    }

    /// Decodes complete lossless `ExAnimation` records.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or incorrectly sized records.
    pub fn to_exanimation_records(&self) -> Result<Vec<ExAnimationRecord>, ClipboardError> {
        self.require_kind(ClipboardKind::ExAnimationRecords)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                ExAnimationRecord::decode(record).map_err(|_| ClipboardError::InvalidRecord {
                    index,
                    length: record.len(),
                })
            })
            .collect()
    }

    /// Encodes each frame with an explicit one- or two-word width.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] when any frame does not match an ordinary recovered transfer
    /// size. The destination record's size mode is validated separately during paste.
    pub fn from_exanimation_frames(frames: &[ExAnimationFrame]) -> Result<Self, ClipboardError> {
        let records = frames
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let count = frame.source_words.len();
                if !(1..=2).contains(&count) {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: count,
                    });
                }
                let mut record = Vec::with_capacity(1 + count * 2);
                record.push(
                    u8::try_from(count).map_err(|_| ClipboardError::InvalidRecord {
                        index,
                        length: count,
                    })?,
                );
                record.extend(
                    frame
                        .source_words
                        .iter()
                        .flat_map(|word| word.to_le_bytes()),
                );
                Ok(record)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(ClipboardKind::ExAnimationFrames, records)
    }

    /// Decodes one- or two-word `ExAnimation` source frames.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for another editor domain, an invalid width marker, or a record
    /// whose byte length does not exactly match that marker.
    pub fn to_exanimation_frames(&self) -> Result<Vec<ExAnimationFrame>, ClipboardError> {
        self.require_kind(ClipboardKind::ExAnimationFrames)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let Some((&count, words)) = record.split_first() else {
                    return Err(ClipboardError::InvalidRecord { index, length: 0 });
                };
                let count = usize::from(count);
                if !(1..=2).contains(&count) || words.len() != count * 2 {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                }
                Ok(ExAnimationFrame {
                    source_words: words
                        .chunks_exact(2)
                        .map(|word| u16::from_le_bytes([word[0], word[1]]))
                        .collect(),
                })
            })
            .collect()
    }
}
