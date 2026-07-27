use crate::ObjectStream;

/// Orientation-neutral position recovered while walking the serialized object stream.
///
/// `major` is the coordinate along the level's screen axis and `minor` is the coordinate within
/// the perpendicular 32-value encoded span (normally 27 visible tiles). A caller with level-mode
/// knowledge may map these to X/Y.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeObjectPlacement {
    pub record_index: usize,
    pub screen: u16,
    pub major: u16,
    pub minor: u8,
    pub major_span: u8,
    pub minor_span: u8,
}

impl ObjectStream {
    /// Resolves ordinary object records for a horizontal level.
    ///
    /// This is the common SMW layout and is equivalent to
    /// [`Self::native_placements_for_orientation`] with `vertical` set to `false`.
    #[must_use]
    pub fn native_placements(&self) -> Vec<NativeObjectPlacement> {
        self.native_placements_for_orientation(false)
    }

    /// Resolves ordinary object records onto the selected native sequential screen axis.
    ///
    /// The high bit on a record advances one screen before placing that record. Internal screen
    /// jump records reset the running screen and are not returned as visible objects. Byte zero
    /// stores the perpendicular coordinate and byte one stores the coordinate along the screen
    /// axis; orientation maps those abstract axes to X/Y after placement.
    #[must_use]
    pub fn native_placements_for_orientation(&self, _vertical: bool) -> Vec<NativeObjectPlacement> {
        let mut screen = 0_u16;
        let mut placements = Vec::with_capacity(self.records.len());
        for (record_index, record) in self.records.iter().enumerate() {
            if let Some(jump) = record.screen_jump() {
                screen = jump.packed_target;
                continue;
            }
            if record.command_id() == 0 && record.parameter() <= 3 {
                continue;
            }
            if record.advances_screen() {
                screen = screen.saturating_add(1);
            }
            let coordinates = record.coordinate_nibbles();
            let parameter = record.parameter();
            // Byte zero always stores the perpendicular coordinate and byte one
            // stores the coordinate along the screen axis. Orientation changes
            // how those abstract axes map to X/Y, not their serialized order.
            let (major_nibble, minor_nibble) = (coordinates.second, coordinates.first);
            let minor = minor_nibble
                | if record.perpendicular_high_coordinate() {
                    0x10
                } else {
                    0
                };
            placements.push(NativeObjectPlacement {
                record_index,
                screen,
                major: screen
                    .saturating_mul(16)
                    .saturating_add(u16::from(major_nibble)),
                minor,
                major_span: (parameter >> 4).saturating_add(1),
                minor_span: (parameter & 0x0f).saturating_add(1),
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
                    major: 3,
                    minor: 2,
                    major_span: 1,
                    minor_span: 1,
                },
                NativeObjectPlacement {
                    record_index: 1,
                    screen: 1,
                    major: 21,
                    minor: 4,
                    major_span: 1,
                    minor_span: 1,
                },
                NativeObjectPlacement {
                    record_index: 2,
                    screen: 1,
                    major: 23,
                    minor: 6,
                    major_span: 1,
                    minor_span: 1,
                },
            ]
        );
    }

    #[test]
    fn byte_zero_bit_four_extends_the_perpendicular_coordinate() {
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x15, 0x17, 0]).unwrap()],
        };
        let horizontal = stream.native_placements_for_orientation(false)[0];
        assert_eq!((horizontal.major, horizontal.minor), (7, 0x15));

        let vertical = stream.native_placements_for_orientation(true)[0];
        assert_eq!((vertical.major, vertical.minor), (7, 0x15));
        assert_eq!(vertical.tile_coordinates(true), (0x15, 7));
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
                    major: 2,
                    minor: 1,
                    major_span: 1,
                    minor_span: 1,
                },
                NativeObjectPlacement {
                    record_index: 2,
                    screen: 3,
                    major: 53,
                    minor: 4,
                    major_span: 1,
                    minor_span: 1,
                },
            ]
        );
    }

    #[test]
    fn direct_map16_internal_controls_are_not_visible_objects() {
        let stream = ObjectStream {
            records: vec![
                ObjectRecord::new(vec![0, 0, 0, 0]).unwrap(),
                ObjectRecord::new(vec![0, 0, 2, 0, 0]).unwrap(),
                ObjectRecord::new(vec![2, 0, 0x10]).unwrap(),
            ],
        };
        assert_eq!(stream.native_placements().len(), 1);
        assert_eq!(stream.native_placements()[0].record_index, 2);
    }

    #[test]
    fn orientation_mapping_is_explicit() {
        let placement = NativeObjectPlacement {
            record_index: 0,
            screen: 2,
            major: 35,
            minor: 7,
            major_span: 1,
            minor_span: 1,
        };
        assert_eq!(placement.tile_coordinates(false), (35, 7));
        assert_eq!(placement.tile_coordinates(true), (7, 35));
    }

    #[test]
    fn vertical_levels_apply_the_screen_axis_to_encoded_y() {
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x82, 0x13, 0]).unwrap()],
        };
        assert_eq!(
            stream.native_placements_for_orientation(true),
            [NativeObjectPlacement {
                record_index: 0,
                screen: 1,
                major: 19,
                minor: 2,
                major_span: 1,
                minor_span: 1,
            }]
        );
    }

    #[test]
    fn parameter_nibbles_recover_native_object_footprints() {
        let stream = ObjectStream {
            records: vec![ObjectRecord::new(vec![0x02, 0x13, 0x42]).unwrap()],
        };
        let placement = stream.native_placements()[0];
        assert_eq!(placement.major_span, 5);
        assert_eq!(placement.minor_span, 3);
    }
}
