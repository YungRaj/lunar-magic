use crate::{
    ControllerSnapshot, ExAnimationController, ExAnimationControllerError,
    ExpandedSettingsController, ExpandedSettingsControllerError, GraphicsController,
    GraphicsControllerError, LevelController, LevelControllerError, Map16Controller,
    Map16ControllerError, NativeLevelAssetsController, NativeLevelAssetsControllerError,
    OverworldController, OverworldControllerError, PaletteController, PaletteControllerError,
    RevisionProfile, RevisionProfileError,
};
use lm_graphics::{GraphicsOwnership, PaletteOwnership};
use lm_project::{
    InstalledAsset, InstalledLayoutError, LoadedNativeLevelAssets, NativeLevelAssetsLoadError,
    Project,
};
use lm_rom::{RomError, RomImage};
use std::fmt;

/// Failure while selecting and decoding a controller through an identity-bound revision profile.
#[derive(Debug)]
pub enum ProfileControllerError {
    Profile(RevisionProfileError),
    Level(LevelControllerError),
    NativeAssets(NativeLevelAssetsControllerError),
    Map16(Map16ControllerError),
    Graphics(GraphicsControllerError),
    Palette(PaletteControllerError),
    ExAnimation(ExAnimationControllerError),
    Overworld(OverworldControllerError),
    ExpandedSettingsUnavailable,
    ExpandedSettings(ExpandedSettingsControllerError),
    Installation(InstalledLayoutError),
    PointerLocator(lm_project::PointerLocatorError),
    Rom(RomError),
    PaletteUnavailable,
    ExAnimationUnavailable,
    Lfix3Detect(lm_profile::SmwUsV1Lfix3DetectError),
    Lfix3Fields(lm_project::Lfix3LevelFieldsIoError),
    VanillaPalette(lm_profile::SmwUsV1LevelPaletteError),
    ExpandedSettingsLayout(lm_profile::SmwUsV1OverworldSettingsLoadError),
}

impl fmt::Display for ProfileControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "profile-driven controller decode failed: {self:?}"
        )
    }
}

impl std::error::Error for ProfileControllerError {}

impl From<InstalledLayoutError> for ProfileControllerError {
    fn from(value: InstalledLayoutError) -> Self {
        Self::Installation(value)
    }
}

impl From<lm_project::PointerLocatorError> for ProfileControllerError {
    fn from(value: lm_project::PointerLocatorError) -> Self {
        Self::PointerLocator(value)
    }
}

impl From<RomError> for ProfileControllerError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

/// Application controller constructors contributed to the shared revision-profile model.
pub trait RevisionProfileControllers {
    /// Decodes the active level with profile-provided native metadata.
    ///
    /// # Errors
    ///
    /// Returns profile identity/validation or native level decoding errors.
    fn decode_level(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<LevelController, ProfileControllerError>;
    /// Decodes one coherent level, palette, `ExAnimation`, and optional expanded-settings snapshot.
    ///
    /// # Errors
    ///
    /// Returns profile identity, native aggregate, interpretation, or ownership errors.
    fn decode_native_level_assets(
        &self,
        snapshot: &ControllerSnapshot,
        palette_ownership: PaletteOwnership,
    ) -> Result<NativeLevelAssetsController, ProfileControllerError>;
    /// Decodes the active level's installed expanded-settings record when declared by the profile.
    ///
    /// # Errors
    ///
    /// Returns profile, missing-capability, or native record decoding errors.
    fn decode_expanded_settings(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExpandedSettingsController, ProfileControllerError>;
    /// Decodes the complete profile-declared Map16 workspace.
    ///
    /// # Errors
    ///
    /// Returns profile identity/validation or native Map16 decoding errors.
    fn decode_map16(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<Map16Controller, ProfileControllerError>;
    /// Decodes selected native graphics under an ownership map.
    ///
    /// # Errors
    ///
    /// Returns profile, native graphics, or ownership-validation errors.
    fn decode_graphics(
        &self,
        snapshot: &ControllerSnapshot,
        ownership: GraphicsOwnership,
    ) -> Result<GraphicsController, ProfileControllerError>;
    /// Decodes selected graphics for read/display use with every tile initially editable.
    ///
    /// # Errors
    ///
    /// Returns profile, native graphics, decompression, or shape failures.
    fn decode_graphics_editable(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<GraphicsController, ProfileControllerError>;
    /// Decodes a native palette under an ownership map.
    ///
    /// # Errors
    ///
    /// Returns profile, native palette, or ownership-validation errors.
    fn decode_palette(
        &self,
        snapshot: &ControllerSnapshot,
        ownership: PaletteOwnership,
    ) -> Result<PaletteController, ProfileControllerError>;
    /// Decodes native `ExAnimation` with the recovered profile mode table.
    ///
    /// # Errors
    ///
    /// Returns profile identity/validation or native animation decoding errors.
    fn decode_exanimation(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExAnimationController, ProfileControllerError>;
    /// Decodes the installed ROM-global ExAnimation domain selected by this profile.
    ///
    /// # Errors
    ///
    /// Returns profile identity/validation or installed global animation decoding errors.
    fn decode_global_exanimation(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExAnimationController, ProfileControllerError>;
    /// Decodes the complete native overworld aggregate.
    ///
    /// # Errors
    ///
    /// Returns profile, native overworld, or palette-ownership errors.
    fn decode_overworld(
        &self,
        snapshot: &ControllerSnapshot,
        slot: usize,
        palette_ownership: PaletteOwnership,
    ) -> Result<OverworldController, ProfileControllerError>;
}

impl RevisionProfileControllers for RevisionProfile {
    /// Decodes the active level using this profile's native tables and sprite-length metadata.
    ///
    /// # Errors
    ///
    /// Rejects a mismatched ROM identity before accessing any profile-provided offset, then
    /// forwards native level decoding failures.
    fn decode_level(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<LevelController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())?;
        LevelController::decode(
            snapshot,
            self.level_layout_for_rom(&image)?,
            &self.sprite_lengths,
        )
        .map_err(ProfileControllerError::Level)
    }

    fn decode_native_level_assets(
        &self,
        snapshot: &ControllerSnapshot,
        palette_ownership: PaletteOwnership,
    ) -> Result<NativeLevelAssetsController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())?;
        let palette = self
            .palette_installation
            .resolve(&image)?
            .ok_or(ProfileControllerError::PaletteUnavailable)?;
        let exanimation = self
            .exanimation_installation
            .resolve(&image)?
            .ok_or(ProfileControllerError::ExAnimationUnavailable)?
            .resolve(&image)?
            .payload;
        let level = self.level_layout_for_rom(&image)?;
        let expanded_settings = resolve_smw_us_v1_expanded_settings_layout(self, &image)?;
        let layout = lm_project::NativeLevelAssetsLayout {
            level,
            palette,
            exanimation,
            expanded_settings,
        };
        let preloaded_assets =
            load_smw_us_v1_assets_with_vanilla_fallback(self, snapshot, &image, layout)?;
        let mut controller = NativeLevelAssetsController::decode_with_preloaded_assets(
            snapshot,
            layout,
            self.layer2,
            self.exanimation_feature_installation,
            &self.sprite_lengths,
            &self.exanimation_double_size_modes,
            palette_ownership,
            preloaded_assets,
        )
        .map_err(ProfileControllerError::NativeAssets)?;
        if self.game == lm_rom::SupportedGame::SuperMarioWorld
            && self.region == lm_rom::Region::NorthAmerica
            && self.revision == 0
            && lm_profile::detect_smw_us_v1_current_lfix3_runtime(image.logical_bytes())
                .map_err(ProfileControllerError::Lfix3Detect)?
                .is_some()
        {
            let crate::EditorMode::Level(slot) = snapshot.mode else {
                unreachable!("validated native level snapshot")
            };
            let layout = lm_profile::smw_us_v1_lfix3_level_fields_layout();
            let fields = lm_project::Project::new(image)
                .load_lfix3_level_fields(usize::from(slot), layout)
                .map_err(ProfileControllerError::Lfix3Fields)?;
            controller.attach_lfix3_level_fields(fields, layout);
        }
        Ok(controller)
    }

    fn decode_expanded_settings(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExpandedSettingsController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())?;
        let layout = resolve_smw_us_v1_expanded_settings_layout(self, &image)?
            .ok_or(ProfileControllerError::ExpandedSettingsUnavailable)?;
        ExpandedSettingsController::decode(snapshot, layout)
            .map_err(ProfileControllerError::ExpandedSettings)
    }

    /// Decodes the complete Map16 workspace through this identity-bound profile.
    ///
    /// # Errors
    ///
    /// Rejects profile identity disagreement or native Map16 decoding failures.
    fn decode_map16(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<Map16Controller, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        Map16Controller::decode(snapshot, self.map16).map_err(ProfileControllerError::Map16)
    }

    /// Decodes the selected graphics file and validates its externally supplied ownership map.
    ///
    /// # Errors
    ///
    /// Rejects profile identity disagreement, native graphics failures, or ownership mismatch.
    fn decode_graphics(
        &self,
        snapshot: &ControllerSnapshot,
        ownership: GraphicsOwnership,
    ) -> Result<GraphicsController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        GraphicsController::decode(snapshot, self.graphics, ownership)
            .map_err(ProfileControllerError::Graphics)
    }

    fn decode_graphics_editable(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<GraphicsController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        GraphicsController::decode_editable(snapshot, self.graphics)
            .map_err(ProfileControllerError::Graphics)
    }

    /// Decodes the selected palette and validates its externally supplied ownership map.
    ///
    /// # Errors
    ///
    /// Rejects profile identity disagreement, native palette failures, or ownership mismatch.
    fn decode_palette(
        &self,
        snapshot: &ControllerSnapshot,
        ownership: PaletteOwnership,
    ) -> Result<PaletteController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        let layout = self
            .palette_installation
            .resolve(&RomImage::from_bytes(snapshot.rom_bytes.clone())?)?
            .ok_or(ProfileControllerError::PaletteUnavailable)?;
        PaletteController::decode(snapshot, layout, ownership)
            .map_err(ProfileControllerError::Palette)
    }

    /// Decodes the selected `ExAnimation` slot with the profile's exact recovered mode table.
    ///
    /// # Errors
    ///
    /// Rejects profile identity disagreement or native `ExAnimation` decoding failures.
    fn decode_exanimation(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExAnimationController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())?;
        let layout = self
            .exanimation_installation
            .resolve(&image)?
            .ok_or(ProfileControllerError::ExAnimationUnavailable)?
            .resolve(&image)?
            .payload;
        ExAnimationController::decode(snapshot, layout, &self.exanimation_double_size_modes)
            .map_err(ProfileControllerError::ExAnimation)
    }

    fn decode_global_exanimation(
        &self,
        snapshot: &ControllerSnapshot,
    ) -> Result<ExAnimationController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        ExAnimationController::decode_global(
            snapshot,
            self.exanimation_installation,
            &self.exanimation_double_size_modes,
        )
        .map_err(ProfileControllerError::ExAnimation)
    }

    /// Decodes all modeled overworld domains through one profile and ownership boundary.
    ///
    /// # Errors
    ///
    /// Rejects profile identity disagreement, native aggregate failures, or palette ownership
    /// mismatch.
    fn decode_overworld(
        &self,
        snapshot: &ControllerSnapshot,
        slot: usize,
        palette_ownership: PaletteOwnership,
    ) -> Result<OverworldController, ProfileControllerError> {
        validate_snapshot(self, snapshot)?;
        OverworldController::decode(
            snapshot,
            slot,
            self.overworld,
            &self.exanimation_double_size_modes,
            palette_ownership,
        )
        .map_err(ProfileControllerError::Overworld)
    }
}

fn resolve_smw_us_v1_expanded_settings_layout(
    profile: &RevisionProfile,
    image: &RomImage,
) -> Result<Option<lm_project::ExpandedLevelSettingsLayout>, ProfileControllerError> {
    if profile.game != lm_rom::SupportedGame::SuperMarioWorld
        || profile.region != lm_rom::Region::NorthAmerica
        || profile.revision != 0
        || profile.mapper != lm_rom::Mapper::LoRom
        || profile.expanded_settings != Some(lm_profile::smw_us_v1_expanded_settings_layout())
    {
        return Ok(profile.expanded_settings);
    }
    let project = Project::new(image.clone());
    lm_profile::smw_us_v1_installed_expanded_settings_layout(&project)
        .map(|installed| installed.or(profile.expanded_settings))
        .map_err(ProfileControllerError::ExpandedSettingsLayout)
}

fn load_smw_us_v1_assets_with_vanilla_fallback(
    profile: &RevisionProfile,
    snapshot: &ControllerSnapshot,
    image: &RomImage,
    layout: lm_project::NativeLevelAssetsLayout,
) -> Result<Option<LoadedNativeLevelAssets>, ProfileControllerError> {
    if profile.game != lm_rom::SupportedGame::SuperMarioWorld
        || profile.region != lm_rom::Region::NorthAmerica
        || profile.revision != 0
        || profile.mapper != lm_rom::Mapper::LoRom
    {
        return Ok(None);
    }
    let crate::EditorMode::Level(slot) = snapshot.mode else {
        return Ok(None);
    };
    let slot = usize::from(slot);
    let pointer_offset = layout
        .palette
        .pointers
        .pointer_offset(slot)
        .map_err(|error| {
            ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                NativeLevelAssetsLoadError::Palette(error.into()),
            ))
        })?;
    let palette_empty = image.read(pointer_offset, 3)? == [0, 0, 0];
    let installed_exanimation = profile
        .exanimation_installation
        .resolve(image)?
        .ok_or(ProfileControllerError::ExAnimationUnavailable)?
        .resolve(image)?;
    let exanimation_pointer_offset = installed_exanimation
        .payload
        .pointers
        .pointer_offset(slot)
        .map_err(|error| {
            ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                NativeLevelAssetsLoadError::ExAnimation(error.into()),
            ))
        })?;
    let pointer = image.read(exanimation_pointer_offset, 3)?;
    let raw_pointer =
        u32::from(pointer[0]) | (u32::from(pointer[1]) << 8) | (u32::from(pointer[2]) << 16);
    let exanimation_empty = raw_pointer & installed_exanimation.pointer_presence_mask == 0;
    if !palette_empty && !exanimation_empty {
        return Ok(None);
    }

    let project = Project::new(image.clone());
    let level = project
        .load_level_slot(slot, layout.level, &profile.sprite_lengths)
        .map_err(|error| {
            ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                NativeLevelAssetsLoadError::Level(error),
            ))
        })?;
    let palette = if palette_empty {
        let mut palette = lm_profile::compose_smw_us_v1_level_palette(
            &project,
            u16::try_from(slot).expect("SMW-US has exactly 512 level slots"),
            level.layer1.header,
            0,
        )
        .map_err(ProfileControllerError::VanillaPalette)?
        .palette;
        palette.colors.insert(1, lm_graphics::Bgr555(0));
        palette.colors.rotate_left(1);
        palette
    } else {
        project
            .load_palette(slot, layout.palette)
            .map_err(|error| {
                ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                    NativeLevelAssetsLoadError::Palette(error),
                ))
            })?
    };
    let exanimation = match project
        .load_installed_exanimation(
            slot,
            profile.exanimation_installation,
            &profile.exanimation_double_size_modes,
        )
        .map_err(|error| {
            ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                NativeLevelAssetsLoadError::ExAnimation(error),
            ))
        })? {
        InstalledAsset::Present(animation) => animation,
        InstalledAsset::SlotEmpty => lm_graphics::CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        },
        InstalledAsset::SubsystemAbsent => {
            return Err(ProfileControllerError::ExAnimationUnavailable);
        }
    };
    let expanded_settings = layout
        .expanded_settings
        .map(|settings| project.load_expanded_level_settings(slot, settings))
        .transpose()
        .map_err(|error| {
            ProfileControllerError::NativeAssets(NativeLevelAssetsControllerError::Load(
                NativeLevelAssetsLoadError::ExpandedSettings(error),
            ))
        })?;
    Ok(Some(LoadedNativeLevelAssets {
        level,
        palette,
        exanimation,
        expanded_settings,
    }))
}

fn validate_snapshot(
    profile: &RevisionProfile,
    snapshot: &ControllerSnapshot,
) -> Result<(), ProfileControllerError> {
    profile
        .validate()
        .map_err(ProfileControllerError::Profile)?;
    profile
        .ensure_identity(&snapshot.identity)
        .map_err(ProfileControllerError::Profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditorMode;
    use lm_project::{
        ExAnimationRomLayout, InstalledExAnimationRomLayout, InstalledLayout, LevelPointerTable,
    };
    use lm_rom::{Mapper, Region, RomIdentity, SnesChecksum, SupportedGame};

    fn installed_fixture(headered: bool, level: u16) -> (RevisionProfile, ControllerSnapshot) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let physical = std::fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let physical_image = RomImage::from_bytes(physical.clone()).unwrap();
        let rom_bytes = if headered {
            physical
        } else {
            physical_image.logical_bytes().to_vec()
        };
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = Mapper::LoRom;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.level.sprites = lm_profile::smw_us_v1_sprite_pointer_table(&image).unwrap();
        profile.layer2 = Some(lm_profile::smw_us_v1_layer2_layout(&image).unwrap());
        profile.palette = lm_profile::smw_us_v1_custom_palette_layout();
        profile.palette_installation = InstalledLayout::Unconditional(profile.palette);
        profile.exanimation = ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x8138b,
                entries: 0x200,
                stride: 3,
            },
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        };
        profile.exanimation_installation =
            InstalledLayout::Unconditional(InstalledExAnimationRomLayout {
                payload: profile.exanimation,
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: None,
            });
        profile.exanimation_feature_installation = InstalledLayout::Absent;
        profile.expanded_settings = Some(lm_profile::smw_us_v1_expanded_settings_layout());
        profile.map16.mapper = Mapper::LoRom;
        profile.graphics.mapper = Mapper::LoRom;
        profile.overworld.layers.mapper = Mapper::LoRom;
        profile.overworld.event_reveals.mapper = Mapper::LoRom;
        profile.overworld.endpoints.mapper = Mapper::LoRom;
        profile.overworld.messages.mapper = Mapper::LoRom;
        profile.overworld.sprites.mapper = Mapper::LoRom;
        profile.overworld.palette.mapper = Mapper::LoRom;
        profile.overworld.animation.mapper = Mapper::LoRom;
        profile.validate().unwrap();
        let snapshot = ControllerSnapshot {
            revision: 0,
            mode: EditorMode::Level(level),
            identity: lm_rom::detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes,
        };
        (profile, snapshot)
    }

    #[test]
    fn identity_mismatch_precedes_rom_and_layout_access() {
        let profile = lm_profile::test_support::profile();
        let mut snapshot = ControllerSnapshot {
            revision: 0,
            mode: EditorMode::Map16,
            identity: RomIdentity {
                game: SupportedGame::SuperMarioWorld,
                mapper: Mapper::LoRom,
                region: Region::NorthAmerica,
                revision: 0,
                map_mode: 0x20,
                cartridge_type: 2,
                internal_header_offset: 0x7fc0,
                stored_checksum: SnesChecksum {
                    complement: 0xffff,
                    checksum: 0,
                },
                computed_checksum: SnesChecksum {
                    complement: 0xffff,
                    checksum: 0,
                },
            },
            document_path: None,
            rom_bytes: Vec::new(),
        };
        assert!(matches!(
            profile.decode_map16(&snapshot),
            Err(ProfileControllerError::Profile(
                RevisionProfileError::IdentityMismatch { .. }
            ))
        ));

        snapshot.identity.mapper = Mapper::ExLoRom;
        let mut malformed = profile;
        malformed.map16.graphics.stride = 2;
        assert!(matches!(
            malformed.decode_map16(&snapshot),
            Err(ProfileControllerError::Profile(
                RevisionProfileError::InvalidPointerStride {
                    domain: "map16.graphics",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn installed_untouched_level_composes_empty_palette_and_exanimation_slots() {
        let mut expected = None;
        for headered in [true, false] {
            let (profile, snapshot) = installed_fixture(headered, 0x01c);
            let controller = profile
                .decode_native_level_assets(&snapshot, PaletteOwnership::editable(257))
                .unwrap();
            assert_eq!(controller.assets().palette.colors.len(), 257);
            assert!(controller.assets().exanimation.records.is_empty());
            assert!(controller.layer2().is_some());
            let state = (
                controller.assets().clone(),
                controller.layer2().cloned(),
                controller.layer2_descriptor(),
            );
            if let Some(expected) = &expected {
                assert_eq!(&state, expected);
            } else {
                expected = Some(state);
            }
        }
    }

    #[test]
    fn canonical_profile_resolves_a_relocated_expanded_settings_table() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let collision =
            std::fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc"))
                .unwrap();
        let image = RomImage::from_bytes(collision).unwrap();
        let mut project =
            Project::new(RomImage::from_bytes(image.logical_bytes().to_vec()).unwrap());
        project
            .install_relocatable_patch(
                &lm_profile::smw_us_v1_expanded_settings_installation_plan().unwrap(),
            )
            .unwrap();
        let layout = lm_profile::smw_us_v1_installed_expanded_settings_layout(&project)
            .unwrap()
            .unwrap();
        assert_eq!(layout.table_offset, 0x09_2d08);
        let mut expected = lm_profile::smw_us_v1_default_expanded_settings_record();
        expected.set_word(9, 0x4567).unwrap();
        project
            .save_expanded_level_settings(0, &expected, layout, 0x7fdc)
            .unwrap();

        let (profile, mut snapshot) = installed_fixture(false, 0);
        snapshot.rom_bytes = project.save_snapshot();
        snapshot.identity = lm_rom::detect_identity(&project.rom).unwrap();
        let controller = profile.decode_expanded_settings(&snapshot).unwrap();
        assert_eq!(controller.record(), &expected);
    }
}
