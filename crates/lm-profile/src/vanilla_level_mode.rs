/// Complete 32-entry mode flag table consumed by `LoadLevelModeConfiguration`.
const LEVEL_MODE_FLAGS: [u8; 32] = [
    0x00, 0x00, 0x80, 0x01, 0x81, 0x02, 0x82, 0x03, 0x83, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VanillaLevelMode {
    pub index: u8,
    pub vertical: bool,
    pub alternate_layer_layout: bool,
    pub high_flag: bool,
}

/// Decodes the recovered level-mode property flags used by Lunar Magic 3.63.
#[must_use]
pub const fn smw_us_v1_level_mode(index: u8) -> VanillaLevelMode {
    let bounded = (index & 0x1f) as usize;
    let flags = LEVEL_MODE_FLAGS[bounded];
    VanillaLevelMode {
        index: index & 0x1f,
        vertical: flags & 1 != 0,
        alternate_layer_layout: flags & 2 != 0,
        high_flag: flags & 0x80 != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_orientation_and_auxiliary_flags_cover_every_mode() {
        let vertical = (0..32_u8)
            .filter(|mode| smw_us_v1_level_mode(*mode).vertical)
            .collect::<Vec<_>>();
        assert_eq!(vertical, [3, 4, 7, 8, 10, 13]);
        assert!(smw_us_v1_level_mode(5).alternate_layer_layout);
        assert!(smw_us_v1_level_mode(2).high_flag);
        assert_eq!(smw_us_v1_level_mode(0x23), smw_us_v1_level_mode(3));
    }
}
