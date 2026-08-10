use crate::{ObjectCoordinateNibbles, ObjectFieldError, ObjectRecord, ObjectStream};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectRelocationError {
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    EmptySelection,
    DuplicateSelection(usize),
    NotOrdinaryObject(usize),
    UnsupportedControl(usize),
    TargetScreenOutOfRange(u16),
    TargetPositionOutOfRange {
        index: usize,
        major: i32,
        minor: i32,
    },
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
    /// Inserts a positioned standard or extended object at an absolute native screen and
    /// coordinate pair.
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

    /// Inserts a positioned standard or extended object and explicitly sets its perpendicular
    /// coordinate bit 4.
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
        if !record.is_positioned_object() {
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

    /// Clones a selected object group and translates every clone by one shared tile delta.
    ///
    /// Selection order is retained in the returned indexes while serialization remains stable by
    /// native screen order. Existing objects are never removed or rewritten except for owned
    /// screen-transition canonicalization. All clones are validated before publication, making an
    /// out-of-bounds member or malformed selection failure-atomic.
    ///
    /// `major_delta` follows the level's screen axis; `minor_delta` follows its perpendicular
    /// 32-tile encoded span. Callers map those orientation-neutral axes to X/Y.
    ///
    /// # Errors
    ///
    /// Rejects an empty or duplicate selection, indexes that do not identify positioned objects,
    /// opaque interleaved controls, and any translated coordinate outside 512×32 native tiles.
    pub fn duplicate_ordinary_object_group(
        &mut self,
        selected: &[usize],
        major_delta: i32,
        minor_delta: i32,
    ) -> Result<Vec<usize>, ObjectRelocationError> {
        if selected.is_empty() {
            return Err(ObjectRelocationError::EmptySelection);
        }
        let (positioned, trailing_controls) = decode_positioned_objects(self)?;
        let original_records = self.records.clone();
        let mut seen = std::collections::BTreeSet::new();
        let mut clones = Vec::with_capacity(selected.len());
        let first_clone_id = self.records.len();
        for (ordinal, index) in selected.iter().copied().enumerate() {
            if !seen.insert(index) {
                return Err(ObjectRelocationError::DuplicateSelection(index));
            }
            if index >= self.records.len() {
                return Err(ObjectRelocationError::IndexOutOfBounds {
                    index,
                    len: self.records.len(),
                });
            }
            let source = positioned
                .iter()
                .find(|object| object.original_index == index)
                .ok_or(ObjectRelocationError::NotOrdinaryObject(index))?;
            let coordinates = source.record.coordinate_nibbles();
            let major = i32::from(source.screen) * 16 + i32::from(coordinates.second) + major_delta;
            let minor = i32::from(coordinates.first)
                + if source.record.perpendicular_high_coordinate() {
                    16
                } else {
                    0
                }
                + minor_delta;
            if !(0..512).contains(&major) || !(0..32).contains(&minor) {
                return Err(ObjectRelocationError::TargetPositionOutOfRange {
                    index,
                    major,
                    minor,
                });
            }
            let invalid_target = || ObjectRelocationError::TargetPositionOutOfRange {
                index,
                major,
                minor,
            };
            let mut record = source.record.clone();
            record
                .set_coordinate_nibbles(ObjectCoordinateNibbles {
                    first: u8::try_from(minor & 0x0f).map_err(|_| invalid_target())?,
                    second: u8::try_from(major & 0x0f).map_err(|_| invalid_target())?,
                })
                .map_err(ObjectRelocationError::Field)?;
            record
                .set_perpendicular_high_coordinate(minor >= 16)
                .map_err(ObjectRelocationError::Field)?;
            record
                .set_advances_screen(false)
                .map_err(ObjectRelocationError::Field)?;
            clones.push(PositionedObject {
                original_index: first_clone_id + ordinal,
                screen: u16::try_from(major / 16).map_err(|_| invalid_target())?,
                record,
            });
        }
        let trailing_start = original_records.len() - trailing_controls.len();
        let mut original_screen = 0_u16;
        let mut record_screens = vec![None; trailing_start];
        let mut last_record_by_screen = std::collections::BTreeMap::new();
        for (index, record) in original_records[..trailing_start].iter().enumerate() {
            if let Some(jump) = record.screen_jump() {
                original_screen = jump.resolved_screen();
            } else if record.is_positioned_object() {
                if record.advances_screen() {
                    original_screen = original_screen.saturating_add(1);
                }
                record_screens[index] = Some(original_screen);
                last_record_by_screen.insert(original_screen, index);
            }
        }
        let mut clones_by_screen =
            std::collections::BTreeMap::<u16, Vec<(usize, ObjectRecord)>>::new();
        for (ordinal, clone) in clones.into_iter().enumerate() {
            clones_by_screen
                .entry(clone.screen)
                .or_default()
                .push((ordinal, clone.record));
        }
        let mut records = Vec::with_capacity(original_records.len() + selected.len() + 1);
        let mut selected_indexes = vec![None; selected.len()];
        for (index, record) in original_records[..trailing_start]
            .iter()
            .cloned()
            .enumerate()
        {
            records.push(record);
            let Some(screen) = record_screens[index] else {
                continue;
            };
            if last_record_by_screen.get(&screen) != Some(&index) {
                continue;
            }
            if let Some(screen_clones) = clones_by_screen.remove(&screen) {
                for (ordinal, clone) in screen_clones {
                    selected_indexes[ordinal] = Some(records.len());
                    records.push(clone);
                }
            }
        }
        for (screen, screen_clones) in clones_by_screen {
            records.push(canonical_screen_jump(screen)?);
            for (ordinal, clone) in screen_clones {
                selected_indexes[ordinal] = Some(records.len());
                records.push(clone);
            }
        }
        records.extend(trailing_controls);
        let selected_indexes = selected_indexes
            .into_iter()
            .enumerate()
            .map(|(ordinal, index)| {
                index.ok_or(ObjectRelocationError::NotOrdinaryObject(selected[ordinal]))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.records = records;
        Ok(selected_indexes)
    }

    /// Translates every selected positioned object by one shared native tile delta.
    ///
    /// The complete group is validated and re-encoded once, retaining selection order and making
    /// any invalid member failure-atomic. Unlike [`Self::duplicate_ordinary_object_group`], this
    /// moves the selected source records instead of cloning them.
    ///
    /// # Errors
    ///
    /// Applies the same selection, control, and 512×32 bounds checks as group duplication.
    pub fn relocate_ordinary_object_group(
        &mut self,
        selected: &[usize],
        major_delta: i32,
        minor_delta: i32,
    ) -> Result<Vec<usize>, ObjectRelocationError> {
        if selected.is_empty() {
            return Err(ObjectRelocationError::EmptySelection);
        }
        let (mut positioned, trailing_controls) = decode_positioned_objects(self)?;
        let mut seen = std::collections::BTreeSet::new();
        for index in selected.iter().copied() {
            if !seen.insert(index) {
                return Err(ObjectRelocationError::DuplicateSelection(index));
            }
            if index >= self.records.len() {
                return Err(ObjectRelocationError::IndexOutOfBounds {
                    index,
                    len: self.records.len(),
                });
            }
            let target = positioned
                .iter_mut()
                .find(|object| object.original_index == index)
                .ok_or(ObjectRelocationError::NotOrdinaryObject(index))?;
            let coordinates = target.record.coordinate_nibbles();
            let major = i32::from(target.screen) * 16 + i32::from(coordinates.second) + major_delta;
            let minor = i32::from(coordinates.first)
                + if target.record.perpendicular_high_coordinate() {
                    16
                } else {
                    0
                }
                + minor_delta;
            if !(0..512).contains(&major) || !(0..32).contains(&minor) {
                return Err(ObjectRelocationError::TargetPositionOutOfRange {
                    index,
                    major,
                    minor,
                });
            }
            let invalid_target = || ObjectRelocationError::TargetPositionOutOfRange {
                index,
                major,
                minor,
            };
            target.screen = u16::try_from(major / 16).map_err(|_| invalid_target())?;
            target
                .record
                .set_coordinate_nibbles(ObjectCoordinateNibbles {
                    first: u8::try_from(minor & 0x0f).map_err(|_| invalid_target())?,
                    second: u8::try_from(major & 0x0f).map_err(|_| invalid_target())?,
                })
                .map_err(ObjectRelocationError::Field)?;
            target
                .record
                .set_perpendicular_high_coordinate(minor >= 16)
                .map_err(ObjectRelocationError::Field)?;
        }
        positioned.sort_by_key(|object| object.screen);
        let (mut records, selected_indexes) = encode_positioned_object_group(positioned, selected)?;
        records.extend(trailing_controls);
        self.records = records;
        Ok(selected_indexes)
    }

    /// Changes the creation/Z order of a positioned object selection by one record while
    /// preserving every object's absolute screen and coordinates. Selected records move as one
    /// stable group past the adjacent unselected record; owned screen transitions are regenerated
    /// so crossing a screen boundary cannot change placement.
    pub fn adjust_ordinary_object_z_order(
        &mut self,
        selected: &[usize],
        increase: bool,
    ) -> Result<Vec<usize>, ObjectRelocationError> {
        if selected.is_empty() {
            return Err(ObjectRelocationError::EmptySelection);
        }
        let (mut positioned, trailing_controls) = decode_positioned_objects(self)?;
        let mut selected_set = std::collections::BTreeSet::new();
        for index in selected.iter().copied() {
            if !selected_set.insert(index) {
                return Err(ObjectRelocationError::DuplicateSelection(index));
            }
            if index >= self.records.len() {
                return Err(ObjectRelocationError::IndexOutOfBounds {
                    index,
                    len: self.records.len(),
                });
            }
            if !positioned
                .iter()
                .any(|object| object.original_index == index)
            {
                return Err(ObjectRelocationError::NotOrdinaryObject(index));
            }
        }
        shift_selected_one_step(
            &mut positioned,
            |object| selected_set.contains(&object.original_index),
            increase,
            |_, _| true,
        );
        let (mut records, selected_indexes) = encode_positioned_object_group(positioned, selected)?;
        records.extend(trailing_controls);
        self.records = records;
        Ok(selected_indexes)
    }
}

pub(crate) fn shift_selected_one_step<T>(
    values: &mut [T],
    selected: impl Fn(&T) -> bool,
    increase: bool,
    can_swap: impl Fn(&T, &T) -> bool,
) {
    if increase {
        for index in (0..values.len().saturating_sub(1)).rev() {
            if selected(&values[index])
                && !selected(&values[index + 1])
                && can_swap(&values[index], &values[index + 1])
            {
                values.swap(index, index + 1);
            }
        }
    } else {
        for index in 1..values.len() {
            if selected(&values[index])
                && !selected(&values[index - 1])
                && can_swap(&values[index - 1], &values[index])
            {
                values.swap(index - 1, index);
            }
        }
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
            screen = jump.resolved_screen();
            continue;
        }
        if !record.is_positioned_object() {
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
    let (records, mut selected_indexes) =
        encode_positioned_object_group(positioned, std::slice::from_ref(&selected))?;
    let selected_index = selected_indexes
        .pop()
        .ok_or(ObjectRelocationError::NotOrdinaryObject(selected))?;
    Ok((records, selected_index))
}

fn encode_positioned_object_group(
    positioned: Vec<PositionedObject>,
    selected: &[usize],
) -> Result<(Vec<ObjectRecord>, Vec<usize>), ObjectRelocationError> {
    let mut screen = 0_u16;
    let mut output = Vec::with_capacity(positioned.len());
    let mut selected_indexes = vec![None; selected.len()];
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
        if let Some(slot) = selected
            .iter()
            .position(|selected| *selected == object.original_index)
        {
            selected_indexes[slot] = Some(output.len());
        }
        output.push(object.record);
    }
    let selected_indexes = selected_indexes
        .into_iter()
        .zip(selected)
        .map(|(index, selected)| index.ok_or(ObjectRelocationError::NotOrdinaryObject(*selected)))
        .collect::<Result<_, _>>()?;
    Ok((output, selected_indexes))
}

fn canonical_screen_jump(screen: u16) -> Result<ObjectRecord, ObjectRelocationError> {
    if screen > 31 {
        return Err(ObjectRelocationError::TargetScreenOutOfRange(screen));
    }
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
    fn group_duplication_preserves_sources_relative_delta_and_selection_order() {
        let trailing = ObjectRecord::new(vec![7, 5, 0, 0xcb]).unwrap();
        let mut stream = ObjectStream {
            records: vec![
                object(false, 1, 2, 0x10),
                object(true, 3, 4, 0x20),
                trailing.clone(),
            ],
        };
        let selected = stream
            .duplicate_ordinary_object_group(&[1, 0], 17, 14)
            .unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(stream.records.last(), Some(&trailing));
        let placements = stream.native_placements();
        assert!(placements.iter().any(|placement| {
            placement.major == 2
                && placement.minor == 1
                && stream.records[placement.record_index].parameter() == 0x10
        }));
        assert!(placements.iter().any(|placement| {
            placement.major == 20
                && placement.minor == 3
                && stream.records[placement.record_index].parameter() == 0x20
        }));
        let first = placements
            .iter()
            .find(|placement| placement.record_index == selected[0])
            .unwrap();
        assert_eq!((first.major, first.minor), (37, 17));
        assert_eq!(stream.records[first.record_index].parameter(), 0x20);
        let second = placements
            .iter()
            .find(|placement| placement.record_index == selected[1])
            .unwrap();
        assert_eq!((second.major, second.minor), (19, 15));
        assert_eq!(stream.records[second.record_index].parameter(), 0x10);
    }

    #[test]
    fn group_duplication_preserves_every_preexisting_transition_byte() {
        let source = object(false, 1, 2, 0x10);
        let jump = ObjectRecord::new(vec![10, 0, 1]).unwrap();
        let later = object(false, 3, 4, 0x20);
        let trailing = ObjectRecord::new(vec![7, 5, 0, 0xcb]).unwrap();
        let mut stream = ObjectStream {
            records: vec![
                source.clone(),
                jump.clone(),
                later.clone(),
                trailing.clone(),
            ],
        };
        let selected = stream.duplicate_ordinary_object_group(&[0], 1, 0).unwrap();
        assert_eq!(selected, vec![1]);
        assert_eq!(stream.records[0], source);
        assert_eq!(stream.records[2], jump);
        assert_eq!(stream.records[3], later);
        assert_eq!(stream.records[4], trailing);
        assert_eq!(stream.records[1].coordinate_nibbles().second, 3);
        assert!(!stream.records[1].advances_screen());
    }

    #[test]
    fn invalid_group_duplication_is_failure_atomic() {
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), object(true, 3, 4, 0x20)],
        };
        let original = stream.clone();
        assert_eq!(
            stream.duplicate_ordinary_object_group(&[], 1, 1),
            Err(ObjectRelocationError::EmptySelection)
        );
        assert_eq!(stream, original);
        assert_eq!(
            stream.duplicate_ordinary_object_group(&[0, 0], 1, 1),
            Err(ObjectRelocationError::DuplicateSelection(0))
        );
        assert_eq!(stream, original);
        assert_eq!(
            stream.duplicate_ordinary_object_group(&[0, 1], 500, 0),
            Err(ObjectRelocationError::TargetPositionOutOfRange {
                index: 1,
                major: 520,
                minor: 3,
            })
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn group_relocation_moves_every_member_once_and_tracks_reordered_indexes() {
        let trailing = ObjectRecord::new(vec![7, 5, 0, 0xcb]).unwrap();
        let mut stream = ObjectStream {
            records: vec![
                object(false, 1, 2, 0x10),
                object(true, 3, 4, 0x20),
                trailing.clone(),
            ],
        };
        let selected = stream
            .relocate_ordinary_object_group(&[1, 0], 17, 14)
            .unwrap();
        assert_eq!(stream.native_placements().len(), 2);
        assert_eq!(stream.records.last(), Some(&trailing));
        let placements = stream.native_placements();
        let first = placements
            .iter()
            .find(|placement| placement.record_index == selected[0])
            .unwrap();
        assert_eq!((first.major, first.minor), (37, 17));
        assert_eq!(stream.records[first.record_index].parameter(), 0x20);
        let second = placements
            .iter()
            .find(|placement| placement.record_index == selected[1])
            .unwrap();
        assert_eq!((second.major, second.minor), (19, 15));
        assert_eq!(stream.records[second.record_index].parameter(), 0x10);
    }

    #[test]
    fn invalid_group_relocation_is_failure_atomic() {
        let mut stream = ObjectStream {
            records: vec![object(false, 1, 2, 0x10), object(true, 3, 4, 0x20)],
        };
        let original = stream.clone();
        assert_eq!(
            stream.relocate_ordinary_object_group(&[0, 1], -3, 0),
            Err(ObjectRelocationError::TargetPositionOutOfRange {
                index: 0,
                major: -1,
                minor: 1,
            })
        );
        assert_eq!(stream, original);
        assert_eq!(
            stream.relocate_ordinary_object_group(&[1, 1], 0, 0),
            Err(ObjectRelocationError::DuplicateSelection(1))
        );
        assert_eq!(stream, original);
    }

    #[test]
    fn z_order_step_preserves_positions_across_forward_and_backtracking_screen_jumps() {
        let mut stream = ObjectStream {
            records: vec![
                object(false, 1, 2, 0x10),
                ObjectRecord::new(vec![2, 0, 1]).unwrap(),
                object(false, 3, 4, 0x20),
                object(false, 5, 6, 0x30),
            ],
        };
        let before = stream
            .native_placements()
            .into_iter()
            .map(|placement| {
                (
                    stream.records[placement.record_index].parameter(),
                    placement.screen,
                    placement.major,
                    placement.minor,
                )
            })
            .collect::<Vec<_>>();
        let moved = stream.adjust_ordinary_object_z_order(&[0], true).unwrap();
        assert_eq!(moved, vec![3]);
        let after = stream
            .native_placements()
            .into_iter()
            .map(|placement| {
                (
                    stream.records[placement.record_index].parameter(),
                    placement.screen,
                    placement.major,
                    placement.minor,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(after[0].0, 0x20);
        assert_eq!(after[1].0, 0x10);
        for original in before {
            assert!(after.contains(&original));
        }
        assert!(stream.records.iter().any(|record| {
            record
                .screen_jump()
                .is_some_and(|jump| jump.resolved_screen() == 0)
        }));
    }

    #[test]
    fn z_order_multi_selection_moves_stably_by_one_unselected_record() {
        let mut stream = ObjectStream {
            records: vec![
                object(false, 1, 1, 0x10),
                object(false, 2, 2, 0x20),
                object(false, 3, 3, 0x30),
                object(false, 4, 4, 0x40),
            ],
        };
        let moved = stream
            .adjust_ordinary_object_z_order(&[0, 2], true)
            .unwrap();
        assert_eq!(moved, vec![1, 3]);
        assert_eq!(
            stream
                .records
                .iter()
                .map(ObjectRecord::parameter)
                .collect::<Vec<_>>(),
            [0x20, 0x10, 0x40, 0x30]
        );
        let moved = stream
            .adjust_ordinary_object_z_order(&moved, false)
            .unwrap();
        assert_eq!(moved, vec![0, 2]);
        assert_eq!(
            stream
                .records
                .iter()
                .map(ObjectRecord::parameter)
                .collect::<Vec<_>>(),
            [0x10, 0x20, 0x30, 0x40]
        );
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
    fn semantic_relocation_removes_a_redundant_leading_screen_zero_jump() {
        let mut stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0, 0, 1]).unwrap(),
                object(false, 3, 4, 0x20),
            ],
        };
        let selected = stream
            .relocate_ordinary_object(
                1,
                0,
                ObjectCoordinateNibbles {
                    first: 2,
                    second: 4,
                },
            )
            .unwrap();

        assert_eq!(selected, 0);
        assert_eq!(stream.records.len(), 1);
        assert!(stream.records[0].screen_jump().is_none());
        assert_eq!(stream.records[0].coordinate_nibbles().first, 2);
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
    fn positioned_command_zero_extended_objects_insert_and_relocate() {
        let extended = ObjectRecord::new(vec![0, 0, 4]).unwrap();
        let mut stream = ObjectStream::default();
        let inserted = stream
            .insert_ordinary_object_at_position(
                extended,
                3,
                ObjectCoordinateNibbles {
                    first: 5,
                    second: 6,
                },
                true,
            )
            .unwrap();
        assert_eq!(stream.records[inserted].command_id(), 0);
        assert_eq!(stream.records[inserted].parameter(), 4);
        assert_eq!(stream.native_placements()[0].screen, 3);
        assert_eq!(stream.native_placements()[0].minor, 0x15);

        let relocated = stream
            .relocate_ordinary_object_position(
                inserted,
                5,
                ObjectCoordinateNibbles {
                    first: 7,
                    second: 8,
                },
                false,
            )
            .unwrap();
        assert_eq!(stream.records[relocated].command_id(), 0);
        assert_eq!(stream.records[relocated].parameter(), 4);
        assert_eq!(stream.native_placements()[0].screen, 5);
        assert_eq!(stream.native_placements()[0].minor, 7);
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

        let mut out_of_range_jump = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x1f, 1, 1]).unwrap(),
                object(false, 1, 2, 0x10),
                object(false, 3, 4, 0x10),
            ],
        };
        let out_of_range_original = out_of_range_jump.clone();
        assert_eq!(
            out_of_range_jump.relocate_ordinary_object(
                1,
                0,
                ObjectCoordinateNibbles {
                    first: 1,
                    second: 2,
                },
            ),
            Err(ObjectRelocationError::TargetScreenOutOfRange(32))
        );
        assert_eq!(out_of_range_jump, out_of_range_original);
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
