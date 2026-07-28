/// Complete 32-entry mode flag table consumed by `LoadLevelModeConfiguration`.
const LEVEL_MODE_FLAGS: [u8; 32] = [
    0x00, 0x00, 0x80, 0x01, 0x81, 0x02, 0x82, 0x03, 0x83, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
];

/// Complete per-mode Layer 2 render-property table loaded into `DAT_00816658`.
///
/// `RenderTransparentLevelBackgroundMap16Tile` at Lunar Magic 3.63 address `$0051D1B0` halves
/// nontransparent background pixels when bit 6 is set.
const LEVEL_MODE_LAYER2_RENDER: [u8; 32] = [
    0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x20, 0x24, 0x24, 0x20, 0x24, 0x20, 0x70, 0x70, 0x24, 0x24,
    0x20, 0xff, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x24, 0x21, 0x22,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VanillaLevelMode {
    pub index: u8,
    pub vertical: bool,
    pub alternate_layer_layout: bool,
    pub high_flag: bool,
    pub background_half_color: bool,
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
        background_half_color: LEVEL_MODE_LAYER2_RENDER[bounded] & 0x40 != 0,
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
        assert!(smw_us_v1_level_mode(0x0c).background_half_color);
        assert!(smw_us_v1_level_mode(0x0d).background_half_color);
        assert!(!smw_us_v1_level_mode(0x0b).background_half_color);
        assert_eq!(smw_us_v1_level_mode(0x23), smw_us_v1_level_mode(3));
    }
}
