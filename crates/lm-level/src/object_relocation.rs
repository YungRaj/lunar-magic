use crate::{ObjectCoordinateNibbles, ObjectFieldError, ObjectRecord, ObjectStream};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectRelocationError {
    IndexOutOfBounds { index: usize, len: usize },
    NotOrdinaryObject(usize),
    UnsupportedControl(usize),
    TargetScreenOutOfRange(u16),
    Field(ObjectFieldError),
}

impl fmt::Display for ObjectRelocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native object relocation: {self:?}")
    }
}

impl std::error::Error for ObjectRelocationError {}

#[derive(Clone)]
struct PositionedObject {
    original_index: usize,
    screen: u16,
    record: ObjectRecord,
}

impl ObjectStream {
    /// Inserts an ordinary object at an absolute native screen and coordinate pair.
    ///
    /// Existing owned screen transitions are regenerated canonically. The new object is placed
    /// after existing objects on the target screen, while trailing opaque controls are preserved.
    ///
    /// # Errors
    ///
    /// Rejects command-zero controls, invalid coordinate fields, targets outside screens 0–31,
    /// and streams containing opaque controls interleaved with ordinary objects.
    pub fn insert_ordinary_object_at(
        &mut self,
        record: ObjectRecord,
        target_screen: u16,
        coordinates: ObjectCoordinateNibbles,
    ) -> Result<usize, ObjectRelocationError> {
        let perpendicular_high = record.perpendicular_high_coordinate();
        self.insert_ordinary_object_at_position(
            record,
            target_screen,
            coordinates,
            perpendicular_high,
        )
    }

    /// Inserts an ordinary object and explicitly sets its perpendicular coordinate bit 4.
    ///
    /// # Errors
    ///
    /// Applies the same validation and atomicity guarantees as
    /// [`Self::insert_ordinary_object_at`].
    pub fn insert_ordinary_object_at_position(
        &mut self,
        mut record: ObjectRecord,
        target_screen: u16,
        coordinates: ObjectCoordinateNibbles,
        perpendicular_high: bool,
    ) -> Result<usize, ObjectRelocationError> {
        if target_screen > 0x1f {
            return Err(ObjectRelocationError::TargetScreenOutOfRange(target_screen));
        }
        if record.command_id() == 0 {
            return Err(ObjectRelocationError::NotOrdinaryObject(self.records.len()));
        }
        record
            .set_coordinate_nibbles(coordinates)
            .map_err(ObjectRelocationError::Field)?;
        record
            .set_perpendicular_high_coordinate(perpendicular_high)
            .map_err(ObjectRelocationError::Field)?;
        record
            .set_advances_screen(false)
            .map_err(ObjectRelocationError::Field)?;
        let (mut positioned, trailing_controls) = decode_positioned_objects(self)?;
        let inserted_id = self.records.len();
        positioned.push(PositionedObject {
            original_index: inserted_id,
            screen: target_screen,
            record,
        });
        positioned.sort_by_key(|object| object.screen);
        let (mut records, new_index) = encode_positioned_objects(positioned, inserted_id)?;
        records.extend(trailing_controls);
        self.records = records;
        Ok(new_index)
    }

    /// Relocates an ordinary object to an absolute native screen and coordinate pair.
    ///
    /// Existing screen-jump controls are owned by this operation and are canonically regenerated.
    /// Ordinary records retain source order within one screen, all extension bytes, and every
    /// field other than coordinates and the transition-owned advance bit.
    ///
    /// Returns the selected ordinary record's new stream index, including regenerated jumps.
    ///
    /// # Errors
    ///
    /// Rejects invalid indexes, nonordinary selections, unknown command-zero controls, screens
    /// outside the native 0–31 editor range, and field encodings that cannot be represented.
    /// Failure leaves the stream unchanged.
    pub fn relocate_ordinary_object(
        &mut self,
        selected: usize,
        target_screen: u16,
        coordinates: ObjectCoordinateNibbles,
    ) -> Result<usize, ObjectRelocationError> {
        let perpendicular_high = self
            .records
            .get(selected)
            .ok_or(ObjectRelocationError::IndexOutOfBounds {
                index: selected,
                len: self.records.len(),
            })?
            .perpendicular_high_coordinate();
        self.relocate_ordinary_object_position(
            selected,
            target_screen,
            coordinates,
            perpendicular_high,
        )
    }

    /// Relocates an ordinary object and explicitly sets its perpendicular coordinate bit 4.
    ///
    /// # Errors
    ///
    /// Applies the same validation and atomicity guarantees as
    /// [`Self::relocate_ordinary_object`].
    pub fn relocate_ordinary_object_position(
        &mut self,
        selected: usize,
        target_screen: u16,
        coordinates: ObjectCoordinateNibbles,
        perpendicular_high: bool,
    ) -> Result<usize, ObjectRelocationError> {
        if selected >= self.records.len() {
            return Err(ObjectRelocationError::IndexOutOfBounds {
                index: selected,
                len: self.records.len(),
            });
        }
        if target_screen > 0x1f {
            return Err(ObjectRelocationError::TargetScreenOutOfRange(target_screen));
        }
        let (mut positioned, trailing_controls) = decode_positioned_objects(self)?;
        let Some(target) = positioned
            .iter_mut()
            .find(|object| object.original_index == selected)
        else {
            return Err(ObjectRelocationError::NotOrdinaryObject(selected));
        };
        target.screen = target_screen;
        target
            .record
            .set_coordinate_nibbles(coordinates)
            .map_err(ObjectRelocationError::Field)?;
        target
            .record
            .set_perpendicular_high_coordinate(perpendicular_high)
            .map_err(ObjectRelocationError::Field)?;
        positioned.sort_by_key(|object| object.screen);
        let (mut records, new_index) = encode_positioned_objects(positioned, selected)?;
        records.extend(trailing_controls);
        self.records = records;
        Ok(new_index)
    }
}

fn decode_positioned_objects(
    stream: &ObjectStream,
) -> Result<(Vec<PositionedObject>, Vec<ObjectRecord>), ObjectRelocationError> {
    let mut screen = 0_u16;
    let mut output = Vec::with_capacity(stream.records.len());
    let mut trailing_controls = Vec::new();
    for (index, record) in stream.records.iter().enumerate() {
        if let Some(jump) = record.screen_jump() {
            if !trailing_controls.is_empty() {
                return Err(ObjectRelocationError::UnsupportedControl(index));
            }
            screen = jump.packed_target;
            continue;
        }
        if record.command_id() == 0 && record.parameter() <= 3 {
            trailing_controls.push(record.clone());
            continue;
        }
        if !trailing_controls.is_empty() {
            return Err(ObjectRelocationError::UnsupportedControl(
                index - trailing_controls.len(),
            ));
        }
        if record.advances_screen() {
            screen = screen.saturating_add(1);
        }
        output.push(PositionedObject {
            original_index: index,
            screen,
            record: record.clone(),
        });
    }
    Ok((output, trailing_controls))
}

fn encode_positioned_objects(
    positioned: Vec<PositionedObject>,
    selected: usize,
) -> Result<(Vec<ObjectRecord>, usize), ObjectRelocationError> {
    let mut screen = 0_u16;
    let mut output = Vec::with_capacity(positioned.len());
    let mut selected_index = None;
    for mut object in positioned {
        let can_advance = object.screen == screen.saturating_add(1);
        let transition = if object.screen == screen {
            false
        } else if can_advance && object.record.set_advances_screen(true).is_ok() {
            screen = object.screen;
            true
        } else {
            output.push(canonical_screen_jump(object.screen)?);
            screen = object.screen;
            false
        };
        if !transition {
            object
                .record
                .set_advances_screen(false)
                .map_err(ObjectRelocationError::Field)?;
        }
        if object.original_index == selected {
            selected_index = Some(output.len());
        }
        output.push(object.record);
    }
    let new_index = selected_index.ok_or(ObjectRelocationError::NotOrdinaryObject(selected))?;
    Ok((output, new_index))
}

fn canonical_screen_jump(screen: u16) -> Result<ObjectRecord, ObjectRelocationError> {
    let target =
        u8::try_from(screen).map_err(|_| ObjectRelocationError::TargetScreenOutOfRange(screen))?;
    ObjectRecord::new(vec![target, 0, 1])
        .map_err(|_| ObjectRelocationError::Field(ObjectFieldError::UnknownEncodedLength))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(screen_advance: bool, first: u8, second: u8, id: u8) -> ObjectRecord {
        ObjectRecord::new(vec![
            first | if screen_advance { 0x80 } else { 0 },
            0x10 | second,
            id,
        ])
        .unwrap()
    }

    #[test]
    fn explicit_position_mutations_cross_the_perpendicular_half_boundary() {
        let mut stream = ObjectStream {
            records: vec![object(false, 2, 3, 4)],
        };
        let coordinates = ObjectCoordinateNibbles {
            first: 5,
            second: 6,
        };
        let index = stream
            .relocate_ordinary_object_position(0, 0, coordinates, true)
            .unwrap();
        assert!(stream.records[index].perpendicular_high_coordinate());
        assert_eq!(stream.native_placements()[0].minor, 0x15);

        let index = stream
            .relocate_ordinary_object_position(index, 0, coordinates, false)
            .unwrap();
        assert!(!stream.records[index].perpendicular_high_coordinate());
        assert_eq!(stream.native_placements()[0].minor, 5);
    }

    #[test]
    fn relocation_reorders_stably_and_regenerates_minimal_transitions() {
        let mut stream = ObjectStream {
            records: vec![
                object(false, 1, 2, 0x10),
                object(true, 3, 4, 0x20),
                object(false, 5, 6, 0x30),
            ],
        };
        let selected = stream
            .relocate_ordinary_object(
                0,
                2,
                ObjectCoordinateNibbles {
                    first: 7,
                    second: 8,
                },
            )
            .unwrap();
        assert_eq!(selected, 2);
        assert_eq!(
            stream
                .native_placements()
                .into_iter()
                .map(|placement| (placement.record_index, placement.screen))
                .collect::<Vec<_>>(),
            [(0, 1), (1, 1), (2, 2)]
        );
        assert_eq!(stream.records[2].coordinate_nibbles().first, 7);
        assert_eq!(stream.records[2].coordinate_nibbles().second, 8);
    }

    #[test]
    fn gaps_and_backtracking_use_canonical_screen_jumps() {
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), object(false, 3, 4, 0x20)],
        };
        let selected = stream
            .relocate_ordinary_object(
                0,
                5,
                ObjectCoordinateNibbles {
                    first: 1,
                    second: 2,
                },
            )
            .unwrap();
        assert_eq!(selected, 2);
        assert_eq!(stream.records[1].screen_jump().unwrap().packed_target, 5);
        assert_eq!(
            stream
                .native_placements()
                .into_iter()
                .map(|placement| placement.screen)
                .collect::<Vec<_>>(),
            [0, 5]
        );
    }

    #[test]
    fn absolute_insertion_regenerates_transitions_and_preserves_trailing_controls() {
        let control = ObjectRecord::new(vec![7, 5, 0, 0xcb]).unwrap();
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), control.clone()],
        };
        let inserted = stream
            .insert_ordinary_object_at(
                object(true, 9, 9, 0x20),
                5,
                ObjectCoordinateNibbles {
                    first: 3,
                    second: 4,
                },
            )
            .unwrap();
        assert_eq!(inserted, 2);
        assert_eq!(stream.records[1].screen_jump().unwrap().packed_target, 5);
        assert_eq!(stream.records[2].coordinate_nibbles().first, 3);
        assert_eq!(stream.records[2].coordinate_nibbles().second, 4);
        assert_eq!(stream.records.last(), Some(&control));
        assert_eq!(
            stream.native_placements()[1].screen,
            5,
            "the inserted ordinary object has an absolute placement"
        );
    }

    #[test]
    fn invalid_absolute_insertion_is_atomic() {
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10)],
        };
        let original = stream.clone();
        assert!(
            stream
                .insert_ordinary_object_at(
                    ObjectRecord::new(vec![0, 0, 1]).unwrap(),
                    2,
                    ObjectCoordinateNibbles {
                        first: 1,
                        second: 2,
                    },
                )
                .is_err()
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn unsupported_controls_and_invalid_targets_are_atomic() {
        let mut stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0, 0, 2, 0, 0]).unwrap(),
                object(false, 1, 2, 0x10),
            ],
        };
        let original = stream.clone();
        assert_eq!(
            stream.relocate_ordinary_object(
                1,
                2,
                ObjectCoordinateNibbles {
                    first: 1,
                    second: 2,
                },
            ),
            Err(ObjectRelocationError::UnsupportedControl(0))
        );
        assert_eq!(stream, original);
        assert!(
            stream
                .relocate_ordinary_object(
                    1,
                    32,
                    ObjectCoordinateNibbles {
                        first: 1,
                        second: 2,
                    },
                )
                .is_err()
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn trailing_opaque_controls_are_retained_byte_for_byte() {
        let control = ObjectRecord::new(vec![7, 5, 0, 0xcb]).unwrap();
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), control.clone()],
        };
        stream
            .relocate_ordinary_object(
                0,
                3,
                ObjectCoordinateNibbles {
                    first: 4,
                    second: 5,
                },
            )
            .unwrap();
        assert_eq!(stream.records.last(), Some(&control));
        assert_eq!(stream.native_placements()[0].screen, 3);
    }

    #[test]
    fn relocation_preserves_custom_object_extension_bytes() {
        let custom = ObjectRecord::new(vec![0x45, 0x26, 0x42, 0xaa]).unwrap();
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), custom],
        };
        let selected = stream
            .relocate_ordinary_object(
                1,
                4,
                ObjectCoordinateNibbles {
                    first: 9,
                    second: 8,
                },
            )
            .unwrap();
        assert_eq!(stream.records[selected].encoded()[3], 0xaa);
        assert_eq!(
            stream.records[selected].coordinate_nibbles(),
            ObjectCoordinateNibbles {
                first: 9,
                second: 8,
            }
        );
    }
}
