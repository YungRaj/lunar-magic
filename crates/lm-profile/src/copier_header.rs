use lm_rom::COPIER_HEADER_LEN;

/// Synthesizes the 512-byte copier header written by Lunar Magic 3.63 when it opens a
/// checksum-valid headerless ROM.
///
/// `ToggleSnesCopierHeader` and `AddCopierHeaderToRomFile` derive the first word from the
/// physical ROM size and derive bytes 2-3 from bit zero of the internal map-mode byte. The
/// remaining structured bytes are the fixed `AA BB 04` signature and zero fill.
#[must_use]
pub fn lunar_magic_copier_header(logical_len: usize, map_mode: u8) -> [u8; COPIER_HEADER_LEN] {
    let mut header = [0; COPIER_HEADER_LEN];
    let size_word = u16::try_from((logical_len >> 17) << 4)
        .expect("supported Lunar Magic ROM lengths fit the copier-header size word");
    header[..2].copy_from_slice(&size_word.to_le_bytes());
    if map_mode & 1 != 0 {
        header[2] = 0x30;
        header[3] = 0x80;
    }
    header[8] = 0xaa;
    header[9] = 0xbb;
    header[10] = 0x04;
    header
}

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
        assert_eq!(header, lunar_magic_copier_header(0x80_000, 0x20));
    }

    #[test]
    fn synthesis_matches_the_recovered_size_and_fast_mapping_fields() {
        let two_mib = lunar_magic_copier_header(0x20_0000, 0x20);
        assert_eq!(&two_mib[..4], &[0x00, 0x01, 0x00, 0x00]);
        let eight_mib_sa1 = lunar_magic_copier_header(0x80_0000, 0x23);
        assert_eq!(&eight_mib_sa1[..4], &[0x00, 0x04, 0x30, 0x80]);
        assert_eq!(&eight_mib_sa1[8..11], &[0xaa, 0xbb, 0x04]);
        assert!(eight_mib_sa1[11..].iter().all(|byte| *byte == 0));
    }
}
