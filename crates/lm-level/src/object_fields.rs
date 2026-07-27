use crate::{ObjectRecord, encoded_record_length};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectFieldError {
    InvalidCommandId(u8),
    InvalidCoordinateNibble { first: u8, second: u8 },
    NotScreenJump,
    InvalidScreenJumpTarget(u16),
    NotScreenExit,
    InvalidScreenExitScreen(u8),
    TerminatorCollision,
    UnknownEncodedLength,
    EncodedLengthMismatch { expected: usize, actual: usize },
    NotExtendedCommand27,
    InvalidExtendedSize { horizontal: u8, vertical: u8 },
}

impl std::fmt::Display for ObjectFieldError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid level-object field edit: {self:?}")
    }
}

impl std::error::Error for ObjectFieldError {}

impl ObjectRecord {
    /// Returns the six-bit command ID recovered from the native object decoder.
    #[must_use]
    pub fn command_id(&self) -> u8 {
        (self.encoded[1] >> 4) | ((self.encoded[0] & 0x60) >> 1)
    }

    /// Returns the third encoded byte, whose interpretation is command-specific.
    #[must_use]
    pub fn parameter(&self) -> u8 {
        self.encoded[2]
    }

    /// Returns Lunar Magic's two seven-bit size components for the extended command `$27` form.
    ///
    /// The recovered editor recognizes command `$27` with byte 3's mode bits set to `$C0`.
    /// Horizontal size minus one occupies byte 2's low seven bits and vertical size minus one
    /// occupies extension byte 6. Byte 2's high bit controls the optional eighth record byte and
    /// is deliberately excluded from the size.
    #[must_use]
    pub fn extended_command27_tile_size(&self) -> Option<(u8, u8)> {
        (self.command_id() == 0x27
            && self
                .encoded
                .get(3)
                .is_some_and(|flags| flags & 0xc0 == 0xc0)
            && self.encoded.len() >= 7)
            .then(|| ((self.encoded[2] & 0x7f) + 1, (self.encoded[6] & 0x7f) + 1))
    }

    /// Changes only Lunar Magic's recovered extended command `$27` size fields.
    ///
    /// Both axes accept 1–128 tiles. The optional-record flag in byte 2 and every unrelated
    /// extension byte remain byte-exact.
    ///
    /// # Errors
    ///
    /// Rejects records outside command `$27` mode `$C0`, sizes outside 1–128, and any candidate
    /// whose native encoded length would no longer match the retained record.
    pub fn set_extended_command27_tile_size(
        &mut self,
        horizontal: u8,
        vertical: u8,
    ) -> Result<(), ObjectFieldError> {
        if self.extended_command27_tile_size().is_none() {
            return Err(ObjectFieldError::NotExtendedCommand27);
        }
        if !(1..=128).contains(&horizontal) || !(1..=128).contains(&vertical) {
            return Err(ObjectFieldError::InvalidExtendedSize {
                horizontal,
                vertical,
            });
        }
        let mut candidate = self.encoded.clone();
        candidate[2] = (candidate[2] & 0x80) | (horizontal - 1);
        candidate[6] = vertical - 1;
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Returns the encoded coordinates before level-orientation interpretation.
    ///
    /// Lunar Magic swaps these nibbles when decoding the alternate level orientation. Keeping the
    /// pair orientation-neutral prevents a standalone record from claiming an absolute X/Y axis.
    #[must_use]
    pub fn coordinate_nibbles(&self) -> ObjectCoordinateNibbles {
        ObjectCoordinateNibbles {
            first: self.encoded[0] & 0x0f,
            second: self.encoded[1] & 0x0f,
        }
    }

    /// Returns whether the object occupies the lower half of a horizontal screen or the right
    /// half of a vertical screen.
    #[must_use]
    pub fn perpendicular_high_coordinate(&self) -> bool {
        self.encoded[0] & 0x10 != 0
    }

    /// Changes only the perpendicular high-coordinate bit.
    ///
    /// This is Y bit 4 in horizontal levels and X bit 4 in vertical levels.
    ///
    /// # Errors
    ///
    /// Rejects the change if the resulting bytes would collide with a control encoding.
    pub fn set_perpendicular_high_coordinate(
        &mut self,
        high: bool,
    ) -> Result<(), ObjectFieldError> {
        let mut candidate = self.encoded.clone();
        candidate[0] = (candidate[0] & !0x10) | (u8::from(high) << 4);
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Returns the encoded new-screen/advance-screen bit recovered from stream normalization.
    #[must_use]
    pub fn advances_screen(&self) -> bool {
        self.encoded[0] & 0x80 != 0
    }

    /// Decodes an editor-maintained screen-jump control record.
    ///
    /// Commands with ID zero and parameter `1` or `3` are the two packed encodings used by Lunar
    /// Magic's screen-transition normalizer. The returned target is deliberately called packed:
    /// interpreting it as an absolute axis coordinate requires the complete level layout.
    #[must_use]
    pub fn screen_jump(&self) -> Option<ObjectScreenJump> {
        if self.command_id() != 0 {
            return None;
        }
        let first = u16::from(self.encoded[0] & 0x1f);
        let second = u16::from(self.encoded[1] & 0x0f);
        let (encoding, packed_target) = match self.parameter() {
            1 => (ScreenJumpEncoding::FirstLow, (second << 8) | first),
            3 => (ScreenJumpEncoding::FirstHigh, (first << 8) | second),
            _ => return None,
        };
        Some(ObjectScreenJump {
            encoding,
            packed_target,
        })
    }

    /// Decodes a native Layer 1 screen-exit object.
    ///
    /// Lunar Magic uses command zero with parameter `0` for the compact four-byte form and
    /// parameter `2` for the five-byte form. The screen index occupies byte 0's low five bits.
    /// The compact form stores the destination high nibble in byte 1; the extended form stores
    /// the complete high byte in its second extension byte.
    #[must_use]
    pub fn screen_exit(&self) -> Option<ObjectScreenExit> {
        if self.command_id() != 0 {
            return None;
        }
        let (encoding, destination_high) = match self.parameter() {
            0 if self.encoded.len() == 4 => {
                (ScreenExitObjectEncoding::Compact, self.encoded[1] & 0x0f)
            }
            2 if self.encoded.len() == 5 => (ScreenExitObjectEncoding::Extended, self.encoded[4]),
            _ => return None,
        };
        Some(ObjectScreenExit {
            encoding,
            screen: self.encoded[0] & 0x1f,
            destination_and_flags: u16::from_le_bytes([self.encoded[3], destination_high]),
        })
    }

    /// Changes only the distributed six-bit command field.
    ///
    /// # Errors
    ///
    /// Rejects IDs above 0x3f and commands whose recovered encoded size differs from this
    /// lossless record. Callers changing record shape must construct a complete replacement.
    pub fn set_command_id(&mut self, command_id: u8) -> Result<(), ObjectFieldError> {
        if command_id > 0x3f {
            return Err(ObjectFieldError::InvalidCommandId(command_id));
        }
        let mut candidate = self.encoded.clone();
        candidate[0] = (candidate[0] & !0x60) | ((command_id & 0x30) << 1);
        candidate[1] = (candidate[1] & 0x0f) | ((command_id & 0x0f) << 4);
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Changes only the command-specific third byte.
    ///
    /// # Errors
    ///
    /// Rejects values that would change the recovered record size; such changes require an
    /// explicit complete-record replacement so extension bytes cannot be fabricated or dropped.
    pub fn set_parameter(&mut self, parameter: u8) -> Result<(), ObjectFieldError> {
        let mut candidate = self.encoded.clone();
        candidate[2] = parameter;
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Changes only the two encoded coordinate nibbles.
    ///
    /// # Errors
    ///
    /// Rejects either value above `0x0f`.
    pub fn set_coordinate_nibbles(
        &mut self,
        coordinates: ObjectCoordinateNibbles,
    ) -> Result<(), ObjectFieldError> {
        if coordinates.first > 0x0f || coordinates.second > 0x0f {
            return Err(ObjectFieldError::InvalidCoordinateNibble {
                first: coordinates.first,
                second: coordinates.second,
            });
        }
        let mut candidate = self.encoded.clone();
        candidate[0] = (candidate[0] & 0xf0) | coordinates.first;
        candidate[1] = (candidate[1] & 0xf0) | coordinates.second;
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Changes only the encoded screen-advance bit.
    ///
    /// # Errors
    ///
    /// Rejects the change if it would make the first encoded byte collide with the stream
    /// terminator.
    pub fn set_advances_screen(&mut self, advances: bool) -> Result<(), ObjectFieldError> {
        let mut candidate = self.encoded.clone();
        candidate[0] = (candidate[0] & !0x80) | if advances { 0x80 } else { 0 };
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Changes the packed target of an existing screen-jump control while retaining its encoding.
    ///
    /// # Errors
    ///
    /// Rejects ordinary object records and targets containing bits outside the selected recovered
    /// packed representation (`0x0f1f` for first-low or `0x1f0f` for first-high).
    pub fn set_screen_jump_target(&mut self, packed_target: u16) -> Result<(), ObjectFieldError> {
        let jump = self.screen_jump().ok_or(ObjectFieldError::NotScreenJump)?;
        let mask = match jump.encoding {
            ScreenJumpEncoding::FirstLow => 0x0f1f,
            ScreenJumpEncoding::FirstHigh => 0x1f0f,
        };
        if packed_target & !mask != 0 {
            return Err(ObjectFieldError::InvalidScreenJumpTarget(packed_target));
        }
        let mut candidate = self.encoded.clone();
        match jump.encoding {
            ScreenJumpEncoding::FirstLow => {
                candidate[0] = (candidate[0] & !0x1f) | packed_target.to_le_bytes()[0] & 0x1f;
                candidate[1] = (candidate[1] & 0xf0) | (packed_target >> 8).to_le_bytes()[0] & 0x0f;
            }
            ScreenJumpEncoding::FirstHigh => {
                candidate[0] =
                    (candidate[0] & !0x1f) | (packed_target >> 8).to_le_bytes()[0] & 0x1f;
                candidate[1] = (candidate[1] & 0xf0) | packed_target.to_le_bytes()[0] & 0x0f;
            }
        }
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }

    /// Rewrites an existing screen-exit object using Lunar Magic's canonical compact/extended
    /// selection while preserving the unrelated new-screen bit in byte 0.
    ///
    /// Destinations whose high byte fits in four bits use parameter `0`; all others use parameter
    /// `2` and the five-byte representation. This deliberately permits the record shape to change,
    /// matching Lunar Magic's recovered `SetScreenExitObjectForScreen` routine.
    ///
    /// # Errors
    ///
    /// Rejects ordinary objects and screen indices above 31 without changing the record.
    pub fn set_screen_exit(
        &mut self,
        screen: u8,
        destination_and_flags: u16,
    ) -> Result<(), ObjectFieldError> {
        if self.screen_exit().is_none() {
            return Err(ObjectFieldError::NotScreenExit);
        }
        if screen > 0x1f {
            return Err(ObjectFieldError::InvalidScreenExitScreen(screen));
        }
        let [low, high] = destination_and_flags.to_le_bytes();
        let advance = self.encoded[0] & 0x80;
        let candidate = if destination_and_flags & 0xf000 == 0 {
            vec![advance | screen, high & 0x0f, 0, low]
        } else {
            vec![advance | screen, 0, 2, low, high]
        };
        validate_candidate(&candidate)?;
        self.encoded = candidate;
        Ok(())
    }
}

/// The two raw coordinate nibbles whose X/Y interpretation depends on level orientation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectCoordinateNibbles {
    pub first: u8,
    pub second: u8,
}

/// Which half of the packed target is stored in the first command byte.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenJumpEncoding {
    FirstLow,
    FirstHigh,
}

/// An exact screen-jump control interpretation recovered from list normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectScreenJump {
    pub encoding: ScreenJumpEncoding,
    pub packed_target: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenExitObjectEncoding {
    Compact,
    Extended,
}

/// Exact native screen-exit fields recovered from Lunar Magic's Layer 1 object synchronizer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectScreenExit {
    pub encoding: ScreenExitObjectEncoding,
    pub screen: u8,
    pub destination_and_flags: u16,
}

fn validate_candidate(encoded: &[u8]) -> Result<(), ObjectFieldError> {
    if encoded.first() == Some(&0xff) {
        return Err(ObjectFieldError::TerminatorCollision);
    }
    let expected = encoded_record_length(encoded).ok_or(ObjectFieldError::UnknownEncodedLength)?;
    if expected != encoded.len() {
        return Err(ObjectFieldError::EncodedLengthMismatch {
            expected,
            actual: encoded.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_decode_and_edit_preserve_every_unowned_bit() {
        let mut record = ObjectRecord::new(vec![0x8f, 0x0a, 1]).unwrap();
        let original_first_unowned = record.encoded()[0] & !0x60;
        let original_second_unowned = record.encoded()[1] & 0x0f;
        record.set_command_id(0x3f).unwrap();
        assert_eq!(record.command_id(), 0x3f);
        assert_eq!(record.encoded()[0] & !0x60, original_first_unowned);
        assert_eq!(record.encoded()[1] & 0x0f, original_second_unowned);
        assert_eq!(record.parameter(), 1);
    }

    #[test]
    fn invalid_or_shape_changing_edits_are_atomic() {
        let mut record = ObjectRecord::new(vec![0, 0, 1]).unwrap();
        let original = record.clone();
        assert_eq!(
            record.set_command_id(0x40),
            Err(ObjectFieldError::InvalidCommandId(0x40))
        );
        assert_eq!(record, original);
        assert_eq!(
            record.set_command_id(0x22),
            Err(ObjectFieldError::EncodedLengthMismatch {
                expected: 4,
                actual: 3
            })
        );
        assert_eq!(record, original);
        assert_eq!(
            record.set_parameter(0),
            Err(ObjectFieldError::EncodedLengthMismatch {
                expected: 4,
                actual: 3
            })
        );
        assert_eq!(record, original);
    }

    #[test]
    fn screen_exit_forms_decode_and_canonically_change_shape() {
        let mut compact = ObjectRecord::new(vec![0x85, 0x0a, 0, 0x34]).unwrap();
        assert_eq!(
            compact.screen_exit(),
            Some(ObjectScreenExit {
                encoding: ScreenExitObjectEncoding::Compact,
                screen: 5,
                destination_and_flags: 0x0a34,
            })
        );
        compact.set_screen_exit(0x1f, 0xbcde).unwrap();
        assert_eq!(compact.encoded(), &[0x9f, 0, 2, 0xde, 0xbc]);
        assert_eq!(
            compact.screen_exit(),
            Some(ObjectScreenExit {
                encoding: ScreenExitObjectEncoding::Extended,
                screen: 0x1f,
                destination_and_flags: 0xbcde,
            })
        );
        compact.set_screen_exit(2, 0x0123).unwrap();
        assert_eq!(compact.encoded(), &[0x82, 1, 0, 0x23]);
    }

    #[test]
    fn screen_exit_edit_rejects_wrong_record_and_screen_atomically() {
        let mut ordinary = ObjectRecord::new(vec![1, 0x10, 2]).unwrap();
        let original = ordinary.clone();
        assert_eq!(
            ordinary.set_screen_exit(0, 0x1234),
            Err(ObjectFieldError::NotScreenExit)
        );
        assert_eq!(ordinary, original);

        let mut exit = ObjectRecord::new(vec![0, 0, 0, 0]).unwrap();
        let original = exit.clone();
        assert_eq!(
            exit.set_screen_exit(0x20, 0x1234),
            Err(ObjectFieldError::InvalidScreenExitScreen(0x20))
        );
        assert_eq!(exit, original);
    }

    #[test]
    fn same_shape_parameter_and_extended_command_edits_work() {
        let mut ordinary = ObjectRecord::new(vec![0, 0, 1]).unwrap();
        ordinary.set_parameter(0x7f).unwrap();
        assert_eq!(ordinary.parameter(), 0x7f);

        let mut extended = ObjectRecord::new(vec![0, 0, 1, 0xaa]).unwrap();
        extended.set_command_id(0x22).unwrap();
        assert_eq!(extended.command_id(), 0x22);
        assert_eq!(extended.encoded()[3], 0xaa);
    }

    #[test]
    fn extended_command27_size_edits_preserve_every_unowned_byte() {
        let mut record =
            ObjectRecord::new(vec![0x40, 0x70, 0x84, 0xc3, 0xaa, 0xbb, 0x06, 0xdd]).unwrap();
        assert_eq!(record.command_id(), 0x27);
        assert_eq!(record.extended_command27_tile_size(), Some((5, 7)));
        record.set_extended_command27_tile_size(128, 64).unwrap();
        assert_eq!(
            record.encoded(),
            &[0x40, 0x70, 0xff, 0xc3, 0xaa, 0xbb, 0x3f, 0xdd]
        );
        assert_eq!(record.extended_command27_tile_size(), Some((128, 64)));
    }

    #[test]
    fn extended_command27_size_rejects_wrong_shape_and_bounds_atomically() {
        let mut ordinary = ObjectRecord::new(vec![0x40, 0x70, 0x04, 0x80, 0xaa, 0xbb]).unwrap();
        let original = ordinary.clone();
        assert_eq!(
            ordinary.set_extended_command27_tile_size(2, 3),
            Err(ObjectFieldError::NotExtendedCommand27)
        );
        assert_eq!(ordinary, original);

        let mut extended =
            ObjectRecord::new(vec![0x40, 0x70, 0x04, 0xc0, 0xaa, 0xbb, 0x06]).unwrap();
        let original = extended.clone();
        assert!(matches!(
            extended.set_extended_command27_tile_size(0, 3),
            Err(ObjectFieldError::InvalidExtendedSize { .. })
        ));
        assert_eq!(extended, original);
    }

    #[test]
    fn coordinate_and_screen_fields_preserve_every_unowned_bit() {
        let mut record = ObjectRecord::new(vec![0x21, 0xa2, 1]).unwrap();
        record
            .set_coordinate_nibbles(ObjectCoordinateNibbles {
                first: 0x0e,
                second: 0x0d,
            })
            .unwrap();
        record.set_advances_screen(true).unwrap();
        record.set_perpendicular_high_coordinate(true).unwrap();
        assert!(record.perpendicular_high_coordinate());
        assert_eq!(
            record.coordinate_nibbles(),
            ObjectCoordinateNibbles {
                first: 0x0e,
                second: 0x0d
            }
        );
        assert!(record.advances_screen());
        assert_eq!(record.encoded(), &[0xbe, 0xad, 1]);
    }

    #[test]
    fn invalid_coordinate_and_shape_changing_screen_edits_are_atomic() {
        let mut ordinary = ObjectRecord::new(vec![0, 0, 1]).unwrap();
        let original = ordinary.clone();
        assert_eq!(
            ordinary.set_coordinate_nibbles(ObjectCoordinateNibbles {
                first: 0x10,
                second: 0,
            }),
            Err(ObjectFieldError::InvalidCoordinateNibble {
                first: 0x10,
                second: 0
            })
        );
        assert_eq!(ordinary, original);

        let mut collision = ObjectRecord::new(vec![0x7f, 0, 1]).unwrap();
        let original = collision.clone();
        assert_eq!(
            collision.set_advances_screen(true),
            Err(ObjectFieldError::TerminatorCollision)
        );
        assert_eq!(collision, original);
    }

    #[test]
    fn both_screen_jump_encodings_decode_and_edit_exactly() {
        let mut first_low = ObjectRecord::new(vec![0x1a, 0x0b, 1]).unwrap();
        assert_eq!(
            first_low.screen_jump(),
            Some(ObjectScreenJump {
                encoding: ScreenJumpEncoding::FirstLow,
                packed_target: 0x0b1a,
            })
        );
        first_low.set_screen_jump_target(0x0c1d).unwrap();
        assert_eq!(first_low.encoded(), &[0x1d, 0x0c, 1]);

        let mut first_high = ObjectRecord::new(vec![0x0b, 0x0a, 3]).unwrap();
        assert_eq!(
            first_high.screen_jump(),
            Some(ObjectScreenJump {
                encoding: ScreenJumpEncoding::FirstHigh,
                packed_target: 0x0b0a,
            })
        );
        first_high.set_screen_jump_target(0x1c0d).unwrap();
        assert_eq!(first_high.encoded(), &[0x1c, 0x0d, 3]);
    }

    #[test]
    fn invalid_screen_jump_edits_are_atomic() {
        let mut ordinary = ObjectRecord::new(vec![0, 0, 2]).unwrap();
        let original = ordinary.clone();
        assert_eq!(
            ordinary.set_screen_jump_target(1),
            Err(ObjectFieldError::NotScreenJump)
        );
        assert_eq!(ordinary, original);

        let mut jump = ObjectRecord::new(vec![0, 0, 1]).unwrap();
        let original = jump.clone();
        assert_eq!(
            jump.set_screen_jump_target(0x1020),
            Err(ObjectFieldError::InvalidScreenJumpTarget(0x1020))
        );
        assert_eq!(jump, original);
    }
}
