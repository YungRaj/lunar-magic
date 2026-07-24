//! Lossless Lunar Magic ROM attribution and packed feature metadata.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LunarMagicRomMetadata {
    attribution: [u8; Self::ATTRIBUTION_LEN],
    vram_version: u8,
    feature_record: [u8; Self::FEATURE_LEN],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LunarMagicRomMetadataError {
    AttributionLength(usize),
    FeatureLength(usize),
    Signature,
    ChecksumStatus(u8),
}

impl std::fmt::Display for LunarMagicRomMetadataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Lunar Magic ROM metadata: {self:?}")
    }
}

impl std::error::Error for LunarMagicRomMetadataError {}

impl LunarMagicRomMetadata {
    pub const ATTRIBUTION_LEN: usize = 0xa0;
    pub const FEATURE_LEN: usize = 0x19;
    pub const SIGNATURE: &'static [u8] = b"Lunar Magic Version ";

    /// Constructs an exact metadata snapshot while validating its stable framing.
    ///
    /// # Errors
    ///
    /// Rejects wrong fixed lengths, an attribution without the recovered signature, or nonzero
    /// reserved high bits in the final checksum-status byte.
    pub fn from_parts(
        attribution: &[u8],
        vram_version: u8,
        feature_record: &[u8],
    ) -> Result<Self, LunarMagicRomMetadataError> {
        let attribution: [u8; Self::ATTRIBUTION_LEN] = attribution
            .try_into()
            .map_err(|_| LunarMagicRomMetadataError::AttributionLength(attribution.len()))?;
        let feature_record: [u8; Self::FEATURE_LEN] = feature_record
            .try_into()
            .map_err(|_| LunarMagicRomMetadataError::FeatureLength(feature_record.len()))?;
        if !attribution.starts_with(Self::SIGNATURE) {
            return Err(LunarMagicRomMetadataError::Signature);
        }
        if feature_record[Self::FEATURE_LEN - 1] & 0xf0 != 0 {
            return Err(LunarMagicRomMetadataError::ChecksumStatus(
                feature_record[Self::FEATURE_LEN - 1],
            ));
        }
        Ok(Self {
            attribution,
            vram_version,
            feature_record,
        })
    }

    #[must_use]
    pub const fn attribution(&self) -> &[u8; Self::ATTRIBUTION_LEN] {
        &self.attribution
    }

    #[must_use]
    pub const fn vram_version(&self) -> u8 {
        self.vram_version
    }

    #[must_use]
    pub const fn feature_record(&self) -> &[u8; Self::FEATURE_LEN] {
        &self.feature_record
    }

    #[must_use]
    pub fn feature_bits(&self) -> u32 {
        u32::from_le_bytes(self.feature_record[..4].try_into().unwrap_or_default())
    }

    #[must_use]
    pub const fn compression_configuration(&self) -> u8 {
        self.feature_record[4]
    }

    #[must_use]
    pub const fn mapping_configuration(&self) -> u8 {
        self.feature_record[5]
    }

    #[must_use]
    pub fn runtime_pointer(&self, index: usize) -> Option<u32> {
        let start = 9usize.checked_add(index.checked_mul(3)?)?;
        let bytes = self.feature_record.get(start..start + 3)?;
        Some(u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16)
    }

    #[must_use]
    pub const fn checksum_status(&self) -> u8 {
        self.feature_record[Self::FEATURE_LEN - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_fields_and_five_runtime_pointers_are_addressable() {
        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let mut feature = [0; LunarMagicRomMetadata::FEATURE_LEN];
        feature[..4].copy_from_slice(&0x1234_5678_u32.to_le_bytes());
        feature[4] = 0xa2;
        feature[5] = 0xb3;
        feature[9..24].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        let metadata = LunarMagicRomMetadata::from_parts(&attribution, 1, &feature).unwrap();
        assert_eq!(metadata.feature_bits(), 0x1234_5678);
        assert_eq!(metadata.compression_configuration(), 0xa2);
        assert_eq!(metadata.mapping_configuration(), 0xb3);
        assert_eq!(metadata.runtime_pointer(0), Some(0x03_0201));
        assert_eq!(metadata.runtime_pointer(4), Some(0x0f_0e0d));
        assert_eq!(metadata.runtime_pointer(5), None);
    }
}
