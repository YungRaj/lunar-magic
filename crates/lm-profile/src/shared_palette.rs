//! Exact SMW US revision-0 shared-palette table and backend marker.

use lm_project::SharedPaletteRomLayout;
use lm_rom::Mapper;

pub const SMW_US_V1_SHARED_PALETTE_OFFSET: usize = 0x30a0;
pub const SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER_OFFSET: usize = 0x77570;
pub const SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER: u8 = 0xc2;

#[must_use]
pub const fn smw_us_v1_shared_palette_layout() -> SharedPaletteRomLayout {
    SharedPaletteRomLayout {
        mapper: Mapper::LoRom,
        table_offset: SMW_US_V1_SHARED_PALETTE_OFFSET,
        expanded_marker_offset: SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER_OFFSET,
        expanded_marker: SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::SmwPaletteBackend;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_rom_matches_the_wine_export_byte_for_byte() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let palette = project
            .load_shared_palette(smw_us_v1_shared_palette_layout())
            .unwrap();
        assert_eq!(palette.backend(), SmwPaletteBackend::Legacy);
        assert_eq!(
            palette.encode(),
            fs::read(root.join("oracle-work/lm363/pristine-us/palette/shared.pal")).unwrap()
        );
    }

    #[test]
    fn retained_installed_rom_uses_expanded_rom_ordering() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(
                fs::read(
                    root.join("oracle-work/lm363/pristine-us/palette-install-positive/after.smc"),
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let palette = project
            .load_shared_palette(smw_us_v1_shared_palette_layout())
            .unwrap();
        assert_eq!(palette.backend(), SmwPaletteBackend::Expanded);
        assert_eq!(palette.palette_bytes().len(), 0x800);
        assert_eq!(palette.auxiliary_bytes().len(), 0x10);
    }
}
