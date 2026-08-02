use crate::{
    NativeSpriteFieldError, NativeSpriteStream, ObjectRecord, ObjectStream, SpriteLengthTable,
    SpriteToken,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelEditError {
    IndexOutOfBounds { index: usize, len: usize },
    LegacyIncompatibleSpriteToken { index: usize },
    LegacyTerminatorCollision { index: usize },
    ExpandedSpritePositionSort,
    ExpandedSpriteRelocationRequiresExpanded,
    ExpandedSpriteYOutOfRange(u16),
    OpaqueExpandedSpriteControl { index: usize },
    ShortSpriteRecord { index: usize, len: usize },
    SpriteField(NativeSpriteFieldError),
    ObjectRelocation(crate::ObjectRelocationError),
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

    /// Relocates one expanded sprite record and canonically rebuilds shared upper-Y controls.
    ///
    /// Records are stably sorted by their decoded five-bit screen and then upper-Y state, matching
    /// Lunar Magic after a cross-screen or upper-band move. Base identity fields, extension bytes,
    /// and priority within the same screen/band remain unchanged. Redundant upper-Y controls are
    /// removed and the minimum state transitions needed by the sorted record sequence are emitted.
    /// Returns the selected record's new token index.
    ///
    /// # Errors
    ///
    /// Rejects legacy streams, invalid/non-record selections, out-of-range coordinates, opaque
    /// control tokens, and revision-table width changes without mutating the stream.
    pub fn relocate_expanded_record(
        &mut self,
        selected: usize,
        screen: u8,
        x: u8,
        y: u16,
        lengths: &SpriteLengthTable,
    ) -> Result<usize, LevelEditError> {
        if !self.expanded {
            return Err(LevelEditError::ExpandedSpriteRelocationRequiresExpanded);
        }
        if selected >= self.tokens.len() {
            return Err(LevelEditError::IndexOutOfBounds {
                index: selected,
                len: self.tokens.len(),
            });
        }
        let upper_y = u8::try_from(y / 32)
            .ok()
            .filter(|value| *value <= 0x7f)
            .ok_or(LevelEditError::ExpandedSpriteYOutOfRange(y))?;
        let y_low =
            u8::try_from(y % 32).map_err(|_| LevelEditError::ExpandedSpriteYOutOfRange(y))?;
        let mut active_upper_y = 0_u8;
        let mut records = Vec::with_capacity(self.tokens.len());
        for (index, token) in self.tokens.iter().enumerate() {
            match token {
                SpriteToken::Screen(value) => active_upper_y = *value,
                SpriteToken::Control(_) => {
                    return Err(LevelEditError::OpaqueExpandedSpriteControl { index });
                }
                SpriteToken::Record(record) => {
                    let mut record = record.clone();
                    let mut fields = record
                        .native_fields()
                        .map_err(LevelEditError::SpriteField)?;
                    let record_upper_y = if index == selected {
                        fields.screen = screen;
                        fields.x = x;
                        fields.y_low = y_low;
                        record
                            .set_native_fields(fields, lengths)
                            .map_err(LevelEditError::SpriteField)?;
                        upper_y
                    } else {
                        active_upper_y
                    };
                    records.push((fields.screen, record_upper_y, index, record));
                }
            }
        }
        if !records.iter().any(|(_, _, index, _)| *index == selected) {
            return Err(LevelEditError::LegacyIncompatibleSpriteToken { index: selected });
        }
        records.sort_by_key(|(screen, upper_y, _, _)| (*screen, *upper_y));
        let mut rebuilt = Vec::with_capacity(records.len().saturating_mul(2));
        let mut emitted_upper_y = 0_u8;
        let mut new_selected = None;
        for (_, record_upper_y, original_index, record) in records {
            if record_upper_y != emitted_upper_y {
                rebuilt.push(SpriteToken::Screen(record_upper_y));
                emitted_upper_y = record_upper_y;
            }
            if original_index == selected {
                new_selected = Some(rebuilt.len());
            }
            rebuilt.push(SpriteToken::Record(record));
        }
        let new_selected = new_selected.ok_or(LevelEditError::IndexOutOfBounds {
            index: selected,
            len: rebuilt.len(),
        })?;
        self.tokens = rebuilt;
        Ok(new_selected)
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

    #[test]
    fn expanded_relocation_rebuilds_minimal_upper_y_transitions() {
        let mut stream = NativeSpriteStream {
            header: 0x5a,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(2),
                sprite(1),
                SpriteToken::Screen(2),
                sprite(2),
                SpriteToken::Screen(5),
                sprite(3),
            ],
        };
        let selected = stream
            .relocate_expanded_record(3, 7, 9, 6 * 32 + 29, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(selected, 5);
        assert_eq!(
            stream.tokens,
            [
                SpriteToken::Screen(2),
                sprite(1),
                SpriteToken::Screen(5),
                sprite(3),
                SpriteToken::Screen(6),
                SpriteToken::Record(crate::SpriteRecord {
                    encoded: vec![0xd1, 0x97, 2],
                }),
            ]
        );
        assert_eq!(stream.header, 0x5a);
    }

    #[test]
    fn expanded_relocation_stably_sorts_screens_and_tracks_upper_y_state() {
        let mut stream = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(3),
                sprite_on_screen(2, 0x10),
                SpriteToken::Screen(1),
                sprite_on_screen(0, 0x20),
                sprite_on_screen(2, 0x30),
                SpriteToken::Screen(4),
                sprite_on_screen(1, 0x40),
            ],
        };

        let selected = stream
            .relocate_expanded_record(1, 1, 5, 2 * 32 + 31, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(selected, 3);
        assert_eq!(
            stream
                .native_placements()
                .into_iter()
                .map(|placement| (placement.screen, placement.minor, placement.sprite_number))
                .collect::<Vec<_>>(),
            [(0, 32, 0x20), (1, 95, 0x10), (1, 128, 0x40), (2, 32, 0x30)]
        );
        assert_eq!(
            stream
                .tokens
                .iter()
                .filter_map(|token| match token {
                    SpriteToken::Record(record) => Some(record.encoded[2]),
                    SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
                })
                .collect::<Vec<_>>(),
            [0x20, 0x10, 0x40, 0x30]
        );
    }

    #[test]
    fn expanded_relocation_preserves_custom_extension_bytes() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(2, 0x42, 5).unwrap();
        let mut stream = NativeSpriteStream {
            header: 0,
            expanded: true,
            tokens: vec![SpriteToken::Record(crate::SpriteRecord {
                encoded: vec![0x08, 0x00, 0x42, 0xaa, 0xbb],
            })],
        };
        assert_eq!(
            stream
                .relocate_expanded_record(0, 0x1e, 3, 0x7f, &lengths)
                .unwrap(),
            1
        );
        let SpriteToken::Record(record) = &stream.tokens[1] else {
            panic!("relocated record must remain a record");
        };
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap().screen, 0x1e);
        assert_eq!(record.native_fields().unwrap().x, 3);
        assert_eq!(record.native_fields().unwrap().y_low, 0x1f);
    }

    #[test]
    fn expanded_relocation_failures_are_atomic() {
        for (mut stream, index, y) in [
            (
                NativeSpriteStream {
                    header: 0,
                    expanded: false,
                    tokens: vec![sprite(1)],
                },
                0,
                0,
            ),
            (
                NativeSpriteStream {
                    header: 0,
                    expanded: true,
                    tokens: vec![SpriteToken::Control(0x80), sprite(1)],
                },
                1,
                0,
            ),
            (
                NativeSpriteStream {
                    header: 0,
                    expanded: true,
                    tokens: vec![sprite(1)],
                },
                0,
                0x1000,
            ),
        ] {
            let original = stream.clone();
            assert!(
                stream
                    .relocate_expanded_record(index, 0, 0, y, &SpriteLengthTable::standard())
                    .is_err()
            );
            assert_eq!(stream, original);
        }
    }
}
