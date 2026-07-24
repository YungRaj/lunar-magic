use crate::{LunarMagicRomMetadata, LunarMagicRomMetadataError};

const MAGIC: &[u8; 8] = b"LMROMMD1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LunarMagicRomMetadataFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Metadata(LunarMagicRomMetadataError),
}

impl std::fmt::Display for LunarMagicRomMetadataFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Lunar Magic ROM metadata file: {self:?}")
    }
}

impl std::error::Error for LunarMagicRomMetadataFileError {}

impl From<LunarMagicRomMetadataError> for LunarMagicRomMetadataFileError {
    fn from(value: LunarMagicRomMetadataError) -> Self {
        Self::Metadata(value)
    }
}

impl LunarMagicRomMetadata {
    pub const FILE_LEN: usize = 8 + Self::ATTRIBUTION_LEN + 1 + Self::FEATURE_LEN;

    #[must_use]
    pub fn encode_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(self.attribution());
        output.push(self.vram_version());
        output.extend_from_slice(self.feature_record());
        output
    }

    /// Decodes one exact allocation-independent `LMROMMD1` snapshot.
    ///
    /// # Errors
    ///
    /// Rejects wrong length/magic or invalid metadata framing.
    pub fn decode_file(bytes: &[u8]) -> Result<Self, LunarMagicRomMetadataFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(LunarMagicRomMetadataFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(LunarMagicRomMetadataFileError::Magic);
        }
        let attribution_end = 8 + Self::ATTRIBUTION_LEN;
        Ok(Self::from_parts(
            &bytes[8..attribution_end],
            bytes[attribution_end],
            &bytes[attribution_end + 1..],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_round_trips_all_opaque_bytes() {
        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let metadata = LunarMagicRomMetadata::from_parts(
            &attribution,
            3,
            &[0; LunarMagicRomMetadata::FEATURE_LEN],
        )
        .unwrap();
        assert_eq!(
            LunarMagicRomMetadata::decode_file(&metadata.encode_file()).unwrap(),
            metadata
        );
    }
}
