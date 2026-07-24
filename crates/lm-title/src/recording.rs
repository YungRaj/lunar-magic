#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TitleScreenRecording {
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TitleScreenRecordingError {
    Length(usize),
    MissingTerminator,
}

impl std::fmt::Display for TitleScreenRecordingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid title-screen recording: {self:?}")
    }
}

impl std::error::Error for TitleScreenRecordingError {}

impl TitleScreenRecording {
    pub const MIN_LEN: usize = 4;
    pub const MAX_LEN: usize = 0x8000;
    pub const TERMINATOR: u8 = 0xff;

    /// Preserves the exact movement-data payload accepted by Lunar Magic's savestate importer.
    ///
    /// # Errors
    ///
    /// Rejects payloads outside `$4..=$8000` bytes or without the required final `$FF`.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, TitleScreenRecordingError> {
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&bytes.len()) {
            return Err(TitleScreenRecordingError::Length(bytes.len()));
        }
        if bytes.last() != Some(&Self::TERMINATOR) {
            return Err(TitleScreenRecordingError::MissingTerminator);
        }
        Ok(Self { bytes })
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_bounds_and_terminator_are_enforced() {
        assert!(TitleScreenRecording::from_bytes(vec![0, 0, 0, 0xff]).is_ok());
        assert!(matches!(
            TitleScreenRecording::from_bytes(vec![0, 0, 0]),
            Err(TitleScreenRecordingError::Length(3))
        ));
        assert!(matches!(
            TitleScreenRecording::from_bytes(vec![0; 4]),
            Err(TitleScreenRecordingError::MissingTerminator)
        ));
        let mut maximum = vec![0; TitleScreenRecording::MAX_LEN];
        *maximum.last_mut().unwrap() = 0xff;
        assert!(TitleScreenRecording::from_bytes(maximum).is_ok());
    }
}
