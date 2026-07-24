use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteRecord {
    pub encoded: Vec<u8>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpriteStream {
    pub header: u8,
    pub records: Vec<SpriteRecord>,
}

impl SpriteStream {
    /// Parses a terminated sprite stream using metadata-driven record lengths.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteStreamError`] for truncation, unknown lengths, or no terminator.
    pub fn parse_with(
        bytes: &[u8],
        mut record_len: impl FnMut(&[u8]) -> Option<usize>,
    ) -> Result<Self, SpriteStreamError> {
        let (&header, rest) = bytes
            .split_first()
            .ok_or(SpriteStreamError::Truncated { offset: 0 })?;
        let mut offset = 0;
        let mut records = Vec::new();
        loop {
            let Some(first) = rest.get(offset) else {
                return Err(SpriteStreamError::MissingTerminator);
            };
            if *first == 0xff {
                return Ok(Self { header, records });
            }
            let len = record_len(&rest[offset..])
                .ok_or(SpriteStreamError::UnknownRecordLength { offset: offset + 1 })?;
            let end = offset
                .checked_add(len)
                .ok_or(SpriteStreamError::Truncated { offset: offset + 1 })?;
            let encoded = rest
                .get(offset..end)
                .ok_or(SpriteStreamError::Truncated { offset: offset + 1 })?
                .to_vec();
            records.push(SpriteRecord { encoded });
            offset = end;
        }
    }

    /// Encodes the header, records, and terminator after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteStreamError::SizeOverflow`] if the aggregate length is not representable.
    pub fn encode(&self) -> Result<Vec<u8>, SpriteStreamError> {
        let mut result = Vec::with_capacity(sprite_stream_len(
            self.records.iter().map(|record| record.encoded.len()),
        )?);
        result.push(self.header);
        for record in &self.records {
            result.extend_from_slice(&record.encoded);
        }
        result.push(0xff);
        Ok(result)
    }
}

fn sprite_stream_len(
    record_lengths: impl IntoIterator<Item = usize>,
) -> Result<usize, SpriteStreamError> {
    record_lengths.into_iter().try_fold(2_usize, |total, len| {
        total
            .checked_add(len)
            .ok_or(SpriteStreamError::SizeOverflow)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpriteStreamError {
    MissingTerminator,
    UnknownRecordLength { offset: usize },
    Truncated { offset: usize },
    SizeOverflow,
}

impl fmt::Display for SpriteStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid sprite stream: {self:?}")
    }
}

impl std::error::Error for SpriteStreamError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_driven_stream_round_trips() {
        let bytes = [0x12, 1, 2, 3, 0x80, 4, 5, 6, 7, 0xff];
        let stream = SpriteStream::parse_with(&bytes, |remaining| {
            remaining
                .first()
                .map(|id| if id & 0x80 == 0 { 3 } else { 5 })
        })
        .unwrap();
        assert_eq!(stream.records.len(), 2);
        assert_eq!(stream.encode().unwrap(), bytes);
    }

    #[test]
    fn malformed_streams_are_rejected() {
        assert!(matches!(
            SpriteStream::parse_with(&[0, 1], |_| Some(3)),
            Err(SpriteStreamError::Truncated { .. })
        ));
        assert!(matches!(
            SpriteStream::parse_with(&[0, 1], |_| None),
            Err(SpriteStreamError::UnknownRecordLength { .. })
        ));
    }

    #[test]
    fn aggregate_length_overflow_is_typed_without_allocating() {
        assert_eq!(sprite_stream_len([3, 5]).unwrap(), 10);
        assert_eq!(
            sprite_stream_len([usize::MAX]),
            Err(SpriteStreamError::SizeOverflow)
        );
    }
}
