use crate::{TitleScreenRecording, TitleScreenRecordingError};

const HEADER: &[u8] = b"ZSNES Save State File";
const OUTPUT_HEADER: &[u8] = b"ZSNES Save State File V143";
const STATE_PREFIX_LEN: usize = 0x0c13;
const SRAM_LEN: usize = 0x2_0000;
const RECORDING_OFFSET: usize = 0x1_0000;
const LENGTH_OFFSET: usize = 0x1_fff8;
const MARKER_OFFSET: usize = 0x1_fffc;
const MARKER: u16 = 0x0042;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZsnesTitleRecordingError {
    Truncated(usize),
    Header,
    Marker(u16),
    EncodedLength(usize),
    Recording(TitleScreenRecordingError),
}

impl std::fmt::Display for ZsnesTitleRecordingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid ZSNES title recording state: {self:?}")
    }
}

impl std::error::Error for ZsnesTitleRecordingError {}

impl From<TitleScreenRecordingError> for ZsnesTitleRecordingError {
    fn from(value: TitleScreenRecordingError) -> Self {
        Self::Recording(value)
    }
}

/// Extracts the title recording using Lunar Magic's exact ZSNES SRAM placement.
///
/// # Errors
///
/// Rejects truncated states, wrong headers/markers, overflowing encoded lengths, or malformed
/// movement payloads.
pub fn decode_zsnes_title_recording(
    bytes: &[u8],
) -> Result<TitleScreenRecording, ZsnesTitleRecordingError> {
    let required = STATE_PREFIX_LEN + SRAM_LEN;
    if bytes.len() < required {
        return Err(ZsnesTitleRecordingError::Truncated(bytes.len()));
    }
    if &bytes[..HEADER.len()] != HEADER {
        return Err(ZsnesTitleRecordingError::Header);
    }
    let sram = &bytes[STATE_PREFIX_LEN..required];
    decode_sram(sram)
}

pub(crate) fn decode_sram(sram: &[u8]) -> Result<TitleScreenRecording, ZsnesTitleRecordingError> {
    if sram.len() < SRAM_LEN {
        return Err(ZsnesTitleRecordingError::Truncated(sram.len()));
    }
    let marker = u16::from_le_bytes([sram[MARKER_OFFSET], sram[MARKER_OFFSET + 1]]);
    if marker != MARKER {
        return Err(ZsnesTitleRecordingError::Marker(marker));
    }
    let encoded = usize::from(u16::from_le_bytes([
        sram[LENGTH_OFFSET],
        sram[LENGTH_OFFSET + 1],
    ]));
    let length = encoded
        .checked_add(4)
        .ok_or(ZsnesTitleRecordingError::EncodedLength(encoded))?;
    if length > TitleScreenRecording::MAX_LEN {
        return Err(ZsnesTitleRecordingError::EncodedLength(encoded));
    }
    Ok(TitleScreenRecording::from_bytes(
        sram[RECORDING_OFFSET..RECORDING_OFFSET + length].to_vec(),
    )?)
}

/// Creates Lunar Magic's minimal zero-filled ZSNES V143 state.
#[must_use]
pub fn encode_zsnes_title_recording(recording: &TitleScreenRecording) -> Vec<u8> {
    let mut output = vec![0; STATE_PREFIX_LEN + SRAM_LEN];
    output[..OUTPUT_HEADER.len()].copy_from_slice(OUTPUT_HEADER);
    let sram = &mut output[STATE_PREFIX_LEN..];
    sram[RECORDING_OFFSET..RECORDING_OFFSET + recording.bytes().len()]
        .copy_from_slice(recording.bytes());
    let encoded = u16::try_from(recording.bytes().len() - 4)
        .unwrap_or(u16::MAX)
        .to_le_bytes();
    sram[LENGTH_OFFSET..LENGTH_OFFSET + 2].copy_from_slice(&encoded);
    sram[MARKER_OFFSET..MARKER_OFFSET + 2].copy_from_slice(&MARKER.to_le_bytes());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_v143_state_round_trips_exact_payload_and_metadata() {
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let state = encode_zsnes_title_recording(&recording);
        assert_eq!(state.len(), 0x20c13);
        assert_eq!(&state[..OUTPUT_HEADER.len()], OUTPUT_HEADER);
        assert_eq!(decode_zsnes_title_recording(&state).unwrap(), recording);
    }

    #[test]
    fn lunar_magic_truncated_batch_fixture_rejects_before_payload_access() {
        assert_eq!(
            decode_zsnes_title_recording(&[0; 12]),
            Err(ZsnesTitleRecordingError::Truncated(12))
        );
    }
}
