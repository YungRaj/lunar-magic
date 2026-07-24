use crate::{TitleScreenRecording, zsnes::decode_sram};
use flate2::read::GzDecoder;
use std::io::Read;

const HEADER_OFFSET: usize = 0x0e;
const BLOCK_HEADER_LEN: usize = 0x0b;
const RAM_LEN: usize = 0x2_0000;
const MAX_INFLATED_LEN: u64 = 64 * 1024 * 1024;

#[derive(Debug)]
pub enum Snes9xTitleRecordingError {
    Io(std::io::Error),
    Header,
    TruncatedBlock { offset: usize },
    BlockLength { offset: usize },
    BlockOverflow { offset: usize },
    MissingRam,
    Recording(crate::ZsnesTitleRecordingError),
}

impl std::fmt::Display for Snes9xTitleRecordingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Snes9x title recording state: {self:?}")
    }
}

impl std::error::Error for Snes9xTitleRecordingError {}

impl From<std::io::Error> for Snes9xTitleRecordingError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Reads an uncompressed or gzip Snes9x snapshot and extracts its tagged `RAM` block.
///
/// # Errors
///
/// Rejects unsupported snapshot signatures, malformed decimal block lengths, truncation,
/// missing/short RAM blocks, decompression failures, and malformed recording metadata.
pub fn decode_snes9x_title_recording(
    bytes: &[u8],
) -> Result<TitleScreenRecording, Snes9xTitleRecordingError> {
    let decoded;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b, 8]) {
        let mut reader = GzDecoder::new(bytes).take(MAX_INFLATED_LEN + 1);
        let mut output = Vec::new();
        reader.read_to_end(&mut output)?;
        if u64::try_from(output.len()).unwrap_or(u64::MAX) > MAX_INFLATED_LEN {
            return Err(Snes9xTitleRecordingError::BlockOverflow { offset: 0 });
        }
        decoded = output;
        decoded.as_slice()
    } else {
        bytes
    };
    if bytes.len() < HEADER_OFFSET
        || (!bytes.starts_with(b"#!snes9x:") && !bytes.starts_with(b"#!s9xsnp:"))
    {
        return Err(Snes9xTitleRecordingError::Header);
    }
    let mut offset = HEADER_OFFSET;
    while offset + BLOCK_HEADER_LEN <= bytes.len() {
        let header = &bytes[offset..offset + BLOCK_HEADER_LEN];
        let length_text = &header[4..10];
        if !length_text.iter().all(u8::is_ascii_digit) {
            return Err(Snes9xTitleRecordingError::BlockLength { offset });
        }
        let length = length_text.iter().try_fold(0usize, |value, digit| {
            value
                .checked_mul(10)?
                .checked_add(usize::from(*digit - b'0'))
        });
        let length = length.ok_or(Snes9xTitleRecordingError::BlockOverflow { offset })?;
        let payload = offset
            .checked_add(BLOCK_HEADER_LEN)
            .ok_or(Snes9xTitleRecordingError::BlockOverflow { offset })?;
        let end = payload
            .checked_add(length)
            .ok_or(Snes9xTitleRecordingError::BlockOverflow { offset })?;
        if end > bytes.len() {
            return Err(Snes9xTitleRecordingError::TruncatedBlock { offset });
        }
        if &header[..3] == b"RAM" && length >= RAM_LEN {
            return decode_sram(&bytes[payload..payload + RAM_LEN])
                .map_err(Snes9xTitleRecordingError::Recording);
        }
        offset = end;
    }
    Err(Snes9xTitleRecordingError::MissingRam)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode_zsnes_title_recording;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    fn snapshot(recording: &TitleScreenRecording) -> Vec<u8> {
        let zsnes = encode_zsnes_title_recording(recording);
        let sram = &zsnes[0x0c13..];
        let mut output = b"#!s9xsnp:0007\n".to_vec();
        output.extend_from_slice(b"CPU:000003:");
        output.extend_from_slice(&[1, 2, 3]);
        output.extend_from_slice(b"RAM:131072:");
        output.extend_from_slice(sram);
        output
    }

    #[test]
    fn tagged_and_gzip_snapshots_match_the_recovered_ram_walk() {
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let plain = snapshot(&recording);
        assert_eq!(decode_snes9x_title_recording(&plain).unwrap(), recording);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&plain).unwrap();
        let gzip = encoder.finish().unwrap();
        assert_eq!(decode_snes9x_title_recording(&gzip).unwrap(), recording);
    }
}
