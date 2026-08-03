use crate::{
    ControllerSnapshot, ExAnimationController, ExAnimationControllerError,
    ExpandedSettingsController, ExpandedSettingsControllerError, GraphicsController,
    GraphicsControllerError, LevelController, LevelControllerError, Map16Controller,
    Map16ControllerError, NativeLevelAssetsController, NativeLevelAssetsControllerError,
    OverworldController, OverworldControllerError, PaletteController, PaletteControllerError,
    RevisionProfile, RevisionProfileError,
};
use lm_graphics::{GraphicsOwnership, PaletteOwnership};
use lm_project::InstalledLayoutError;
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
        let mut controller = NativeLevelAssetsController::decode_with_layer2_and_features(
            snapshot,
            lm_project::NativeLevelAssetsLayout {
                level,
                palette,
                exanimation,
                expanded_settings: self.expanded_settings,
            },
            self.layer2,
            self.exanimation_feature_installation,
            &self.sprite_lengths,
            &self.exanimation_double_size_modes,
            palette_ownership,
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
        let layout = self
            .expanded_settings
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
    use lm_rom::{Mapper, Region, RomIdentity, SnesChecksum, SupportedGame};

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
}
