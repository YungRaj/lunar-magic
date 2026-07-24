use crate::{Observation, ObservationError, sha256_hex};
use lm_rom::LunarMagicRomMetadata;

/// Produces deterministic evidence for Lunar Magic's fixed ROM metadata.
///
/// The attribution remains lossless and is represented by a hash because its trailing bytes
/// include opaque version-specific text. The recovered packed fields are exposed separately.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_lunar_magic_rom_metadata(
    metadata: &LunarMagicRomMetadata,
) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    observation.insert(
        "lunar-magic/metadata/attribution-sha256",
        sha256_hex(metadata.attribution()),
    )?;
    observation.insert(
        "lunar-magic/metadata/vram-version",
        metadata.vram_version().to_string(),
    )?;
    observation.insert(
        "lunar-magic/metadata/feature-bits",
        format!("{:08x}", metadata.feature_bits()),
    )?;
    observation.insert(
        "lunar-magic/metadata/compression-configuration",
        format!("{:02x}", metadata.compression_configuration()),
    )?;
    observation.insert(
        "lunar-magic/metadata/mapping-configuration",
        format!("{:02x}", metadata.mapping_configuration()),
    )?;
    for (index, marker) in metadata.feature_record()[6..9].iter().enumerate() {
        observation.insert(
            format!("lunar-magic/metadata/marker/{index}"),
            format!("{marker:02x}"),
        )?;
    }
    for index in 0..5 {
        if let Some(pointer) = metadata.runtime_pointer(index) {
            observation.insert(
                format!("lunar-magic/metadata/runtime-pointer/{index}"),
                format!("{pointer:06x}"),
            )?;
        }
    }
    observation.insert(
        "lunar-magic/metadata/checksum-status",
        format!("{:02x}", metadata.checksum_status()),
    )?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_fields_are_visible_but_physical_rom_offsets_are_not() {
        let mut attribution = [b' '; LunarMagicRomMetadata::ATTRIBUTION_LEN];
        attribution[..LunarMagicRomMetadata::SIGNATURE.len()]
            .copy_from_slice(LunarMagicRomMetadata::SIGNATURE);
        let mut feature = [0; LunarMagicRomMetadata::FEATURE_LEN];
        feature[..4].copy_from_slice(&0xfff8_0000_u32.to_le_bytes());
        feature[6..9].copy_from_slice(&[0x42, 0, 0x42]);
        feature[9..12].copy_from_slice(&[0x4e, 0x07, 0x08]);
        let metadata = LunarMagicRomMetadata::from_parts(&attribution, 1, &feature).unwrap();
        let observed = observe_lunar_magic_rom_metadata(&metadata).unwrap();
        assert_eq!(
            observed.get("lunar-magic/metadata/feature-bits"),
            Some("fff80000")
        );
        assert_eq!(
            observed.get("lunar-magic/metadata/runtime-pointer/0"),
            Some("08074e")
        );
    }
}
