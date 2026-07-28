//! Profile-derived plans for atomic native level-asset persistence.

use crate::{RevisionAllocationError, RevisionProfile};
use lm_project::{
    ExAnimationSaveOptions, LevelLayer2RomLayout, LevelLayer2SaveOptions, LevelSaveOptions,
    NativeLevelAssetsLayout, NativeLevelAssetsSaveOptions, PaletteSaveOptions,
};
use std::ops::Range;

impl RevisionProfile {
    /// Builds the native aggregate plan after resolving marker-gated subsystems against `rom`.
    ///
    /// # Errors
    ///
    /// Returns an allocation, marker, or missing-subsystem error without fabricating a layout.
    pub fn native_level_assets_save_plan_for_rom(
        &self,
        search: Range<usize>,
        rom: &lm_rom::RomImage,
        internal_header_offset: usize,
    ) -> Result<(NativeLevelAssetsLayout, NativeLevelAssetsSaveOptions), RevisionAllocationError>
    {
        let (mut layout, mut options) =
            self.native_level_assets_save_plan(search, rom.logical_len(), internal_header_offset)?;
        layout.palette = self.palette_installation.resolve(rom)?.ok_or(
            RevisionAllocationError::OptionalSubsystemUnavailable("per-level palette"),
        )?;
        layout.exanimation = self
            .exanimation_installation
            .resolve(rom)?
            .ok_or(RevisionAllocationError::OptionalSubsystemUnavailable(
                "per-level ExAnimation",
            ))?
            .resolve(rom)?
            .payload;
        let allocation = self.allocation_policy_for_rom(
            options.level.layer1_allocation.search.clone(),
            rom,
            internal_header_offset,
        )?;
        options.level.layer1_allocation = allocation.clone();
        options.level.sprite_allocation = allocation.clone();
        options.palette.allocation = allocation.clone();
        options.exanimation.allocation = allocation;
        Ok((layout, options))
    }

    /// Builds an optional revision-derived Layer 2 layout and copy-on-write save policy.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionAllocationError`] when the profile-wide allocation policy is invalid.
    pub fn level_layer2_save_plan(
        &self,
        search: Range<usize>,
        image_len: usize,
        internal_header_offset: usize,
    ) -> Result<Option<(LevelLayer2RomLayout, LevelLayer2SaveOptions)>, RevisionAllocationError>
    {
        let Some(layout) = self.layer2 else {
            return Ok(None);
        };
        let allocation = self.allocation_policy(search, image_len, internal_header_offset)?;
        Ok(Some((
            layout,
            LevelLayer2SaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )))
    }

    /// Builds the complete layout and allocation options for one grouped native level save.
    ///
    /// Every payload receives the same profile-wide protection policy. Consequently no allocation
    /// can overwrite another subsystem's pointer table, the optional expanded-settings table, or
    /// the complete SNES internal header. Previous-block ownership remains absent until a caller
    /// supplies revision-specific proof, so this plan is copy-on-write by default.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionAllocationError`] when the profile, search range, mapper boundary, or any
    /// protected metadata span is invalid for the current image.
    pub fn native_level_assets_save_plan(
        &self,
        search: Range<usize>,
        image_len: usize,
        internal_header_offset: usize,
    ) -> Result<(NativeLevelAssetsLayout, NativeLevelAssetsSaveOptions), RevisionAllocationError>
    {
        let allocation = self.allocation_policy(search, image_len, internal_header_offset)?;
        let layout = NativeLevelAssetsLayout {
            level: self.level,
            palette: self.palette,
            exanimation: self.exanimation,
            expanded_settings: self.expanded_settings,
        };
        let options = NativeLevelAssetsSaveOptions {
            level: LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation.clone(),
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            palette: PaletteSaveOptions {
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            exanimation: ExAnimationSaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        };
        Ok((layout, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rats::ProtectedRange;
    use lm_rom::pc_to_snes;

    fn write_u24(rom: &mut lm_rom::RomImage, offset: usize, value: u32) {
        let bytes = value.to_le_bytes();
        rom.write(offset, &bytes[..3]).unwrap();
    }

    #[test]
    fn plan_uses_profile_layout_and_one_complete_protection_policy() {
        let profile = crate::test_support::profile();
        let (layout, options) = profile
            .native_level_assets_save_plan(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap();
        assert_eq!(layout.level, profile.level);
        assert_eq!(layout.palette, profile.palette);
        assert_eq!(layout.exanimation, profile.exanimation);
        assert_eq!(layout.expanded_settings, profile.expanded_settings);

        let expected = &options.level.layer1_allocation;
        assert_eq!(&options.level.sprite_allocation, expected);
        assert_eq!(&options.palette.allocation, expected);
        assert_eq!(&options.exanimation.allocation, expected);
        assert!(expected.protected.contains(&ProtectedRange(0x7fc0..0x8000)));
        let expanded = profile.expanded_settings.unwrap();
        let expanded_end = expanded.table_offset
            + (expanded.entries - 1) * expanded.stride
            + lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN;
        assert!(
            expected
                .protected
                .contains(&ProtectedRange(expanded.table_offset..expanded_end))
        );
    }

    #[test]
    fn plan_rejects_an_out_of_image_search_before_constructing_options() {
        let profile = crate::test_support::profile();
        assert!(matches!(
            profile.native_level_assets_save_plan(0x2000..0x4000, 0x3000, 0x7fc0),
            Err(RevisionAllocationError::InvalidSearchRange { .. })
        ));
    }

    #[test]
    fn layer2_plan_is_explicitly_optional_and_uses_complete_protection() {
        let mut profile = crate::test_support::profile();
        assert!(
            profile
                .level_layer2_save_plan(0x6000..0x7000, 0x3_0000, 0x7fc0)
                .unwrap()
                .is_none()
        );
        let pointers = lm_project::LevelPointerTable {
            offset: 0x2_9000,
            entries: 0x200,
            stride: 3,
        };
        profile.layer2 = Some(LevelLayer2RomLayout {
            mapper: profile.mapper,
            pointers,
            background_bank_substitution: None,
            legacy_pointer_redirect: None,
            descriptor_table: None,
            maximum_compressed_len: 0x8000,
            tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::SplitPlanes,
        });
        let (layout, options) = profile
            .level_layer2_save_plan(0x6000..0x7000, 0x3_0000, 0x7fc0)
            .unwrap()
            .unwrap();
        assert_eq!(layout, profile.layer2.unwrap());
        assert!(options.allocation.protected.contains(&ProtectedRange(
            pointers.offset..pointers.offset + pointers.entries * pointers.stride
        )));
    }

    #[test]
    fn rom_aware_native_plan_refuses_absent_optional_subsystems() {
        let mut profile = crate::test_support::profile();
        let rom = lm_rom::RomImage::from_bytes(vec![0xff; 0x3_0000]).unwrap();
        profile.palette_installation = lm_project::InstalledLayout::Absent;
        assert!(matches!(
            profile.native_level_assets_save_plan_for_rom(0x6000..0x7000, &rom, 0x7fc0),
            Err(RevisionAllocationError::OptionalSubsystemUnavailable(
                "per-level palette"
            ))
        ));
        profile.palette_installation = lm_project::InstalledLayout::Unconditional(profile.palette);
        profile.exanimation_installation = lm_project::InstalledLayout::Absent;
        assert!(matches!(
            profile.native_level_assets_save_plan_for_rom(0x6000..0x7000, &rom, 0x7fc0),
            Err(RevisionAllocationError::OptionalSubsystemUnavailable(
                "per-level ExAnimation"
            ))
        ));
    }

    #[test]
    fn rom_aware_plan_resolves_and_protects_dynamic_exanimation_table() {
        let mut profile = crate::test_support::profile();
        let marker = 0x2_8810;
        let first_operand = marker + 1;
        let runtime_target = 0x2_f000;
        let final_operand = runtime_target - 0x20;
        let dynamic_table = 0x2_a000;
        profile.exanimation_installation = lm_project::InstalledLayout::Alternatives {
            primary: lm_project::GatedLayout {
                marker: lm_project::InstallationMarker {
                    offset: marker,
                    expected: 0x22,
                },
                layout: lm_project::InstalledExAnimationRomLayout {
                    payload: profile.exanimation,
                    pointer_presence_mask: 0x00ff_ff00,
                    pointer_locator: Some(lm_project::ChainedSnesPointerLocator {
                        mapper: profile.mapper,
                        first_operand_offset: first_operand,
                        final_operand_displacement: -0x20,
                    }),
                },
            },
            fallback: None,
        };
        let mut rom = lm_rom::RomImage::from_bytes(vec![0xff; 0x3_0000]).unwrap();
        rom.write(marker, &[0x22]).unwrap();
        write_u24(
            &mut rom,
            first_operand,
            pc_to_snes(profile.mapper, runtime_target).unwrap(),
        );
        write_u24(
            &mut rom,
            final_operand,
            pc_to_snes(profile.mapper, dynamic_table).unwrap(),
        );

        let (layout, options) = profile
            .native_level_assets_save_plan_for_rom(0x6000..0x7000, &rom, 0x7fc0)
            .unwrap();

        assert_eq!(layout.exanimation.pointers.offset, dynamic_table);
        let protected = &options.exanimation.allocation.protected;
        assert!(protected.contains(&ProtectedRange(first_operand..first_operand + 3)));
        assert!(protected.contains(&ProtectedRange(final_operand..final_operand + 3)));
        assert!(protected.contains(&ProtectedRange(dynamic_table..dynamic_table + 0x200 * 3)));
    }
}
