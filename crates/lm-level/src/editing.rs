use crate::{NativeSpriteStream, ObjectRecord, ObjectStream, SpriteToken};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelEditError {
    IndexOutOfBounds { index: usize, len: usize },
    LegacyIncompatibleSpriteToken { index: usize },
    LegacyTerminatorCollision { index: usize },
}

impl fmt::Display for LevelEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid level edit: {self:?}")
    }
}

impl std::error::Error for LevelEditError {}

impl ObjectStream {
    /// Inserts an object before `index`; `len` appends.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError::IndexOutOfBounds`] when `index` is greater than the length.
    pub fn insert(&mut self, index: usize, object: ObjectRecord) -> Result<(), LevelEditError> {
        insert(&mut self.records, index, object)
    }

    /// Removes and returns an object.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError::IndexOutOfBounds`] when no object exists at `index`.
    pub fn remove(&mut self, index: usize) -> Result<ObjectRecord, LevelEditError> {
        remove(&mut self.records, index)
    }

    /// Moves one object before an index in the pre-move ordering; `len` means the end.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError::IndexOutOfBounds`] for an invalid source or destination.
    pub fn move_before(&mut self, from: usize, before: usize) -> Result<(), LevelEditError> {
        move_before(&mut self.records, from, before)
    }
}

impl NativeSpriteStream {
    /// Inserts a sprite record/control token before `index`; `len` appends.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError`] for bounds or a token incompatible with a legacy stream.
    pub fn insert(&mut self, index: usize, token: SpriteToken) -> Result<(), LevelEditError> {
        if !self.expanded {
            validate_legacy_token(&token, index)?;
        }
        insert(&mut self.tokens, index, token)
    }

    /// Removes and returns a sprite token.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError::IndexOutOfBounds`] when no token exists at `index`.
    pub fn remove(&mut self, index: usize) -> Result<SpriteToken, LevelEditError> {
        remove(&mut self.tokens, index)
    }

    /// Moves one token before an index in the pre-move ordering; `len` means the end.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError::IndexOutOfBounds`] for an invalid source or destination.
    pub fn move_before(&mut self, from: usize, before: usize) -> Result<(), LevelEditError> {
        move_before(&mut self.tokens, from, before)
    }

    /// Changes the stream format after validating lossless legacy compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`LevelEditError`] when a control token or `0xFF`-prefixed record requires the
    /// expanded escaping format. Failure leaves the stream unchanged.
    pub fn set_expanded(&mut self, expanded: bool) -> Result<(), LevelEditError> {
        if !expanded {
            for (index, token) in self.tokens.iter().enumerate() {
                validate_legacy_token(token, index)?;
            }
        }
        self.expanded = expanded;
        Ok(())
    }
}

fn validate_legacy_token(token: &SpriteToken, index: usize) -> Result<(), LevelEditError> {
    match token {
        SpriteToken::Record(record) if record.encoded.first() == Some(&0xff) => {
            Err(LevelEditError::LegacyTerminatorCollision { index })
        }
        SpriteToken::Record(_) => Ok(()),
        SpriteToken::Screen(_) | SpriteToken::Control(_) => {
            Err(LevelEditError::LegacyIncompatibleSpriteToken { index })
        }
    }
}

fn insert<T>(values: &mut Vec<T>, index: usize, value: T) -> Result<(), LevelEditError> {
    if index > values.len() {
        return Err(LevelEditError::IndexOutOfBounds {
            index,
            len: values.len(),
        });
    }
    values.insert(index, value);
    Ok(())
}

fn remove<T>(values: &mut Vec<T>, index: usize) -> Result<T, LevelEditError> {
    if index >= values.len() {
        return Err(LevelEditError::IndexOutOfBounds {
            index,
            len: values.len(),
        });
    }
    Ok(values.remove(index))
}

fn move_before<T>(values: &mut Vec<T>, from: usize, before: usize) -> Result<(), LevelEditError> {
    let len = values.len();
    if from >= len {
        return Err(LevelEditError::IndexOutOfBounds { index: from, len });
    }
    if before > len {
        return Err(LevelEditError::IndexOutOfBounds { index: before, len });
    }
    if from == before || from.checked_add(1) == Some(before) {
        return Ok(());
    }
    let value = values.remove(from);
    let destination = if before > from { before - 1 } else { before };
    values.insert(destination, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpriteRecord;

    fn object(id: u8) -> ObjectRecord {
        ObjectRecord::new(vec![id, 0, 0]).unwrap()
    }

    fn sprite(id: u8) -> SpriteToken {
        SpriteToken::Record(SpriteRecord {
            encoded: vec![0, 0, id],
        })
    }

    #[test]
    fn object_insert_remove_and_move_preserve_expected_order() {
        let mut stream = ObjectStream {
            records: vec![object(1), object(2), object(3)],
        };
        stream.move_before(0, 3).unwrap();
        assert_eq!(
            stream
                .records
                .iter()
                .map(|record| record.encoded()[0])
                .collect::<Vec<_>>(),
            [2, 3, 1]
        );
        stream.insert(1, object(4)).unwrap();
        assert_eq!(stream.remove(2).unwrap().encoded()[0], 3);
        assert_eq!(stream.encode().unwrap(), [2, 0, 0, 4, 0, 0, 1, 0, 0, 0xff]);
    }

    #[test]
    fn invalid_edits_leave_sequences_unchanged() {
        let mut stream = ObjectStream {
            records: vec![object(1)],
        };
        let original = stream.clone();
        assert!(stream.move_before(0, 2).is_err());
        assert_eq!(stream, original);
        assert!(stream.insert(2, object(2)).is_err());
        assert_eq!(stream, original);
    }

    #[test]
    fn expanded_controls_cannot_be_silently_converted_to_legacy() {
        let mut stream = NativeSpriteStream {
            header: 0,
            expanded: true,
            tokens: vec![sprite(1), SpriteToken::Screen(2)],
        };
        assert!(matches!(
            stream.set_expanded(false),
            Err(LevelEditError::LegacyIncompatibleSpriteToken { index: 1 })
        ));
        assert!(stream.expanded);
        stream.remove(1).unwrap();
        stream.set_expanded(false).unwrap();
        assert!(!stream.expanded);
        assert!(stream.insert(1, SpriteToken::Control(0x80)).is_err());
        assert_eq!(stream.tokens, [sprite(1)]);
    }

    #[test]
    fn ff_prefixed_records_require_expanded_escaping() {
        let mut stream = NativeSpriteStream {
            header: 0,
            expanded: true,
            tokens: vec![SpriteToken::Record(SpriteRecord {
                encoded: vec![0xff, 0, 1],
            })],
        };
        assert!(matches!(
            stream.set_expanded(false),
            Err(LevelEditError::LegacyTerminatorCollision { index: 0 })
        ));
    }
}
