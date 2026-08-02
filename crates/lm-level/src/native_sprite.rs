use crate::SpriteRecord;

mod length_table;
mod record_fields;
mod stream_codec;

pub use length_table::{SpriteLengthTable, SpriteLengthTableError};
pub use record_fields::{NativeSpriteFieldError, NativeSpriteRecordFields};

#[cfg(test)]
use stream_codec::checked_native_stream_len;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteToken {
    Record(SpriteRecord),
    /// Lunar Magic 3.x `FF 00..7F` command setting the upper seven Y-position bits.
    ///
    /// The historical variant name is retained for interchange/API compatibility.
    Screen(u8),
    Control(u8),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeSpriteStream {
    pub header: u8,
    pub expanded: bool,
    pub tokens: Vec<SpriteToken>,
}

impl NativeSpriteStream {
    /// Per-stream discriminator written by Lunar Magic's native sprite serializer.
    pub const EXPANDED_HEADER_FLAG: u8 = 0x20;

    /// Reports whether a serialized header selects expanded control/escape framing.
    #[must_use]
    pub const fn header_uses_expanded_framing(header: u8) -> bool {
        header & Self::EXPANDED_HEADER_FLAG != 0
    }

    /// Reports whether any token requires Lunar Magic's expanded control/escape grammar.
    #[must_use]
    pub fn requires_expanded_framing(&self) -> bool {
        self.tokens.iter().any(|token| match token {
            SpriteToken::Screen(_) | SpriteToken::Control(_) => true,
            SpriteToken::Record(record) => record.encoded.first() == Some(&0xff),
        })
    }

    /// Selects Lunar Magic's canonical framing from the tokens and synchronizes header bit `$20`.
    pub fn canonicalize_framing(&mut self) {
        self.expanded = self.requires_expanded_framing();
        if self.expanded {
            self.header |= Self::EXPANDED_HEADER_FLAG;
        } else {
            self.header &= !Self::EXPANDED_HEADER_FLAG;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeSpriteEncodingError {
    RecordTooShort {
        token: usize,
        len: usize,
    },
    UnknownRecordLength {
        token: usize,
    },
    RecordLengthMismatch {
        token: usize,
        expected: usize,
        actual: usize,
    },
    LegacyControlToken {
        token: usize,
    },
    LegacyTerminatorCollision {
        token: usize,
    },
    InvalidScreen {
        token: usize,
        value: u8,
    },
    InvalidControl {
        token: usize,
        value: u8,
    },
    SizeOverflow,
}

impl std::fmt::Display for NativeSpriteEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid native sprite encoding: {self:?}")
    }
}

impl std::error::Error for NativeSpriteEncodingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_table_matches_recovered_initializer() {
        assert!(
            SpriteLengthTable::standard()
                .encoded()
                .iter()
                .all(|length| *length == 3)
        );
    }

    #[test]
    fn custom_record_length_round_trips() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(2, 0x42, 5).unwrap();
        let bytes = [0x10, 0x08, 0x20, 0x42, 1, 2, 0xff];
        let stream = NativeSpriteStream::parse(&bytes, false, &lengths).unwrap();
        assert_eq!(stream.encode_checked().unwrap(), bytes);
    }

    #[test]
    fn length_table_edits_reject_selector_aliases_and_preserve_every_entry() {
        let mut lengths = SpriteLengthTable::standard();
        let before = lengths.clone();
        assert_eq!(
            lengths.set(4, 0x42, 5),
            Err(SpriteLengthTableError::TableOutOfRange(4))
        );
        assert_eq!(lengths, before);
        assert_eq!(
            lengths.set(0, 0x42, 2),
            Err(SpriteLengthTableError::RecordTooShort(2))
        );
        assert_eq!(lengths, before);
    }

    #[test]
    fn expanded_controls_and_escaped_record_are_lossless() {
        let bytes = [
            0x30, 0xff, 0x12, 0x00, 0x20, 0x01, 0xff, 0xff, 0x30, 0x02, 0xff, 0x90, 0xff, 0xfe,
        ];
        let stream =
            NativeSpriteStream::parse(&bytes, true, &SpriteLengthTable::standard()).unwrap();
        assert!(matches!(stream.tokens[0], SpriteToken::Screen(0x12)));
        assert!(matches!(stream.tokens[3], SpriteToken::Control(0x90)));
        assert_eq!(stream.encode_checked().unwrap(), bytes);
    }

    #[test]
    fn encoding_canonicalizes_the_header_framing_bit() {
        let mut stream = NativeSpriteStream {
            header: 0xff,
            expanded: false,
            tokens: Vec::new(),
        };
        assert_eq!(stream.encode_checked().unwrap(), [0xdf, 0xff]);

        stream.expanded = true;
        stream.header = 0;
        assert_eq!(stream.encode_checked().unwrap(), [0x20, 0xff, 0xfe]);
    }

    #[test]
    fn checked_encoding_rejects_tokens_that_alias_terminators_or_change_value() {
        for token in [
            SpriteToken::Screen(0x80),
            SpriteToken::Control(0x7f),
            SpriteToken::Control(0xfe),
            SpriteToken::Control(0xff),
            SpriteToken::Record(SpriteRecord {
                encoded: vec![1, 2],
            }),
        ] {
            let stream = NativeSpriteStream {
                header: 0,
                expanded: true,
                tokens: vec![token],
            };
            assert!(stream.encode_checked().is_err());
        }
        let legacy = NativeSpriteStream {
            header: 0,
            expanded: false,
            tokens: vec![SpriteToken::Record(SpriteRecord {
                encoded: vec![0xff, 0, 1],
            })],
        };
        assert!(matches!(
            legacy.encode_checked(),
            Err(NativeSpriteEncodingError::LegacyTerminatorCollision { .. })
        ));
    }

    #[test]
    fn revision_table_validation_covers_every_selector_and_sprite_id() {
        let mut lengths = SpriteLengthTable::standard();
        for table in 0_u8..4 {
            for sprite_id in 0_u8..=u8::MAX {
                let expected = 3 + usize::from((table ^ sprite_id) & 3);
                lengths
                    .set(table, sprite_id, u8::try_from(expected).unwrap())
                    .unwrap();
                let mut record = vec![table << 2, 0, sprite_id];
                record.resize(expected, sprite_id);
                let stream = NativeSpriteStream {
                    header: 0x10,
                    expanded: false,
                    tokens: vec![SpriteToken::Record(SpriteRecord { encoded: record })],
                };
                let encoded = stream.encode_for_table(&lengths).unwrap();
                assert_eq!(
                    NativeSpriteStream::parse(&encoded, false, &lengths).unwrap(),
                    stream
                );
            }
        }
    }

    #[test]
    fn revision_table_mismatch_reports_the_exact_token_without_serializing() {
        let mut lengths = SpriteLengthTable::standard();
        lengths.set(3, 0x42, 6).unwrap();
        let stream = NativeSpriteStream {
            header: 0,
            expanded: true,
            tokens: vec![
                SpriteToken::Screen(2),
                SpriteToken::Record(SpriteRecord {
                    encoded: vec![0x0c, 0, 0x42, 1, 2],
                }),
            ],
        };
        assert_eq!(
            stream.encode_for_table(&lengths),
            Err(NativeSpriteEncodingError::RecordLengthMismatch {
                token: 1,
                expected: 6,
                actual: 5,
            })
        );
    }

    #[test]
    fn legacy_and_expanded_aggregate_overflow_is_typed_without_allocating() {
        assert_eq!(
            checked_native_stream_len(false, [Ok(3), Ok(5)]).unwrap(),
            10
        );
        assert_eq!(checked_native_stream_len(true, [Ok(2), Ok(4)]).unwrap(), 9);
        for expanded in [false, true] {
            assert_eq!(
                checked_native_stream_len(expanded, [Ok(usize::MAX)]),
                Err(NativeSpriteEncodingError::SizeOverflow)
            );
        }
    }
}
