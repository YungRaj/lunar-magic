use crate::{
    NativeSpriteFieldError, NativeSpriteRecordFields, NativeSpriteStream, ObjectRecord,
    ObjectStream, SpriteLengthTable, SpriteToken,
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
    LegacySpriteYOutOfRange(u16),
    InvalidExpandedSpriteControl { index: usize, value: u8 },
    ShortSpriteRecord { index: usize, len: usize },
    SpriteField(NativeSpriteFieldError),
    ObjectField(crate::ObjectFieldError),
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
    /// Canonicalizes framing and Lunar Magic's orientation-dependent record ordering atomically.
    ///
    /// Legacy records use their five-bit screen. Expanded records additionally use resolved
    /// upper-Y state, and vertical modes use the record's low Y nibble as the final key. Raw
    /// parsing and encoding remain lossless; semantic aggregate serializers call this method.
    ///
    /// # Errors
    ///
    /// Rejects malformed records or invalid expanded controls without changing the stream.
    pub fn canonicalize_for_orientation(&mut self, vertical: bool) -> Result<(), LevelEditError> {
        let mut staged = self.clone();
        staged.canonicalize_framing();
        if staged.expanded {
            staged.sort_expanded_records_for_orientation(vertical)?;
        }
        *self = staged;
        Ok(())
    }

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

    /// Places one native sprite record at an absolute canvas position and restores Lunar Magic's
    /// canonical ordering. Legacy streams sort by screen; expanded streams also rebuild the
    /// minimum upper-Y controls for the requested orientation. The record's sprite number, extra
    /// bits, and extension bytes are preserved. Returns the placed record's resulting token index.
    ///
    /// # Errors
    ///
    /// Rejects malformed records, revision-table width changes, coordinates outside the native
    /// space, or malformed existing controls without mutating the stream.
    pub fn place_record_at_position(
        &mut self,
        mut record: crate::SpriteRecord,
        screen: u8,
        x: u8,
        y: u16,
        vertical: bool,
        lengths: &SpriteLengthTable,
    ) -> Result<usize, LevelEditError> {
        let mut staged = self.clone();
        let mut fields = record
            .native_fields()
            .map_err(LevelEditError::SpriteField)?;
        let y_low = if staged.expanded {
            u8::try_from(y % 32).map_err(|_| LevelEditError::ExpandedSpriteYOutOfRange(y))?
        } else {
            u8::try_from(y)
                .ok()
                .filter(|value| *value <= 0x1f)
                .ok_or(LevelEditError::LegacySpriteYOutOfRange(y))?
        };
        fields.screen = screen;
        fields.x = x;
        fields.y_low = y_low;
        record
            .set_native_fields(fields, lengths)
            .map_err(LevelEditError::SpriteField)?;
        let selected = staged.tokens.len();
        staged.insert(selected, SpriteToken::Record(record))?;
        let selected = if staged.expanded {
            staged.relocate_expanded_record(selected, screen, x, y, vertical, lengths)?
        } else {
            staged.sort_legacy_records_by_screen(selected)?
        };
        *self = staged;
        Ok(selected)
    }

    /// Relocates one native sprite record through the same absolute-position model used by canvas
    /// drag/drop. Identity fields and extension bytes remain unchanged, while record ordering and
    /// expanded upper-Y controls are rebuilt canonically. Returns the record's resulting index.
    ///
    /// # Errors
    ///
    /// Rejects a non-record selection, invalid coordinates, revision-table width changes, or
    /// malformed stream state without mutating the stream.
    pub fn relocate_record_position(
        &mut self,
        selected: usize,
        screen: u8,
        x: u8,
        y: u16,
        vertical: bool,
        lengths: &SpriteLengthTable,
    ) -> Result<usize, LevelEditError> {
        let mut staged = self.clone();
        if staged.expanded {
            let selected =
                staged.relocate_expanded_record(selected, screen, x, y, vertical, lengths)?;
            *self = staged;
            return Ok(selected);
        }
        let y_low = u8::try_from(y)
            .ok()
            .filter(|value| *value <= 0x1f)
            .ok_or(LevelEditError::LegacySpriteYOutOfRange(y))?;
        let len = staged.tokens.len();
        let Some(SpriteToken::Record(record)) = staged.tokens.get_mut(selected) else {
            return Err(if selected >= len {
                LevelEditError::IndexOutOfBounds {
                    index: selected,
                    len,
                }
            } else {
                LevelEditError::LegacyIncompatibleSpriteToken { index: selected }
            });
        };
        let mut fields = record
            .native_fields()
            .map_err(LevelEditError::SpriteField)?;
        fields.screen = screen;
        fields.x = x;
        fields.y_low = y_low;
        record
            .set_native_fields(fields, lengths)
            .map_err(LevelEditError::SpriteField)?;
        let selected = staged.sort_legacy_records_by_screen(selected)?;
        *self = staged;
        Ok(selected)
    }

    /// Replaces every proven native base field of one sprite record while preserving its extension
    /// bytes and current expanded upper-Y band. The record is then tracked through Lunar Magic's
    /// legacy or orientation-aware expanded ordering. Returns its resulting token index.
    ///
    /// # Errors
    ///
    /// Rejects a non-record selection, out-of-range fields, revision-table width changes, or
    /// malformed stream controls without mutating the stream.
    pub fn set_record_fields(
        &mut self,
        selected: usize,
        fields: NativeSpriteRecordFields,
        vertical: bool,
        lengths: &SpriteLengthTable,
    ) -> Result<usize, LevelEditError> {
        let mut staged = self.clone();
        if selected >= staged.tokens.len() {
            return Err(LevelEditError::IndexOutOfBounds {
                index: selected,
                len: staged.tokens.len(),
            });
        }
        let mut active_upper_y = 0_u8;
        for token in staged.tokens.iter().take(selected) {
            if let SpriteToken::Screen(value) = token {
                active_upper_y = *value;
            }
        }
        let Some(SpriteToken::Record(record)) = staged.tokens.get_mut(selected) else {
            return Err(LevelEditError::LegacyIncompatibleSpriteToken { index: selected });
        };
        record
            .set_native_fields(fields, lengths)
            .map_err(LevelEditError::SpriteField)?;
        let selected = if staged.expanded {
            let y = u16::from(active_upper_y)
                .checked_mul(32)
                .and_then(|value| value.checked_add(u16::from(fields.y_low)))
                .ok_or(LevelEditError::ExpandedSpriteYOutOfRange(u16::MAX))?;
            staged.relocate_expanded_record(
                selected,
                fields.screen,
                fields.x,
                y,
                vertical,
                lengths,
            )?
        } else {
            staged.sort_legacy_records_by_screen(selected)?
        };
        *self = staged;
        Ok(selected)
    }

    /// Stably restores Lunar Magic's expanded sprite ordering and minimum upper-Y transitions.
    ///
    /// # Errors
    ///
    /// Rejects legacy streams, malformed records, and invalid control tokens without changing the
    /// stream.
    pub fn sort_expanded_records_for_orientation(
        &mut self,
        vertical: bool,
    ) -> Result<(), LevelEditError> {
        if !self.expanded {
            return Err(LevelEditError::ExpandedSpriteRelocationRequiresExpanded);
        }
        let mut active_upper_y = 0_u8;
        let mut records = Vec::with_capacity(self.tokens.len());
        for (index, token) in self.tokens.iter().enumerate() {
            match token {
                SpriteToken::Screen(value) => active_upper_y = *value,
                SpriteToken::Control(value) if (0x80..=0xfd).contains(value) => {}
                SpriteToken::Control(value) => {
                    return Err(LevelEditError::InvalidExpandedSpriteControl {
                        index,
                        value: *value,
                    });
                }
                SpriteToken::Record(record) => {
                    let fields =
                        record
                            .native_fields()
                            .map_err(|_| LevelEditError::ShortSpriteRecord {
                                index,
                                len: record.encoded.len(),
                            })?;
                    let orientation_nibble = if vertical { fields.y_low & 0x0f } else { 0 };
                    records.push((
                        fields.screen,
                        active_upper_y,
                        orientation_nibble,
                        record.clone(),
                    ));
                }
            }
        }
        records.sort_by_key(|(screen, upper_y, orientation_nibble, _)| {
            (*screen, *upper_y, *orientation_nibble)
        });
        let mut rebuilt = Vec::with_capacity(records.len().saturating_mul(2));
        let mut emitted_upper_y = 0_u8;
        for (_, record_upper_y, _, record) in records {
            if record_upper_y != emitted_upper_y {
                rebuilt.push(SpriteToken::Screen(record_upper_y));
                emitted_upper_y = record_upper_y;
            }
            rebuilt.push(SpriteToken::Record(record));
        }
        self.tokens = rebuilt;
        self.canonicalize_framing();
        Ok(())
    }

    /// Relocates one expanded sprite record and canonically rebuilds shared upper-Y controls.
    ///
    /// Records are stably sorted by their decoded five-bit screen and then upper-Y state; vertical
    /// levels additionally compare the low four Y bits, matching Lunar Magic after a cross-screen
    /// or upper-band move. Base identity fields, extension bytes, and priority within the complete
    /// key remain unchanged. Redundant upper-Y controls are removed and the minimum state
    /// transitions needed by the sorted record sequence are emitted. Returns the selected record's
    /// new token index. Lunar Magic-ignored `$80..FD` control pairs are stripped.
    ///
    /// # Errors
    ///
    /// Rejects legacy streams, invalid/non-record selections, out-of-range coordinates, and
    /// revision-table width changes without mutating the stream.
    pub fn relocate_expanded_record(
        &mut self,
        selected: usize,
        screen: u8,
        x: u8,
        y: u16,
        vertical: bool,
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
                SpriteToken::Control(value) if (0x80..=0xfd).contains(value) => {}
                SpriteToken::Control(value) => {
                    return Err(LevelEditError::InvalidExpandedSpriteControl {
                        index,
                        value: *value,
                    });
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
                    let orientation_nibble = if vertical { fields.y_low & 0x0f } else { 0 };
                    records.push((
                        fields.screen,
                        record_upper_y,
                        orientation_nibble,
                        index,
                        record,
                    ));
                }
            }
        }
        if !records.iter().any(|(_, _, _, index, _)| *index == selected) {
            return Err(LevelEditError::LegacyIncompatibleSpriteToken { index: selected });
        }
        records.sort_by_key(|(screen, upper_y, orientation_nibble, _, _)| {
            (*screen, *upper_y, *orientation_nibble)
        });
        let mut rebuilt = Vec::with_capacity(records.len().saturating_mul(2));
        let mut emitted_upper_y = 0_u8;
        let mut new_selected = None;
        for (_, record_upper_y, _, original_index, record) in records {
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
        self.canonicalize_framing();
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

    fn sprite_at(screen: u8, y_low: u8, id: u8) -> SpriteToken {
        let SpriteToken::Record(mut record) = sprite_on_screen(screen, id) else {
            unreachable!();
        };
        let mut fields = record.native_fields().unwrap();
        fields.y_low = y_low;
        record
            .set_native_fields(fields, &SpriteLengthTable::standard())
            .unwrap();
        SpriteToken::Record(record)
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
    fn absolute_legacy_sprite_place_and_relocate_sort_and_reject_high_y_atomically() {
        let lengths = SpriteLengthTable::standard();
        let mut stream = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![sprite_on_screen(2, 0x10), sprite_on_screen(1, 0x20)],
        };
        let selected = stream
            .place_record_at_position(
                SpriteRecord {
                    encoded: vec![0x08, 0x00, 0x47],
                },
                0x1f,
                0x0c,
                0x1a,
                false,
                &lengths,
            )
            .unwrap();
        assert_eq!(selected, 2);
        let fields = match &stream.tokens[selected] {
            SpriteToken::Record(record) => record.native_fields().unwrap(),
            SpriteToken::Screen(_) | SpriteToken::Control(_) => unreachable!(),
        };
        assert_eq!((fields.screen, fields.x, fields.y_low), (0x1f, 0x0c, 0x1a));
        assert_eq!((fields.extra_bits, fields.sprite_number), (2, 0x47));

        let selected = stream
            .relocate_record_position(selected, 0, 3, 9, false, &lengths)
            .unwrap();
        assert_eq!(selected, 0);
        assert_eq!(stream.native_placements()[0].sprite_number, 0x47);
        let original = stream.clone();
        assert!(matches!(
            stream.relocate_record_position(0, 0, 0, 0x20, false, &lengths),
            Err(LevelEditError::LegacySpriteYOutOfRange(0x20))
        ));
        assert_eq!(stream, original);
    }

    #[test]
    fn absolute_expanded_sprite_place_and_relocate_rebuild_upper_y_controls() {
        let lengths = SpriteLengthTable::standard();
        let mut stream = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![sprite_on_screen(1, 0x10)],
        };
        let selected = stream
            .place_record_at_position(
                SpriteRecord {
                    encoded: vec![0x04, 0x00, 0x47],
                },
                0x1e,
                0x0a,
                4 * 32 + 0x1d,
                false,
                &lengths,
            )
            .unwrap();
        assert_eq!(stream.native_placements()[1].sprite_number, 0x47);
        assert_eq!(stream.native_placements()[1].minor, 4 * 32 + 0x1d);
        assert!(matches!(
            stream.tokens[selected - 1],
            SpriteToken::Screen(4)
        ));

        let selected = stream
            .relocate_record_position(selected, 0, 2, 2 * 32 + 7, true, &lengths)
            .unwrap();
        let placement = stream
            .native_placements()
            .into_iter()
            .find(|placement| placement.sprite_number == 0x47)
            .unwrap();
        assert_eq!(
            (placement.screen, placement.major, placement.minor),
            (0, 2, 71)
        );
        assert!(matches!(
            stream.tokens[selected - 1],
            SpriteToken::Screen(2)
        ));
    }

    #[test]
    fn semantic_record_fields_preserve_extensions_upper_y_and_track_reordering() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(2, 0x42, 5).unwrap();
        let custom = SpriteToken::Record(SpriteRecord {
            encoded: vec![0x08, 0x00, 0x42, 0xaa, 0xbb],
        });
        let fields = NativeSpriteRecordFields {
            y_low: 0x1d,
            extra_bits: 2,
            screen: 0x1f,
            x: 0x0c,
            sprite_number: 0x42,
        };

        let mut legacy = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![sprite_on_screen(2, 1), custom.clone()],
        };
        let selected = legacy
            .set_record_fields(1, fields, false, &lengths)
            .unwrap();
        assert_eq!(selected, 1);
        let SpriteToken::Record(record) = &legacy.tokens[selected] else {
            unreachable!();
        };
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);
        assert_eq!(record.native_fields().unwrap(), fields);

        let mut expanded = NativeSpriteStream {
            header: NativeSpriteStream::EXPANDED_HEADER_FLAG,
            expanded: true,
            tokens: vec![SpriteToken::Screen(4), custom],
        };
        let selected = expanded
            .set_record_fields(1, fields, true, &lengths)
            .unwrap();
        let placement = expanded.native_placements()[0];
        assert_eq!(
            (placement.screen, placement.major, placement.minor),
            (0x1f, 0x1fc, 0x9d)
        );
        assert!(matches!(
            expanded.tokens[selected - 1],
            SpriteToken::Screen(4)
        ));
        let SpriteToken::Record(record) = &expanded.tokens[selected] else {
            unreachable!();
        };
        assert_eq!(&record.encoded[3..], [0xaa, 0xbb]);

        let original = expanded.clone();
        let mut invalid = fields;
        invalid.extra_bits = 4;
        assert!(
            expanded
                .set_record_fields(selected, invalid, true, &lengths)
                .is_err()
        );
        assert_eq!(expanded, original);
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
    fn expanded_semantic_canonicalization_uses_orientation_and_minimum_transitions() {
        let source = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(2),
                sprite_at(1, 15, 0x10),
                SpriteToken::Screen(1),
                sprite_at(0, 9, 0x20),
                sprite_at(0, 2, 0x30),
                SpriteToken::Screen(2),
                sprite_at(1, 1, 0x40),
            ],
        };
        let ids = |stream: &NativeSpriteStream| {
            stream
                .tokens
                .iter()
                .filter_map(|token| match token {
                    SpriteToken::Record(record) => Some(record.encoded[2]),
                    SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
                })
                .collect::<Vec<_>>()
        };

        let mut horizontal = source.clone();
        horizontal.canonicalize_for_orientation(false).unwrap();
        assert_eq!(ids(&horizontal), [0x20, 0x30, 0x10, 0x40]);
        assert_eq!(
            horizontal
                .tokens
                .iter()
                .filter(|token| matches!(token, SpriteToken::Screen(_)))
                .count(),
            2
        );

        let mut vertical = source;
        vertical.canonicalize_for_orientation(true).unwrap();
        assert_eq!(ids(&vertical), [0x30, 0x20, 0x40, 0x10]);
    }

    #[test]
    fn expanded_semantic_canonicalization_failure_is_atomic() {
        let mut stream = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![sprite(1), SpriteToken::Control(0xfe)],
        };
        let original = stream.clone();
        assert!(stream.canonicalize_for_orientation(false).is_err());
        assert_eq!(stream, original);
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
            .relocate_expanded_record(3, 7, 9, 6 * 32 + 29, false, &SpriteLengthTable::standard())
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
        assert_eq!(stream.header, 0x7a);
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
            .relocate_expanded_record(1, 1, 5, 2 * 32 + 31, false, &SpriteLengthTable::standard())
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
    fn vertical_expanded_relocation_uses_the_recovered_coordinate_nibble_tiebreaker() {
        let mut stream = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(2),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0xa0, 0x05, 0x10],
                }),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x20, 0x05, 0x20],
                }),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x80, 0x05, 0x30],
                }),
            ],
        };

        let selected = stream
            .relocate_expanded_record(1, 5, 0, 2 * 32 + 10, true, &SpriteLengthTable::standard())
            .unwrap();
        assert_eq!(selected, 3);
        assert_eq!(
            stream
                .tokens
                .iter()
                .filter_map(|token| match token {
                    SpriteToken::Record(record) => Some(record.encoded[2]),
                    SpriteToken::Screen(_) | SpriteToken::Control(_) => None,
                })
                .collect::<Vec<_>>(),
            [0x20, 0x30, 0x10]
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
                .relocate_expanded_record(0, 0x1e, 3, 0x7f, false, &lengths)
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
    fn expanded_relocation_downgrades_when_no_token_requires_expanded_framing() {
        let mut stream = NativeSpriteStream {
            header: 0x6a,
            expanded: true,
            tokens: vec![SpriteToken::Screen(2), sprite(1)],
        };

        assert_eq!(
            stream
                .relocate_expanded_record(1, 0, 0, 7, false, &SpriteLengthTable::standard())
                .unwrap(),
            0
        );
        assert!(!stream.expanded);
        assert_eq!(stream.header, 0x4a);
        assert_eq!(
            stream.tokens,
            [SpriteToken::Record(SpriteRecord {
                encoded: vec![0x70, 0, 1],
            })]
        );
        assert_eq!(stream.encode_checked().unwrap(), [0x4a, 0x70, 0, 1, 0xff]);
    }

    #[test]
    fn expanded_relocation_strips_ignored_controls_without_changing_upper_y_state() {
        let mut stream = NativeSpriteStream {
            header: 0x20,
            expanded: true,
            tokens: vec![
                SpriteToken::Control(0x80),
                SpriteToken::Screen(2),
                SpriteToken::Control(0xfd),
                sprite(1),
                SpriteToken::Control(0x90),
                sprite(2),
            ],
        };

        assert_eq!(
            stream
                .relocate_expanded_record(
                    5,
                    1,
                    3,
                    2 * 32 + 7,
                    false,
                    &SpriteLengthTable::standard()
                )
                .unwrap(),
            2
        );
        assert_eq!(
            stream.tokens,
            [
                SpriteToken::Screen(2),
                sprite(1),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x70, 0x31, 2],
                }),
            ]
        );
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
                    tokens: vec![SpriteToken::Control(0xfe), sprite(1)],
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
                    .relocate_expanded_record(
                        index,
                        0,
                        0,
                        y,
                        false,
                        &SpriteLengthTable::standard(),
                    )
                    .is_err()
            );
            assert_eq!(stream, original);
        }
    }
}
