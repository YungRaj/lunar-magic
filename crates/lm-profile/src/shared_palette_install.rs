//! Identity-checked installation of Lunar Magic's expanded shared-palette runtime.

use crate::{SMW_US_V1_CHECKSUM_FIELD, SMW_US_V1_SHARED_PALETTE_OFFSET};
use lm_graphics::{SmwPaletteBackend, SmwPaletteFile};
use lm_project::{PatchWrite, RelocatablePatchPlan};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET: usize = 0x77600;

const HOOK_A_STUB: [u8; 0x10] = [
    0xa5, 0x0e, 0x8d, 0x0b, 0x01, 0x1a, 0x85, 0xfe, 0x3a, 0x0a, 0xa8, 0x6b, 0xff, 0xff, 0xff, 0xff,
];
const HOOK_B_STUB: [u8; 0x10] = [
    0x9c, 0xcd, 0x13, 0x64, 0xfe, 0x64, 0xff, 0x84, 0x76, 0x84, 0x89, 0x6b, 0xff, 0xff, 0xff, 0xff,
];
const RUNTIME: [u8; 0x60] = [
    0xc2, 0x30, 0xa5, 0xfe, 0xf0, 0x06, 0x3a, 0x20, 0x83, 0xf5, 0x64, 0xfe, 0xe2, 0x30, 0x22, 0x8a,
    0xbe, 0x05, 0x6b, 0xe2, 0x10, 0x8b, 0x4b, 0xab, 0xc2, 0x10, 0x85, 0x00, 0x0a, 0x18, 0x65, 0x00,
    0xa8, 0xb9, 0x00, 0xf6, 0x85, 0x04, 0xc8, 0xb9, 0x00, 0xf6, 0xd0, 0x02, 0xab, 0x60, 0xab, 0x85,
    0x05, 0xe2, 0x10, 0xa0, 0x00, 0x85, 0x08, 0xb7, 0x04, 0x99, 0x01, 0x07, 0xe6, 0x04, 0xe6, 0x04,
    0xa9, 0x00, 0x01, 0x18, 0x65, 0x04, 0x85, 0x07, 0xb7, 0x04, 0x99, 0x03, 0x07, 0xb7, 0x07, 0x99,
    0x03, 0x08, 0xc8, 0xc8, 0xd0, 0xf2, 0x60, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SharedPaletteInstallPlanError {
    RequiresExpandedPalette,
    ExpectedTableLength(usize),
}

impl std::fmt::Display for SharedPaletteInstallPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "shared-palette installation plan failed: {self:?}"
        )
    }
}

impl std::error::Error for SharedPaletteInstallPlanError {}

/// Builds the exact fixed-location expanded-palette installation recovered from Lunar Magic 3.63.
///
/// `expected_table` must be the current 0x810-byte ROM table snapshot. Including it as a
/// precondition makes palette replacement and runtime installation one failure-atomic edit.
///
/// # Errors
///
/// Rejects a legacy palette or an incorrectly sized table snapshot.
pub fn smw_us_v1_expanded_shared_palette_installation_plan(
    palette: &SmwPaletteFile,
    expected_table: &[u8],
) -> Result<RelocatablePatchPlan, SharedPaletteInstallPlanError> {
    if palette.backend() != SmwPaletteBackend::Expanded {
        return Err(SharedPaletteInstallPlanError::RequiresExpandedPalette);
    }
    if expected_table.len() != SmwPaletteFile::EXPANDED_FILE_LEN {
        return Err(SharedPaletteInstallPlanError::ExpectedTableLength(
            expected_table.len(),
        ));
    }
    let mut rom_palette = Vec::with_capacity(SmwPaletteFile::EXPANDED_FILE_LEN);
    rom_palette.extend_from_slice(palette.auxiliary_bytes());
    rom_palette.extend_from_slice(palette.palette_bytes());
    Ok(RelocatablePatchPlan {
        description: "install expanded shared SMW palettes".into(),
        mapper: Mapper::LoRom,
        allocation: AllocationPolicy::lorom(0..0x80000),
        checksum_field: SMW_US_V1_CHECKSUM_FIELD,
        expansion_fill: 0xff,
        payloads: Vec::new(),
        writes: vec![
            direct(
                0x2d8e2,
                &[0xa5, 0x0e, 0x0a, 0xa8],
                &[0x22, 0x50, 0xf5, 0x0e],
            ),
            direct(0x26b8, &[0x84, 0x76, 0x84, 0x89], &[0x22, 0x60, 0xf5, 0x0e]),
            direct(0x25bf, &[0x22, 0x8a, 0xbe, 0x05], &[0x22, 0x70, 0xf5, 0x0e]),
            direct(0x77550, &[0xff; 0x10], &HOOK_A_STUB),
            direct(0x77560, &[0xff; 0x10], &HOOK_B_STUB),
            direct(0x77570, &[0xff; 0x60], &RUNTIME),
            direct(
                SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET,
                &[0xff; 0x600],
                &[0x00; 0x600],
            ),
            direct(
                SMW_US_V1_SHARED_PALETTE_OFFSET,
                expected_table,
                &rom_palette,
            ),
        ],
    })
}

fn direct(offset: usize, expected: &[u8], replacement: &[u8]) -> PatchWrite {
    PatchWrite {
        offset,
        expected: expected.to_vec(),
        replacement: replacement.to_vec(),
        fixups: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smw_us_v1_shared_palette_layout;
    use lm_project::Project;
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_install_matches_recovered_wine_regions_and_undoes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let installed = RomImage::from_bytes(
            fs::read(root.join("oracle-work/lm363/pristine-us/palette-install-positive/after.smc"))
                .unwrap(),
        )
        .unwrap();
        let installed_project = Project::new(installed);
        let palette = installed_project
            .load_shared_palette(smw_us_v1_shared_palette_layout())
            .unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let expected = project
            .rom
            .read(
                SMW_US_V1_SHARED_PALETTE_OFFSET,
                SmwPaletteFile::EXPANDED_FILE_LEN,
            )
            .unwrap()
            .to_vec();
        let plan =
            smw_us_v1_expanded_shared_palette_installation_plan(&palette, &expected).unwrap();
        project.install_relocatable_patch(&plan).unwrap();

        for (offset, len) in [
            (0x2d8e2, 4),
            (0x26b8, 4),
            (0x25bf, 4),
            (0x77550, 0x20),
            (0x77570, 0x60),
        ] {
            assert_eq!(
                project.rom.read(offset, len).unwrap(),
                installed_project.rom.read(offset, len).unwrap()
            );
        }
        assert!(
            project
                .rom
                .read(SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET, 0x600)
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(
            project
                .load_shared_palette(smw_us_v1_shared_palette_layout())
                .unwrap(),
            palette
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn late_precondition_failure_is_atomic() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let expected = project
            .rom
            .read(
                SMW_US_V1_SHARED_PALETTE_OFFSET,
                SmwPaletteFile::EXPANDED_FILE_LEN,
            )
            .unwrap()
            .to_vec();
        let palette = SmwPaletteFile::expanded(vec![0x12; 0x800], vec![0x34; 0x10]).unwrap();
        let mut plan =
            smw_us_v1_expanded_shared_palette_installation_plan(&palette, &expected).unwrap();
        plan.writes[6].expected[0] = 0;
        assert!(project.install_relocatable_patch(&plan).is_err());
        assert_eq!(project.save_snapshot(), original);
        assert!(!project.history.can_undo());
    }
}
