use crate::{NativeSpriteStream, ObjectRecord, ObjectStream, SpriteToken};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelEditError {
    IndexOutOfBounds { index: usize, len: usize },
    LegacyIncompatibleSpriteToken { index: usize },
    LegacyTerminatorCollision { index: usize },
    ExpandedSpritePositionSort,
    ShortSpriteRecord { index: usize, len: usize },
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

    /// Stably restores Lunar Magic's legacy sprite screen ordering.
    ///
    /// Returns the selected record's new index. Records on the same screen retain their prior
    /// priority order.
    ///
    /// # Errors
    ///
    /// Rejects expanded streams, control tokens, short records, and an invalid selected index
    /// without changing the stream.
    pub fn sort_legacy_records_by_screen(
        &mut self,
        selected: usize,
    ) -> Result<usize, LevelEditError> {
        if selected >= self.tokens.len() {
            return Err(LevelEditError::IndexOutOfBounds {
                index: selected,
                len: self.tokens.len(),
            });
        }
        if self.expanded {
            return Err(LevelEditError::ExpandedSpritePositionSort);
        }
        let mut staged = Vec::with_capacity(self.tokens.len());
        for (index, token) in self.tokens.iter().cloned().enumerate() {
            let SpriteToken::Record(record) = &token else {
                return Err(LevelEditError::LegacyIncompatibleSpriteToken { index });
            };
            let fields = record
                .native_fields()
                .map_err(|_| LevelEditError::ShortSpriteRecord {
                    index,
                    len: record.encoded.len(),
                })?;
            staged.push((fields.screen, index, token));
        }
        staged.sort_by_key(|(screen, _, _)| *screen);
        let Some(new_index) = staged
            .iter()
            .position(|(_, original, _)| *original == selected)
        else {
            return Err(LevelEditError::IndexOutOfBounds {
                index: selected,
                len: staged.len(),
            });
        };
        self.tokens = staged.into_iter().map(|(_, _, token)| token).collect();
        Ok(new_index)
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

    fn sprite_on_screen(screen: u8, id: u8) -> SpriteToken {
        SpriteToken::Record(SpriteRecord {
            encoded: vec![(screen >> 4) << 1, screen & 0x0f, id],
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

    #[test]
    fn legacy_position_sort_is_stable_and_tracks_the_selected_record() {
        let mut stream = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![
                sprite_on_screen(2, 0x10),
                sprite_on_screen(0, 0x20),
                sprite_on_screen(2, 0x30),
                sprite_on_screen(1, 0x40),
            ],
        };
        assert_eq!(stream.sort_legacy_records_by_screen(0).unwrap(), 2);
        assert_eq!(
            stream
                .tokens
                .iter()
                .map(|token| match token {
                    SpriteToken::Record(record) => record.encoded[2],
                    SpriteToken::Screen(_) | SpriteToken::Control(_) => unreachable!(),
                })
                .collect::<Vec<_>>(),
            [0x20, 0x40, 0x10, 0x30]
        );
    }

    #[test]
    fn position_sort_failures_are_atomic() {
        for mut stream in [
            NativeSpriteStream {
                header: 0,
                expanded: true,
                tokens: vec![sprite(1)],
            },
            NativeSpriteStream {
                header: 0,
                expanded: false,
                tokens: vec![SpriteToken::Record(SpriteRecord { encoded: vec![1] })],
            },
        ] {
            let original = stream.clone();
            assert!(stream.sort_legacy_records_by_screen(0).is_err());
            assert_eq!(stream, original);
        }
    }
}
