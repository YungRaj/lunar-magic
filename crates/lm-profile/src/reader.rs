use crate::{RevisionProfile, RevisionProfileError};
use std::fmt;
use std::io::{self, Read};
use std::string::FromUtf8Error;

#[derive(Debug)]
pub enum RevisionProfileReadError {
    Io(io::Error),
    Utf8(FromUtf8Error),
    Profile(RevisionProfileError),
}

impl fmt::Display for RevisionProfileReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "cannot read revision profile: {self:?}")
    }
}

impl std::error::Error for RevisionProfileReadError {}

pub(super) fn read(reader: impl Read) -> Result<RevisionProfile, RevisionProfileReadError> {
    let limit = u64::try_from(RevisionProfile::MAX_TEXT_LEN)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(RevisionProfile::MAX_TEXT_LEN.min(4096));
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(RevisionProfileReadError::Io)?;
    if bytes.len() > RevisionProfile::MAX_TEXT_LEN {
        return Err(RevisionProfileReadError::Profile(
            RevisionProfileError::TextTooLong {
                actual: bytes.len(),
                maximum: RevisionProfile::MAX_TEXT_LEN,
            },
        ));
    }
    let text = String::from_utf8(bytes).map_err(RevisionProfileReadError::Utf8)?;
    RevisionProfile::parse(&text).map_err(RevisionProfileReadError::Profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reader_stops_one_byte_beyond_limit_and_rejects_invalid_utf8() {
        let oversized = vec![b'x'; RevisionProfile::MAX_TEXT_LEN + 1000];
        assert!(matches!(
            read(Cursor::new(oversized)),
            Err(RevisionProfileReadError::Profile(
                RevisionProfileError::TextTooLong {
                    actual,
                    maximum: RevisionProfile::MAX_TEXT_LEN,
                }
            )) if actual == RevisionProfile::MAX_TEXT_LEN + 1
        ));
        assert!(matches!(
            read(Cursor::new(vec![0xff])),
            Err(RevisionProfileReadError::Utf8(_))
        ));
    }
}
