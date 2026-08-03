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

/// Editor screen counts returned by `ConfigureLevelLayoutDimensions` at Lunar Magic 3.63
/// address `$00421690` for the ordinary (non-reduced Layer 2) configuration.
const LEVEL_MODE_EDITOR_MAJOR_SCREENS: [u8; 32] = [
    32, 16, 16, 13, 13, 14, 14, 14, 14, 0, 28, 0, 32, 28, 32, 16, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 32, 16,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct VanillaLevelMode {
    pub index: u8,
    pub vertical: bool,
    pub alternate_layer_layout: bool,
    pub high_flag: bool,
    pub background_half_color: bool,
    pub editor_major_screens: u8,
}

/// Decodes the recovered level-mode property flags used by Lunar Magic 3.63.
#[must_use]
pub const fn smw_us_v1_level_mode(index: u8) -> VanillaLevelMode {
    let bounded = (index & 0x1f) as usize;
    let flags = LEVEL_MODE_FLAGS[bounded];
    VanillaLevelMode {
        index: index & 0x1f,
        vertical: lm_level::native_level_mode_is_vertical(index),
        alternate_layer_layout: flags & 2 != 0,
        high_flag: flags & 0x80 != 0,
        background_half_color: LEVEL_MODE_LAYER2_RENDER[bounded] & 0x40 != 0,
        editor_major_screens: LEVEL_MODE_EDITOR_MAJOR_SCREENS[bounded],
    }
}

/// Returns the physical Map16-cache base used for the secondary level layer.
///
/// `ConfigureLevelLayoutDimensions` partitions the shared `$3800`-word cache at screen 16 for
/// ordinary `$1B0`-stride layouts and at screen 14 for the mixed/vertical `$200`-stride layouts.
/// The latter family comprises modes `$05`-`$08`, `$0A`, and `$0D`.
#[must_use]
pub const fn smw_us_v1_secondary_layer_cache_base_cell(index: u8) -> usize {
    match index & 0x1f {
        0x05..=0x08 | 0x0a | 0x0d => 14 * 0x200,
        _ => 16 * 0x1b0,
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
        assert_eq!(smw_us_v1_level_mode(0x03).editor_major_screens, 13);
        assert_eq!(smw_us_v1_level_mode(0x07).editor_major_screens, 14);
        assert_eq!(smw_us_v1_level_mode(0x0a).editor_major_screens, 28);
        assert_eq!(smw_us_v1_level_mode(0x0d).editor_major_screens, 28);
        assert_eq!(smw_us_v1_level_mode(0x23), smw_us_v1_level_mode(3));

        for mode in [0x05, 0x06, 0x07, 0x08, 0x0a, 0x0d] {
            assert_eq!(
                smw_us_v1_secondary_layer_cache_base_cell(mode),
                0x1c00,
                "mode {mode:02X}"
            );
        }
        for mode in [0x00, 0x01, 0x02, 0x03, 0x04, 0x0c, 0x0e, 0x0f, 0x1e, 0x1f] {
            assert_eq!(
                smw_us_v1_secondary_layer_cache_base_cell(mode),
                0x1b00,
                "mode {mode:02X}"
            );
        }
        assert_eq!(
            smw_us_v1_secondary_layer_cache_base_cell(0x25),
            smw_us_v1_secondary_layer_cache_base_cell(0x05)
        );
    }
}
