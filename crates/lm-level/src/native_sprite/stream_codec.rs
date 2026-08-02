use super::{NativeSpriteEncodingError, NativeSpriteStream, SpriteLengthTable, SpriteToken};
use crate::{SpriteRecord, SpriteStreamError};

impl NativeSpriteStream {
    /// Parses legacy or expanded serialized sprite data with screen-control preservation.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteStreamError`] for truncation, invalid length-table entries, or missing
    /// terminators.
    pub fn parse(
        bytes: &[u8],
        expanded: bool,
        lengths: &SpriteLengthTable,
    ) -> Result<Self, SpriteStreamError> {
        let (&header, _) = bytes
            .split_first()
            .ok_or(SpriteStreamError::Truncated { offset: 0 })?;
        let mut offset = 1;
        let mut tokens = Vec::new();
        loop {
            let Some(&first) = bytes.get(offset) else {
                return Err(SpriteStreamError::MissingTerminator);
            };
            if first == 0xff {
                if !expanded {
                    return Ok(Self {
                        header,
                        expanded,
                        tokens,
                    });
                }
                let control = *bytes
                    .get(offset + 1)
                    .ok_or(SpriteStreamError::Truncated { offset })?;
                if control == 0xfe {
                    return Ok(Self {
                        header,
                        expanded,
                        tokens,
                    });
                }
                if control < 0x80 {
                    tokens.push(SpriteToken::Screen(control));
                    offset += 2;
                    continue;
                }
                if control != 0xff {
                    tokens.push(SpriteToken::Control(control));
                    offset += 2;
                    continue;
                }
                offset += 1;
            }

            let remaining = &bytes[offset..];
            let len = lengths
                .record_len(remaining)
                .ok_or(SpriteStreamError::UnknownRecordLength { offset })?;
            let end = offset
                .checked_add(len)
                .ok_or(SpriteStreamError::Truncated { offset })?;
            let encoded = bytes
                .get(offset..end)
                .ok_or(SpriteStreamError::Truncated { offset })?
                .to_vec();
            tokens.push(SpriteToken::Record(SpriteRecord { encoded }));
            offset = end;
        }
    }

    fn encode_validated(&self) -> Result<Vec<u8>, NativeSpriteEncodingError> {
        let mut bytes = Vec::with_capacity(self.encoded_len()?);
        // Lunar Magic stores the framing discriminator in bit $20 of the stream header. Its
        // serializer clears the bit for the one-byte legacy terminator and sets it whenever the
        // expanded control/escape grammar and `$FF $FE` terminator are emitted.
        bytes.push(
            (self.header & !Self::EXPANDED_HEADER_FLAG)
                | if self.expanded {
                    Self::EXPANDED_HEADER_FLAG
                } else {
                    0
                },
        );
        for token in &self.tokens {
            match token {
                SpriteToken::Record(record) => {
                    if self.expanded && record.encoded.first() == Some(&0xff) {
                        bytes.push(0xff);
                    }
                    bytes.extend_from_slice(&record.encoded);
                }
                SpriteToken::Screen(screen) => bytes.extend_from_slice(&[0xff, *screen]),
                SpriteToken::Control(control) => bytes.extend_from_slice(&[0xff, *control]),
            }
        }
        if self.expanded {
            bytes.extend_from_slice(&[0xff, 0xfe]);
        } else {
            bytes.push(0xff);
        }
        Ok(bytes)
    }

    /// Validates universal token framing before encoding a native stream.
    ///
    /// # Errors
    ///
    /// Returns [`NativeSpriteEncodingError`] for short records or tokens incompatible with the
    /// selected legacy/expanded framing.
    pub fn encode_checked(&self) -> Result<Vec<u8>, NativeSpriteEncodingError> {
        self.validate_framing()?;
        self.encode_validated()
    }

    fn validate_framing(&self) -> Result<(), NativeSpriteEncodingError> {
        for (token, value) in self.tokens.iter().enumerate() {
            match value {
                SpriteToken::Record(record) if record.encoded.len() < 3 => {
                    return Err(NativeSpriteEncodingError::RecordTooShort {
                        token,
                        len: record.encoded.len(),
                    });
                }
                SpriteToken::Record(record)
                    if !self.expanded && record.encoded.first() == Some(&0xff) =>
                {
                    return Err(NativeSpriteEncodingError::LegacyTerminatorCollision { token });
                }
                SpriteToken::Screen(_) | SpriteToken::Control(_) if !self.expanded => {
                    return Err(NativeSpriteEncodingError::LegacyControlToken { token });
                }
                SpriteToken::Screen(value) if *value > 0x7f => {
                    return Err(NativeSpriteEncodingError::InvalidScreen {
                        token,
                        value: *value,
                    });
                }
                SpriteToken::Control(value) if !(0x80..=0xfd).contains(value) => {
                    return Err(NativeSpriteEncodingError::InvalidControl {
                        token,
                        value: *value,
                    });
                }
                SpriteToken::Record(_) | SpriteToken::Screen(_) | SpriteToken::Control(_) => {}
            }
        }
        Ok(())
    }

    fn encoded_len(&self) -> Result<usize, NativeSpriteEncodingError> {
        let lengths = self.tokens.iter().map(|token| match token {
            SpriteToken::Record(record)
                if self.expanded && record.encoded.first() == Some(&0xff) =>
            {
                record
                    .encoded
                    .len()
                    .checked_add(1)
                    .ok_or(NativeSpriteEncodingError::SizeOverflow)
            }
            SpriteToken::Record(record) => Ok(record.encoded.len()),
            SpriteToken::Screen(_) | SpriteToken::Control(_) => Ok(2),
        });
        checked_native_stream_len(self.expanded, lengths)
    }

    /// Validates framing and every record against the revision-specific length table.
    ///
    /// # Errors
    ///
    /// Returns a typed framing or record-length error without emitting partial bytes.
    pub fn encode_for_table(
        &self,
        lengths: &SpriteLengthTable,
    ) -> Result<Vec<u8>, NativeSpriteEncodingError> {
        self.validate_framing()?;
        for (token, value) in self.tokens.iter().enumerate() {
            let SpriteToken::Record(record) = value else {
                continue;
            };
            let expected = lengths
                .record_len(&record.encoded)
                .ok_or(NativeSpriteEncodingError::UnknownRecordLength { token })?;
            if record.encoded.len() != expected {
                return Err(NativeSpriteEncodingError::RecordLengthMismatch {
                    token,
                    expected,
                    actual: record.encoded.len(),
                });
            }
        }
        self.encode_validated()
    }
}

pub(super) fn checked_native_stream_len(
    expanded: bool,
    token_lengths: impl IntoIterator<Item = Result<usize, NativeSpriteEncodingError>>,
) -> Result<usize, NativeSpriteEncodingError> {
    let terminator_len = if expanded { 2 } else { 1 };
    token_lengths
        .into_iter()
        .try_fold(1_usize + terminator_len, |total, token_len| {
            total
                .checked_add(token_len?)
                .ok_or(NativeSpriteEncodingError::SizeOverflow)
        })
}
