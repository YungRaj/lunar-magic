use lm_rom::COPIER_HEADER_LEN;

/// Returns the exact copier header Lunar Magic 3.63 creates when it first opens a headerless,
/// pristine SMW-US revision-0 image.
#[must_use]
pub const fn smw_us_v1_lunar_magic_copier_header() -> [u8; COPIER_HEADER_LEN] {
    let mut header = [0; COPIER_HEADER_LEN];
    header[0] = 0x40;
    header[8] = 0xaa;
    header[9] = 0xbb;
    header[10] = 0x04;
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_header_matches_the_complete_retained_lunar_magic_oracle() {
        let header = smw_us_v1_lunar_magic_copier_header();
        assert_eq!(header.len(), COPIER_HEADER_LEN);
        assert_eq!(
            &header[..12],
            &[0x40, 0, 0, 0, 0, 0, 0, 0, 0xaa, 0xbb, 0x04, 0]
        );
        assert!(header[12..].iter().all(|byte| *byte == 0));
    }
}
