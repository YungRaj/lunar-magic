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
    pub const FASTROM_MARKER: u8 = b'B';
    const USE_FASTROM_INDEX: usize = 6;
    const FASTROM_EVER_ENABLED_INDEX: usize = 7;
    const FASTROM_PATCH_INDEX: usize = 8;
    const SA1_RAM_REMAP_BIT: u32 = 1 << 17;

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

    /// Whether mapper-aware inserted ASM relocations use the SA-1 Pack IRAM/WRAM remap.
    #[must_use]
    pub fn sa1_ram_remap(&self) -> bool {
        self.feature_bits() & Self::SA1_RAM_REMAP_BIT != 0
    }

    /// Returns an exact metadata copy with packed feature bit 17 updated.
    #[must_use]
    pub fn with_sa1_ram_remap(&self, enabled: bool) -> Self {
        let mut updated = self.clone();
        let mut bits = updated.feature_bits();
        if enabled {
            bits |= Self::SA1_RAM_REMAP_BIT;
        } else {
            bits &= !Self::SA1_RAM_REMAP_BIT;
        }
        updated.feature_record[..4].copy_from_slice(&bits.to_le_bytes());
        updated
    }

    #[must_use]
    pub const fn compression_configuration(&self) -> u8 {
        self.feature_record[4]
    }

    #[must_use]
    pub const fn mapping_configuration(&self) -> u8 {
        self.feature_record[5]
    }

    /// Whether newly saved pointers use the FastROM `$80-$FF` LoROM mirrors.
    #[must_use]
    pub const fn use_fastrom_addressing(&self) -> bool {
        self.feature_record[Self::USE_FASTROM_INDEX] == Self::FASTROM_MARKER
    }

    /// Whether FastROM addressing was ever enabled, permanently ruling out later ExLoROM use.
    #[must_use]
    pub const fn fastrom_ever_enabled(&self) -> bool {
        self.feature_record[Self::FASTROM_EVER_ENABLED_INDEX] == Self::FASTROM_MARKER
    }

    /// Whether Lunar Magic's irreversible original-game FastROM speed patch is installed.
    #[must_use]
    pub const fn fastrom_speed_patch_applied(&self) -> bool {
        self.feature_record[Self::FASTROM_PATCH_INDEX] == Self::FASTROM_MARKER
    }

    /// Returns an exact metadata copy with the ROM-scoped addressing option updated.
    /// Enabling permanently records the historical lock; disabling never clears that lock.
    #[must_use]
    pub fn with_use_fastrom_addressing(&self, enabled: bool) -> Self {
        let mut updated = self.clone();
        updated.feature_record[Self::USE_FASTROM_INDEX] =
            if enabled { Self::FASTROM_MARKER } else { 0 };
        if enabled {
            updated.feature_record[Self::FASTROM_EVER_ENABLED_INDEX] = Self::FASTROM_MARKER;
        }
        updated
    }

    /// Returns an exact metadata copy with the irreversible speed-patch marker installed.
    #[must_use]
    pub fn with_fastrom_speed_patch_applied(&self) -> Self {
        let mut updated = self.clone();
        updated.feature_record[Self::FASTROM_PATCH_INDEX] = Self::FASTROM_MARKER;
        updated
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

    #[test]
    fn fastrom_markers_preserve_the_irreversible_history_contract() {
        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let metadata = LunarMagicRomMetadata::from_parts(
            &attribution,
            1,
            &[0; LunarMagicRomMetadata::FEATURE_LEN],
        )
        .unwrap();
        assert!(!metadata.use_fastrom_addressing());
        assert!(!metadata.fastrom_ever_enabled());
        assert!(!metadata.fastrom_speed_patch_applied());

        let enabled = metadata.with_use_fastrom_addressing(true);
        assert!(enabled.use_fastrom_addressing());
        assert!(enabled.fastrom_ever_enabled());
        let disabled = enabled.with_use_fastrom_addressing(false);
        assert!(!disabled.use_fastrom_addressing());
        assert!(disabled.fastrom_ever_enabled());
        let patched = disabled.with_fastrom_speed_patch_applied();
        assert!(patched.fastrom_speed_patch_applied());
        assert_eq!(patched.feature_record()[6..9], [0, b'B', b'B']);
    }

    #[test]
    fn sa1_ram_remap_toggles_only_packed_feature_bit_seventeen() {
        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let mut feature = [0x5a; LunarMagicRomMetadata::FEATURE_LEN];
        feature[LunarMagicRomMetadata::FEATURE_LEN - 1] &= 0x0f;
        let metadata = LunarMagicRomMetadata::from_parts(&attribution, 7, &feature).unwrap();
        let enabled = metadata.with_sa1_ram_remap(true);
        assert!(enabled.sa1_ram_remap());
        assert_eq!(enabled.feature_bits(), metadata.feature_bits() | 1 << 17);
        assert_eq!(
            &enabled.feature_record()[4..],
            &metadata.feature_record()[4..]
        );
        let disabled = enabled.with_sa1_ram_remap(false);
        assert!(!disabled.sa1_ram_remap());
        assert_eq!(
            disabled.feature_bits(),
            metadata.feature_bits() & !(1 << 17)
        );
    }
}
