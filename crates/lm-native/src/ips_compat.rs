use lm_rom::{CopierHeader, RomImage, detect_identity};

/// Materializes the physical image Lunar Magic uses while creating or applying an IPS patch.
///
/// Lunar Magic's IPS engine addresses a canonical headered image. A ROM that already has a
/// copier prefix contributes those exact 512 bytes; a headerless ROM receives the same synthesized
/// prefix used by `ToggleSnesCopierHeader` before the IPS operation. Callers discard that temporary
/// prefix again when the open project was headerless.
pub(crate) fn lunar_magic_ips_image(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = RomImage::from_bytes(bytes.to_vec()).map_err(|error| error.to_string())?;
    if image.copier_header() == CopierHeader::Present {
        return Ok(bytes.to_vec());
    }
    let identity = detect_identity(&image).map_err(|error| error.to_string())?;
    let header = lm_profile::lunar_magic_copier_header(image.logical_len(), identity.map_mode);
    let mut normalized = Vec::with_capacity(header.len() + image.logical_len());
    normalized.extend_from_slice(&header);
    normalized.extend_from_slice(image.logical_bytes());
    Ok(normalized)
}

/// Converts a Lunar-Magic-coordinate IPS patch into the logical-coordinate patch consumed by the
/// shared application transaction. This preserves the open project's exact copier-prefix state.
pub(crate) fn logical_patch_for_open_rom(bytes: &[u8], patch: &[u8]) -> Result<Vec<u8>, String> {
    let source = RomImage::from_bytes(bytes.to_vec()).map_err(|error| error.to_string())?;
    let normalized_source = lunar_magic_ips_image(bytes)?;
    let normalized_target =
        lm_rom::apply_ips(&normalized_source, patch).map_err(|error| error.to_string())?;
    let target = RomImage::from_bytes(normalized_target).map_err(|error| error.to_string())?;
    if target.copier_header() != CopierHeader::Present {
        return Err("the IPS result does not retain Lunar Magic's temporary copier header".into());
    }
    lm_rom::create_ips(source.logical_bytes(), target.logical_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{COPIER_HEADER_LEN, apply_ips, compute_snes_checksum, create_ips};

    fn changed_logical_rom() -> (Vec<u8>, Vec<u8>) {
        let source = crate::test_support::pristine_smw_us_rom_bytes();
        let mut target = source.clone();
        target[0x1000] = 0x42;
        let checksum = compute_snes_checksum(&target, 0x7fdc).unwrap();
        target[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        (source, target)
    }

    #[test]
    fn headerless_roms_use_lunar_magics_canonical_physical_coordinates() {
        let (source, target) = changed_logical_rom();
        let normalized_source = lunar_magic_ips_image(&source).unwrap();
        let normalized_target = lunar_magic_ips_image(&target).unwrap();
        assert_eq!(normalized_source.len(), source.len() + COPIER_HEADER_LEN);
        let patch = create_ips(&normalized_source, &normalized_target).unwrap();
        assert_eq!(&patch[5..8], &[0x00, 0x12, 0x00]);
        assert_eq!(
            apply_ips(&normalized_source, &patch).unwrap(),
            normalized_target
        );
    }

    #[test]
    fn physical_patch_conversion_preserves_header_state_and_exact_logical_target() {
        let (source, target) = changed_logical_rom();
        let physical_patch = create_ips(
            &lunar_magic_ips_image(&source).unwrap(),
            &lunar_magic_ips_image(&target).unwrap(),
        )
        .unwrap();
        let logical_patch = logical_patch_for_open_rom(&source, &physical_patch).unwrap();
        assert_eq!(apply_ips(&source, &logical_patch).unwrap(), target);

        let mut headered = lm_profile::smw_us_v1_lunar_magic_copier_header().to_vec();
        headered.extend_from_slice(&source);
        let logical_patch = logical_patch_for_open_rom(&headered, &physical_patch).unwrap();
        assert_eq!(apply_ips(&source, &logical_patch).unwrap(), target);
    }
}
