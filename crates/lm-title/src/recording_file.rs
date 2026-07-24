use crate::{TitleScreenRecording, TitleScreenRecordingError};

const MAGIC: &[u8; 8] = b"LMTITL01";
const HEADER_LEN: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TitleScreenRecordingFileError {
    Truncated(usize),
    Magic,
    Length { declared: usize, actual: usize },
    Recording(TitleScreenRecordingError),
}

impl std::fmt::Display for TitleScreenRecordingFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid title-screen recording file: {self:?}")
    }
}

impl std::error::Error for TitleScreenRecordingFileError {}

impl From<TitleScreenRecordingError> for TitleScreenRecordingFileError {
    fn from(value: TitleScreenRecordingError) -> Self {
        Self::Recording(value)
    }
}

impl TitleScreenRecording {
    pub const MAX_FILE_LEN: usize = HEADER_LEN + Self::MAX_LEN;

    #[must_use]
    pub fn encode_native_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(HEADER_LEN + self.bytes().len());
        output.extend_from_slice(MAGIC);
        let length = u16::try_from(self.bytes().len()).unwrap_or(u16::MAX);
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(self.bytes());
        output
    }

    /// Decodes one exact allocation-independent `LMTITL01` recording.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing bytes, invalid lengths, or missing terminators.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, TitleScreenRecordingFileError> {
        if bytes.len() < HEADER_LEN {
            return Err(TitleScreenRecordingFileError::Truncated(bytes.len()));
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(TitleScreenRecordingFileError::Magic);
        }
        let declared = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        let actual = bytes.len() - HEADER_LEN;
        if declared != actual {
            return Err(TitleScreenRecordingFileError::Length { declared, actual });
        }
        Ok(Self::from_bytes(bytes[HEADER_LEN..].to_vec())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_file_round_trips_without_allocation_state() {
        let recording = TitleScreenRecording::from_bytes(vec![1, 2, 3, 0xff]).unwrap();
        assert_eq!(
            TitleScreenRecording::decode_native_file(&recording.encode_native_file()).unwrap(),
            recording
        );
    }
}
