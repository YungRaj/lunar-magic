use crate::ObjectStream;

/// Orientation-neutral position recovered while walking the serialized object stream.
///
/// `major` is the coordinate along the level's screen axis and `minor` is the coordinate within
/// the perpendicular 16-tile span. A caller with level-mode knowledge may map these to X/Y.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeObjectPlacement {
    pub record_index: usize,
    pub screen: u16,
    pub major: u16,
    pub minor: u8,
}

impl ObjectStream {
    /// Resolves ordinary object records onto the native sequential screen axis.
    ///
    /// The high bit on a record advances one screen before placing that record. Internal screen
    /// jump records reset the running screen and are not returned as visible objects.
    #[must_use]
    pub fn native_placements(&self) -> Vec<NativeObjectPlacement> {
        let mut screen = 0_u16;
        let mut placements = Vec::with_capacity(self.records.len());
        for (record_index, record) in self.records.iter().enumerate() {
            if let Some(jump) = record.screen_jump() {
                screen = jump.packed_target;
                continue;
            }
            if record.advances_screen() {
                screen = screen.saturating_add(1);
            }
            let coordinates = record.coordinate_nibbles();
            placements.push(NativeObjectPlacement {
                record_index,
                screen,
                major: screen
                    .saturating_mul(16)
                    .saturating_add(u16::from(coordinates.first)),
                minor: coordinates.second,
            });
        }
        placements
    }
}

impl NativeObjectPlacement {
    /// Converts the orientation-neutral axes to tile X/Y for the selected level orientation.
    #[must_use]
    pub const fn tile_coordinates(self, vertical: bool) -> (u16, u16) {
        if vertical {
            (self.minor as u16, self.major)
        } else {
            (self.major, self.minor as u16)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjectRecord;

    #[test]
    fn screen_advances_are_applied_before_object_coordinates() {
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x02, 0x13, 0]).unwrap(),
                ObjectRecord::new(vec![0x84, 0x15, 0]).unwrap(),
                ObjectRecord::new(vec![0x06, 0x17, 0]).unwrap(),
            ],
        };
        assert_eq!(
            stream.native_placements(),
            [
                NativeObjectPlacement {
                    record_index: 0,
                    screen: 0,
                    major: 2,
                    minor: 3,
                },
                NativeObjectPlacement {
                    record_index: 1,
                    screen: 1,
                    major: 20,
                    minor: 5,
                },
                NativeObjectPlacement {
                    record_index: 2,
                    screen: 1,
                    major: 22,
                    minor: 7,
                },
            ]
        );
    }

    #[test]
    fn screen_jump_controls_reset_the_axis_and_are_not_visible() {
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0x01, 0x12, 0]).unwrap(),
                ObjectRecord::new(vec![0x03, 0x00, 1]).unwrap(),
                ObjectRecord::new(vec![0x04, 0x15, 0]).unwrap(),
            ],
        };
        assert_eq!(
            stream.native_placements(),
            [
                NativeObjectPlacement {
                    record_index: 0,
                    screen: 0,
                    major: 1,
                    minor: 2,
                },
                NativeObjectPlacement {
                    record_index: 2,
                    screen: 3,
                    major: 52,
                    minor: 5,
                },
            ]
        );
    }

    #[test]
    fn orientation_mapping_is_explicit() {
        let placement = NativeObjectPlacement {
            record_index: 0,
            screen: 2,
            major: 35,
            minor: 7,
        };
        assert_eq!(placement.tile_coordinates(false), (35, 7));
        assert_eq!(placement.tile_coordinates(true), (7, 35));
    }
}
