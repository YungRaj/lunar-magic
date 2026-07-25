use crate::{NativeSpriteStream, SpriteToken};

/// One decoded native sprite position, independent of horizontal/vertical level presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSpritePlacement {
    pub token_index: usize,
    /// First native record byte retained for renderer variants not covered by decoded fields.
    pub first_byte: u8,
    pub screen: u16,
    pub major: u16,
    pub minor: u16,
    pub sprite_number: u8,
    pub extra_bits: u8,
}

impl NativeSpriteStream {
    /// Resolves screen-control tokens and native three-byte sprite coordinate fields.
    ///
    /// Extended records retain the first three native bytes and may append command-specific data.
    /// Other control tokens emit no placement.
    #[must_use]
    pub fn native_placements(&self) -> Vec<NativeSpritePlacement> {
        let mut y_high = 0_u16;
        let mut placements = Vec::new();
        for (token_index, token) in self.tokens.iter().enumerate() {
            match token {
                SpriteToken::Screen(value) => y_high = u16::from(*value) << 5,
                SpriteToken::Record(record) if record.encoded.len() >= 3 => {
                    let first = record.encoded[0];
                    let second = record.encoded[1];
                    let screen = u16::from(second & 0x0f) | (u16::from(first & 0x02 != 0) << 4);
                    placements.push(NativeSpritePlacement {
                        token_index,
                        first_byte: first,
                        screen,
                        major: screen
                            .saturating_mul(16)
                            .saturating_add(u16::from(second >> 4)),
                        minor: y_high | u16::from(first >> 4) | (u16::from(first & 1) << 4),
                        sprite_number: record.encoded[2],
                        extra_bits: (first >> 2) & 0x03,
                    });
                }
                SpriteToken::Control(_) | SpriteToken::Record(_) => {}
            }
        }
        placements
    }
}

impl NativeSpritePlacement {
    /// Converts the native major/minor axes to tile X/Y.
    #[must_use]
    pub const fn tile_coordinates(self, vertical: bool) -> (u16, u16) {
        if vertical {
            (self.minor, self.major)
        } else {
            (self.major, self.minor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NativeSpriteStream, SpriteRecord};

    #[test]
    fn legacy_screen_bit_coordinates_and_metadata_are_decoded() {
        let stream = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x0b, 0x07, 0x42],
                }),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x16, 0x0d, 0x7f],
                }),
            ],
        };
        assert_eq!(
            stream.native_placements(),
            [
                NativeSpritePlacement {
                    token_index: 0,
                    first_byte: 0x0b,
                    screen: 23,
                    major: 368,
                    minor: 16,
                    sprite_number: 0x42,
                    extra_bits: 2,
                },
                NativeSpritePlacement {
                    token_index: 1,
                    first_byte: 0x16,
                    screen: 29,
                    major: 464,
                    minor: 1,
                    sprite_number: 0x7f,
                    extra_bits: 1,
                },
            ]
        );
    }

    #[test]
    fn expanded_position_controls_supply_upper_y_bits_without_changing_screen() {
        let stream = NativeSpriteStream {
            header: 0,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(5),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x01, 0x02, 3],
                }),
            ],
        };
        assert_eq!(stream.native_placements()[0].major, 32);
        assert_eq!(stream.native_placements()[0].minor, 176);
        assert_eq!(stream.native_placements()[0].screen, 2);
    }
}
