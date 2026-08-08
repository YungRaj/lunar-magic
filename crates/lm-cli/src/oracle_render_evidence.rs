use lm_oracle::{Observation, sha256_hex};
use std::fmt;

pub const MAX_RELEASE_RENDER_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderEvidenceError {
    MissingObservation(&'static str),
    InvalidObservedDimension(&'static str),
    HashMismatch {
        expected: String,
        actual: String,
    },
    Truncated,
    InvalidSignature,
    InvalidChunkLength,
    InvalidChunkCrc,
    InvalidChunkOrder,
    UnsupportedIhdr,
    DimensionMismatch {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    MissingImageData,
    TrailingBytes,
}

impl fmt::Display for RenderEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid release render artifact: {self:?}")
    }
}

impl std::error::Error for RenderEvidenceError {}

/// Binds one release `render.png` to its observed digest and dimensions.
///
/// Every PNG chunk is length-bounded and CRC-checked. The image must be non-interlaced 8-bit RGBA,
/// begin with one IHDR, contain image data, and end exactly at IEND.
///
/// # Errors
///
/// Rejects missing or malformed observation fields, digest/dimension mismatches, malformed PNG
/// framing, invalid CRCs, unsupported IHDR fields, missing image data, and trailing bytes.
pub fn validate_release_render(
    bytes: &[u8],
    observation: &Observation,
) -> Result<(), RenderEvidenceError> {
    let expected_hash = required(observation, "release/render-level/png-sha256")?;
    let expected_width = dimension(observation, "release/render-level/width")?;
    let expected_height = dimension(observation, "release/render-level/height")?;
    validate_png(bytes, expected_hash, expected_width, expected_height)
}

/// Binds one emulator screenshot PNG to its observed digest and dimensions.
///
/// # Errors
///
/// Returns the same bounded structural, digest, and dimension failures as release rendering.
pub fn validate_emulator_screenshot(
    bytes: &[u8],
    observation: &Observation,
) -> Result<(), RenderEvidenceError> {
    let expected_hash = required(observation, "release/emulator-boot/screenshot-sha256")?;
    let expected_width = dimension(observation, "release/emulator-boot/screenshot-width")?;
    let expected_height = dimension(observation, "release/emulator-boot/screenshot-height")?;
    validate_png(bytes, expected_hash, expected_width, expected_height)
}

fn validate_png(
    bytes: &[u8],
    expected_hash: &str,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), RenderEvidenceError> {
    let actual_hash = sha256_hex(bytes);
    if actual_hash != expected_hash {
        return Err(RenderEvidenceError::HashMismatch {
            expected: expected_hash.into(),
            actual: actual_hash,
        });
    }
    if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n") {
        return Err(if bytes.len() < 8 {
            RenderEvidenceError::Truncated
        } else {
            RenderEvidenceError::InvalidSignature
        });
    }

    let mut offset = 8_usize;
    let mut chunk_index = 0_usize;
    let mut dimensions = None;
    let mut has_image_data = false;
    let mut ended = false;
    while offset < bytes.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or(RenderEvidenceError::InvalidChunkLength)?;
        let header = bytes
            .get(offset..header_end)
            .ok_or(RenderEvidenceError::Truncated)?;
        let length = usize::try_from(u32::from_be_bytes(header[..4].try_into().unwrap()))
            .map_err(|_| RenderEvidenceError::InvalidChunkLength)?;
        let kind: [u8; 4] = header[4..].try_into().unwrap();
        let data_start = header_end;
        let data_end = data_start
            .checked_add(length)
            .ok_or(RenderEvidenceError::InvalidChunkLength)?;
        let chunk_end = data_end
            .checked_add(4)
            .ok_or(RenderEvidenceError::InvalidChunkLength)?;
        let data = bytes
            .get(data_start..data_end)
            .ok_or(RenderEvidenceError::Truncated)?;
        let recorded_crc = bytes
            .get(data_end..chunk_end)
            .ok_or(RenderEvidenceError::Truncated)?;
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(&kind);
        crc_input.extend_from_slice(data);
        if crc32(&crc_input).to_be_bytes() != recorded_crc {
            return Err(RenderEvidenceError::InvalidChunkCrc);
        }
        match &kind {
            b"IHDR" if chunk_index == 0 && data.len() == 13 && dimensions.is_none() => {
                let width = u32::from_be_bytes(data[..4].try_into().unwrap());
                let height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                if width == 0 || height == 0 || data[8..] != [8, 6, 0, 0, 0] {
                    return Err(RenderEvidenceError::UnsupportedIhdr);
                }
                dimensions = Some((width, height));
            }
            b"IDAT" if dimensions.is_some() && !ended => has_image_data = true,
            b"IEND" if dimensions.is_some() && data.is_empty() && !ended => ended = true,
            b"IEND" | b"IHDR" => return Err(RenderEvidenceError::InvalidChunkOrder),
            _ if ended => return Err(RenderEvidenceError::TrailingBytes),
            _ => {}
        }
        offset = chunk_end;
        chunk_index += 1;
        if ended && offset != bytes.len() {
            return Err(RenderEvidenceError::TrailingBytes);
        }
    }
    let (actual_width, actual_height) = dimensions.ok_or(RenderEvidenceError::InvalidChunkOrder)?;
    if !ended {
        return Err(RenderEvidenceError::Truncated);
    }
    if !has_image_data {
        return Err(RenderEvidenceError::MissingImageData);
    }
    if (actual_width, actual_height) != (expected_width, expected_height) {
        return Err(RenderEvidenceError::DimensionMismatch {
            expected_width,
            expected_height,
            actual_width,
            actual_height,
        });
    }
    Ok(())
}

fn required<'a>(
    observation: &'a Observation,
    path: &'static str,
) -> Result<&'a str, RenderEvidenceError> {
    observation
        .get(path)
        .ok_or(RenderEvidenceError::MissingObservation(path))
}

fn dimension(observation: &Observation, path: &'static str) -> Result<u32, RenderEvidenceError> {
    required(observation, path)?
        .parse::<u32>()
        .ok()
        .filter(|value| *value != 0)
        .ok_or(RenderEvidenceError::InvalidObservedDimension(path))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_render::{Canvas, encode_png};
    use std::path::PathBuf;

    fn evidence(bytes: &[u8], width: &str, height: &str) -> Observation {
        let mut observation = Observation::new();
        observation
            .insert("release/render-level/png-sha256", sha256_hex(bytes))
            .unwrap();
        observation
            .insert("release/render-level/width", width)
            .unwrap();
        observation
            .insert("release/render-level/height", height)
            .unwrap();
        observation
    }

    #[test]
    fn valid_png_is_bound_to_hash_dimensions_and_structure() {
        let bytes = encode_png(&Canvas::try_new(2, 3).unwrap()).unwrap();
        validate_release_render(&bytes, &evidence(&bytes, "2", "3")).unwrap();
        assert!(matches!(
            validate_release_render(&bytes, &evidence(&bytes, "3", "2")),
            Err(RenderEvidenceError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            validate_release_render(&bytes, &evidence(b"different", "2", "3")),
            Err(RenderEvidenceError::HashMismatch { .. })
        ));
    }

    #[test]
    fn corruption_truncation_and_trailing_data_are_rejected() {
        let bytes = encode_png(&Canvas::try_new(1, 1).unwrap()).unwrap();
        let mut corrupt = bytes.clone();
        corrupt[29] ^= 1;
        assert!(matches!(
            validate_release_render(&corrupt, &evidence(&corrupt, "1", "1")),
            Err(RenderEvidenceError::InvalidChunkCrc)
        ));
        for malformed in [&bytes[..7], &bytes[..bytes.len() - 1]] {
            assert!(validate_release_render(malformed, &evidence(malformed, "1", "1")).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            validate_release_render(&trailing, &evidence(&trailing, "1", "1")),
            Err(RenderEvidenceError::TrailingBytes)
        ));
    }

    #[test]
    fn retained_legacy_import_dialog_captures_are_hash_and_structure_bound() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/oracle-work/lm363/pristine-us/legacy-import-prompts");
        for (name, digest) in [
            (
                "optional-palette-missing.png",
                "6067a650c910c7ae151464d6344ebf090b640d7f3f134a2822027047d488bc0f",
            ),
            (
                "required-layer1-missing.png",
                "233fd519ca61e3c11446e9cb956e3e5003c7aa35f5e85bb93526097dbb00cf99",
            ),
            (
                "required-layer2-missing.png",
                "62135f1c0338572edeb4c59ac003b9965d46fe5823f05fa569ee09fbefc0449c",
            ),
            (
                "required-sprites-missing.png",
                "5b6780f05cc60701eb12f44cd903e7a511a30cc115057352f04f01581186130a",
            ),
        ] {
            let bytes = std::fs::read(root.join(name)).unwrap();
            validate_png(&bytes, digest, 1424, 1296).unwrap();
        }
    }
}
