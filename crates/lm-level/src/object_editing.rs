use crate::{
    LevelEditError, ObjectCoordinateNibbles, ObjectFieldError, ObjectRecord, ObjectStream,
    ObjectStreamError,
};
use std::fmt;

/// One ordered mutation in an atomic object-stream batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectEdit {
    Insert {
        index: usize,
        record: ObjectRecord,
    },
    /// Inserts an ordinary object at an absolute screen and canonicalizes screen transitions.
    InsertOrdinaryAt {
        record: ObjectRecord,
        screen: u16,
        coordinates: ObjectCoordinateNibbles,
    },
    /// Inserts an ordinary object with an explicit perpendicular coordinate bit 4.
    InsertOrdinaryAtPosition {
        record: ObjectRecord,
        screen: u16,
        coordinates: ObjectCoordinateNibbles,
        perpendicular_high: bool,
    },
    Replace {
        index: usize,
        record: ObjectRecord,
    },
    Remove {
        index: usize,
    },
    /// Moves one object before an index in the ordering that exists when this command runs.
    MoveBefore {
        from: usize,
        before: usize,
    },
    /// Changes the recovered distributed six-bit command field without replacing raw bytes.
    SetCommandId {
        index: usize,
        command_id: u8,
    },
    /// Changes the command-specific third byte without replacing raw bytes.
    SetParameter {
        index: usize,
        parameter: u8,
    },
    /// Changes the two orientation-neutral encoded coordinate nibbles.
    SetCoordinateNibbles {
        index: usize,
        coordinates: ObjectCoordinateNibbles,
    },
    /// Changes the encoded new-screen/advance-screen bit.
    SetAdvancesScreen {
        index: usize,
        advances: bool,
    },
    /// Replaces every proven field of an ordinary object at an absolute position.
    SetOrdinaryFields {
        index: usize,
        fields: NativeObjectRecordFields,
    },
    /// Changes the exact packed target of an existing screen-jump control record.
    SetScreenJumpTarget {
        index: usize,
        packed_target: u16,
    },
    /// Canonically rewrites an existing screen-exit source screen and destination/flags value.
    SetScreenExit {
        index: usize,
        screen: u8,
        destination_and_flags: u16,
    },
    /// Relocates one ordinary object and canonically regenerates owned screen transitions.
    RelocateOrdinary {
        index: usize,
        screen: u16,
        coordinates: ObjectCoordinateNibbles,
    },
    /// Relocates an ordinary object with an explicit perpendicular coordinate bit 4.
    RelocateOrdinaryPosition {
        index: usize,
        screen: u16,
        coordinates: ObjectCoordinateNibbles,
        perpendicular_high: bool,
    },
    /// Clones a positioned selection and translates every clone by one shared native tile delta.
    DuplicateOrdinaryGroup {
        selected: Vec<usize>,
        major_delta: i32,
        minor_delta: i32,
    },
    /// Translates a positioned selection by one shared native tile delta.
    RelocateOrdinaryGroup {
        selected: Vec<usize>,
        major_delta: i32,
        minor_delta: i32,
    },
    /// Moves a positioned selection by one creation/Z-order step without changing coordinates.
    AdjustOrdinaryZOrder {
        selected: Vec<usize>,
        increase: bool,
    },
}

/// Proven semantic fields of one positioned native object record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeObjectRecordFields {
    pub command_id: u8,
    pub parameter: u8,
    pub screen: u16,
    pub coordinates: ObjectCoordinateNibbles,
    pub perpendicular_high: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectEditError {
    Command {
        command: usize,
        error: LevelEditError,
    },
    Field {
        command: usize,
        error: ObjectFieldError,
    },
    BankLimitExceeded,
}

impl fmt::Display for ObjectEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid atomic object edit: {self:?}")
    }
}

impl std::error::Error for ObjectEditError {}

impl ObjectStream {
    /// Applies ordered object mutations atomically and enforces the native single-bank encoding
    /// limit, including the stream terminator.
    ///
    /// Each command observes the result of preceding commands. An empty batch is a no-op. If any
    /// command or final serialization check fails, the original stream is unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectEditError`] with the failing command index, or
    /// [`ObjectEditError::BankLimitExceeded`] when the resulting stream cannot be stored natively.
    #[allow(clippy::too_many_lines)]
    pub fn apply_edits(&mut self, edits: &[ObjectEdit]) -> Result<(), ObjectEditError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        for (command, edit) in edits.iter().enumerate() {
            let result = match edit {
                ObjectEdit::Insert { index, record } => staged.insert(*index, record.clone()),
                ObjectEdit::InsertOrdinaryAt {
                    record,
                    screen,
                    coordinates,
                } => staged
                    .insert_ordinary_object_at(record.clone(), *screen, *coordinates)
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::InsertOrdinaryAtPosition {
                    record,
                    screen,
                    coordinates,
                    perpendicular_high,
                } => staged
                    .insert_ordinary_object_at_position(
                        record.clone(),
                        *screen,
                        *coordinates,
                        *perpendicular_high,
                    )
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::Replace { index, record } => {
                    let Some(target) = staged.records.get_mut(*index) else {
                        return Err(ObjectEditError::Command {
                            command,
                            error: LevelEditError::IndexOutOfBounds {
                                index: *index,
                                len: staged.records.len(),
                            },
                        });
                    };
                    *target = record.clone();
                    Ok(())
                }
                ObjectEdit::Remove { index } => staged.remove(*index).map(drop),
                ObjectEdit::MoveBefore { from, before } => staged.move_before(*from, *before),
                ObjectEdit::SetCommandId { index, command_id } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_command_id(*command_id)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::SetParameter { index, parameter } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_parameter(*parameter)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::SetCoordinateNibbles { index, coordinates } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_coordinate_nibbles(*coordinates)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::SetAdvancesScreen { index, advances } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_advances_screen(*advances)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::SetOrdinaryFields { index, fields } => {
                    staged.set_ordinary_fields(*index, *fields).map(drop)
                }
                ObjectEdit::SetScreenJumpTarget {
                    index,
                    packed_target,
                } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_screen_jump_target(*packed_target)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::SetScreenExit {
                    index,
                    screen,
                    destination_and_flags,
                } => {
                    let target = record_mut(&mut staged, command, *index)?;
                    target
                        .set_screen_exit(*screen, *destination_and_flags)
                        .map_err(|error| ObjectEditError::Field { command, error })?;
                    Ok(())
                }
                ObjectEdit::RelocateOrdinary {
                    index,
                    screen,
                    coordinates,
                } => staged
                    .relocate_ordinary_object(*index, *screen, *coordinates)
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::RelocateOrdinaryPosition {
                    index,
                    screen,
                    coordinates,
                    perpendicular_high,
                } => staged
                    .relocate_ordinary_object_position(
                        *index,
                        *screen,
                        *coordinates,
                        *perpendicular_high,
                    )
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::DuplicateOrdinaryGroup {
                    selected,
                    major_delta,
                    minor_delta,
                } => staged
                    .duplicate_ordinary_object_group(selected, *major_delta, *minor_delta)
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::RelocateOrdinaryGroup {
                    selected,
                    major_delta,
                    minor_delta,
                } => staged
                    .relocate_ordinary_object_group(selected, *major_delta, *minor_delta)
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
                ObjectEdit::AdjustOrdinaryZOrder { selected, increase } => staged
                    .adjust_ordinary_object_z_order(selected, *increase)
                    .map(drop)
                    .map_err(LevelEditError::ObjectRelocation),
            };
            result.map_err(|error| ObjectEditError::Command { command, error })?;
        }
        staged.encode_banked().map_err(|error| match error {
            ObjectStreamError::BankLimitExceeded | ObjectStreamError::SizeOverflow => {
                ObjectEditError::BankLimitExceeded
            }
            _ => unreachable!("validated ObjectRecord values always serialize"),
        })?;
        *self = staged;
        Ok(())
    }
}

impl ObjectStream {
    /// Replaces all proven semantic fields of one ordinary object, preserves extension bytes, and
    /// tracks it through canonical absolute-screen ordering. Returns its resulting record index.
    ///
    /// # Errors
    ///
    /// Rejects invalid indexes, shape-changing fields, controls, and unrepresentable positions
    /// without mutating the stream.
    pub fn set_ordinary_fields(
        &mut self,
        selected: usize,
        fields: NativeObjectRecordFields,
    ) -> Result<usize, LevelEditError> {
        let mut staged = self.clone();
        let len = staged.records.len();
        let record = staged
            .records
            .get_mut(selected)
            .ok_or(LevelEditError::IndexOutOfBounds {
                index: selected,
                len,
            })?;
        if !record.is_positioned_object() {
            return Err(LevelEditError::ObjectRelocation(
                crate::ObjectRelocationError::NotOrdinaryObject(selected),
            ));
        }
        record
            .set_command_id(fields.command_id)
            .and_then(|()| record.set_parameter(fields.parameter))
            .map_err(LevelEditError::ObjectField)?;
        let selected = staged
            .relocate_ordinary_object_position(
                selected,
                fields.screen,
                fields.coordinates,
                fields.perpendicular_high,
            )
            .map_err(LevelEditError::ObjectRelocation)?;
        *self = staged;
        Ok(selected)
    }
}

fn record_mut(
    stream: &mut ObjectStream,
    command: usize,
    index: usize,
) -> Result<&mut ObjectRecord, ObjectEditError> {
    let len = stream.records.len();
    stream
        .records
        .get_mut(index)
        .ok_or(ObjectEditError::Command {
            command,
            error: LevelEditError::IndexOutOfBounds { index, len },
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u8) -> ObjectRecord {
        ObjectRecord::new(vec![id, 0, 1]).unwrap()
    }

    #[test]
    fn ordered_batch_round_trips_through_native_stream() {
        let mut stream = ObjectStream {
            records: vec![record(1), record(2), record(3)],
        };
        stream
            .apply_edits(&[
                ObjectEdit::MoveBefore { from: 2, before: 0 },
                ObjectEdit::Replace {
                    index: 1,
                    record: record(4),
                },
                ObjectEdit::Remove { index: 2 },
                ObjectEdit::Insert {
                    index: 2,
                    record: record(5),
                },
            ])
            .unwrap();
        let encoded = stream.encode_banked().unwrap();
        assert_eq!(encoded, [3, 0, 1, 4, 0, 1, 5, 0, 1, 0xff]);
        assert_eq!(ObjectStream::parse(&encoded).unwrap(), stream);
    }

    #[test]
    fn late_failure_rolls_back_every_prior_command() {
        let mut stream = ObjectStream {
            records: vec![record(1), record(2)],
        };
        let original = stream.clone();
        let error = stream
            .apply_edits(&[
                ObjectEdit::Remove { index: 0 },
                ObjectEdit::Replace {
                    index: 4,
                    record: record(9),
                },
            ])
            .unwrap_err();
        assert!(matches!(error, ObjectEditError::Command { command: 1, .. }));
        assert_eq!(stream, original);
    }

    #[test]
    fn final_bank_limit_failure_is_atomic() {
        let record = ObjectRecord::new(vec![1; 8]).unwrap();
        let mut stream = ObjectStream {
            records: vec![record.clone(); 0x8000 / 8 - 1],
        };
        let original = stream.clone();
        assert_eq!(
            stream.apply_edits(&[ObjectEdit::Insert {
                index: stream.records.len(),
                record,
            }]),
            Err(ObjectEditError::BankLimitExceeded)
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn typed_field_edits_preserve_unowned_and_extension_bytes() {
        let mut stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x9f, 0x0a, 1, 0xaa]).unwrap()],
        };
        stream
            .apply_edits(&[
                ObjectEdit::SetCommandId {
                    index: 0,
                    command_id: 0x22,
                },
                ObjectEdit::SetParameter {
                    index: 0,
                    parameter: 0x0f,
                },
                ObjectEdit::SetCoordinateNibbles {
                    index: 0,
                    coordinates: ObjectCoordinateNibbles {
                        first: 3,
                        second: 4,
                    },
                },
                ObjectEdit::SetAdvancesScreen {
                    index: 0,
                    advances: true,
                },
            ])
            .unwrap();
        let record = &stream.records[0];
        assert_eq!(record.command_id(), 0x22);
        assert_eq!(record.parameter(), 0x0f);
        assert_eq!(
            record.coordinate_nibbles(),
            ObjectCoordinateNibbles {
                first: 3,
                second: 4
            }
        );
        assert!(record.advances_screen());
        assert_eq!(record.encoded()[3], 0xaa);
    }

    #[test]
    fn semantic_ordinary_fields_preserve_extensions_reorder_and_roll_back() {
        let mut custom = ObjectRecord::new(vec![0x9f, 0x0a, 1, 0xaa]).unwrap();
        custom.set_command_id(0x22).unwrap();
        custom.set_parameter(0x0f).unwrap();
        let command_id = 0x22;
        let mut stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x00, 0x10, 2]).unwrap(),
                ObjectRecord::new(vec![0x02, 0x00, 1]).unwrap(),
                custom,
            ],
        };
        let fields = NativeObjectRecordFields {
            command_id,
            parameter: 0x0f,
            screen: 0,
            coordinates: ObjectCoordinateNibbles {
                first: 3,
                second: 4,
            },
            perpendicular_high: true,
        };
        let selected = stream.set_ordinary_fields(2, fields).unwrap();
        assert_eq!(selected, 1);
        let record = &stream.records[selected];
        assert_eq!(record.command_id(), command_id);
        assert_eq!(record.parameter(), 0x0f);
        assert_eq!(record.encoded()[3], 0xaa);
        assert!(record.perpendicular_high_coordinate());
        assert_eq!(stream.native_placements()[selected].screen, 0);

        let original = stream.clone();
        let mut invalid = fields;
        invalid.command_id = 0;
        invalid.parameter = 1;
        assert!(stream.set_ordinary_fields(selected, invalid).is_err());
        assert_eq!(stream, original);

        let mut control = ObjectStream {
            records: vec![ObjectRecord::new(vec![2, 0, 1]).unwrap()],
        };
        let original = control.clone();
        assert!(control.set_ordinary_fields(0, fields).is_err());
        assert_eq!(control, original);
    }

    #[test]
    fn late_typed_field_failure_rolls_back_the_batch() {
        let mut stream = ObjectStream {
            records: vec![record(1), record(2)],
        };
        let original = stream.clone();
        let error = stream
            .apply_edits(&[
                ObjectEdit::SetParameter {
                    index: 0,
                    parameter: 0x7f,
                },
                ObjectEdit::SetCommandId {
                    index: 1,
                    command_id: 0x40,
                },
            ])
            .unwrap_err();
        assert_eq!(
            error,
            ObjectEditError::Field {
                command: 1,
                error: ObjectFieldError::InvalidCommandId(0x40),
            }
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn screen_jump_target_edit_is_transactional() {
        let mut stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x01, 0x02, 1]).unwrap(), record(2)],
        };
        stream
            .apply_edits(&[ObjectEdit::SetScreenJumpTarget {
                index: 0,
                packed_target: 0x0a1b,
            }])
            .unwrap();
        assert_eq!(stream.records[0].encoded(), &[0x1b, 0x0a, 1]);

        let original = stream.clone();
        assert!(
            stream
                .apply_edits(&[
                    ObjectEdit::SetParameter {
                        index: 1,
                        parameter: 0x7f,
                    },
                    ObjectEdit::SetScreenJumpTarget {
                        index: 1,
                        packed_target: 1,
                    },
                ])
                .is_err()
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn screen_exit_edit_changes_shape_canonically_and_is_transactional() {
        let source = ObjectRecord::new(vec![0x85, 0x0a, 0, 0x34]).unwrap();
        let mut stream = ObjectStream {
            records: vec![source.clone(), record(2)],
        };
        stream
            .apply_edits(&[ObjectEdit::SetScreenExit {
                index: 0,
                screen: 0x1f,
                destination_and_flags: 0x1000,
            }])
            .unwrap();
        assert_eq!(stream.records[0].encoded(), &[0x9f, 0, 2, 0, 0x14]);

        let original = stream.clone();
        assert!(matches!(
            stream.apply_edits(&[
                ObjectEdit::SetParameter {
                    index: 1,
                    parameter: 0x7f,
                },
                ObjectEdit::SetScreenExit {
                    index: 1,
                    screen: 0,
                    destination_and_flags: 0,
                },
            ]),
            Err(ObjectEditError::Field {
                command: 1,
                error: ObjectFieldError::NotScreenExit,
            })
        ));
        assert_eq!(stream, original);
    }

    #[test]
    fn absolute_insert_is_available_through_atomic_edit_batches() {
        let mut stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![1, 0x10, 1]).unwrap()],
        };
        stream
            .apply_edits(&[ObjectEdit::InsertOrdinaryAt {
                record: ObjectRecord::new(vec![2, 0x10, 2]).unwrap(),
                screen: 7,
                coordinates: ObjectCoordinateNibbles {
                    first: 5,
                    second: 6,
                },
            }])
            .unwrap();
        let placements = stream.native_placements();
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[1].screen, 7);
        assert_eq!(placements[1].major, 7 * 16 + 6);
        assert_eq!(placements[1].minor, 5);
    }
}
