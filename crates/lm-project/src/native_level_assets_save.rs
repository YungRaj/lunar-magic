use crate::exanimation_io::exanimation_save_request;
use crate::expanded_settings_io::expanded_settings_write;
use crate::level_save::level_save_requests;
use crate::palette_io::palette_save_request;
use crate::{
    ExAnimationIoError, ExAnimationRomLayout, ExAnimationSaveOptions, ExpandedLevelSettingsIoError,
    ExpandedLevelSettingsLayout, LevelRomLayout, LevelSaveError, LevelSaveOptions, LoadedLevelSlot,
    PaletteIoError, PaletteRomLayout, PaletteSaveOptions, PayloadReclamation, PayloadSaveError,
    PayloadSaveResult, Project,
};
use lm_graphics::{CompactExAnimation, Palette};
use lm_level::{ExpandedLevelSettingsRecord, SpriteLengthTable};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsLayout {
    pub level: LevelRomLayout,
    pub palette: PaletteRomLayout,
    pub exanimation: ExAnimationRomLayout,
    pub expanded_settings: Option<ExpandedLevelSettingsLayout>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsSaveOptions {
    pub level: LevelSaveOptions,
    pub palette: PaletteSaveOptions,
    pub exanimation: ExAnimationSaveOptions,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeLevelAssets<'a> {
    pub level: &'a LoadedLevelSlot,
    pub palette: &'a Palette,
    pub exanimation: &'a CompactExAnimation,
    pub expanded_settings: Option<&'a ExpandedLevelSettingsRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedNativeLevelAssets {
    pub layer1: PayloadSaveResult,
    pub sprites: PayloadSaveResult,
    pub palette: PayloadSaveResult,
    pub exanimation: PayloadSaveResult,
    pub expanded_settings_saved: bool,
}

#[derive(Clone, Copy)]
enum NativeAssetsCommit<'a> {
    WithChecksum(usize),
    WithReclamation(PayloadReclamation<'a>),
}

impl Project {
    /// Saves every currently modeled native per-level payload and optional installed settings
    /// record as one transaction.
    ///
    /// Serialization and canonical reopen checks for all four tagged payloads finish before
    /// allocation. Allocation, pointer writes, the protected direct-table write, ROM growth,
    /// checksum repair, and history publication then occur once for the complete group.
    ///
    /// # Errors
    ///
    /// Returns a domain-specific validation error or a grouped payload error. Any failure leaves
    /// ROM bytes, logical length, and undo history unchanged.
    pub fn save_native_level_assets(
        &mut self,
        assets: NativeLevelAssets<'_>,
        layout: NativeLevelAssetsLayout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        checksum_field: usize,
        options: &NativeLevelAssetsSaveOptions,
    ) -> Result<SavedNativeLevelAssets, NativeLevelAssetsSaveError> {
        self.save_native_level_assets_group(
            assets,
            layout,
            sprite_lengths,
            double_size_modes,
            options,
            NativeAssetsCommit::WithChecksum(checksum_field),
        )
    }

    /// Saves the aggregate, reclaims exactly owned displaced tagged payloads, performs the optional
    /// protected settings-table write, and repairs the checksum in one undoable transaction.
    ///
    /// # Errors
    ///
    /// Returns [`NativeLevelAssetsSaveError`] for any domain, ownership, allocation, direct-write,
    /// mapping, or checksum failure without partial ROM or history mutation.
    pub fn save_native_level_assets_with_reclamation(
        &mut self,
        assets: NativeLevelAssets<'_>,
        layout: NativeLevelAssetsLayout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        options: &NativeLevelAssetsSaveOptions,
        reclamation: PayloadReclamation<'_>,
    ) -> Result<SavedNativeLevelAssets, NativeLevelAssetsSaveError> {
        self.save_native_level_assets_group(
            assets,
            layout,
            sprite_lengths,
            double_size_modes,
            options,
            NativeAssetsCommit::WithReclamation(reclamation),
        )
    }

    fn save_native_level_assets_group(
        &mut self,
        assets: NativeLevelAssets<'_>,
        layout: NativeLevelAssetsLayout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        options: &NativeLevelAssetsSaveOptions,
        commit: NativeAssetsCommit<'_>,
    ) -> Result<SavedNativeLevelAssets, NativeLevelAssetsSaveError> {
        let [layer1, sprites] =
            level_save_requests(layout.level, assets.level, sprite_lengths, &options.level)?;
        let palette = palette_save_request(
            assets.level.number,
            assets.palette,
            layout.palette,
            &options.palette,
        )?;
        let exanimation = exanimation_save_request(
            assets.level.number,
            assets.exanimation,
            layout.exanimation,
            double_size_modes,
            &options.exanimation,
        )?;
        let requests = [layer1, sprites, palette, exanimation];
        let expanded_write = match (assets.expanded_settings, layout.expanded_settings) {
            (None, None) => None,
            (Some(record), Some(layout)) => Some(expanded_settings_write(
                self,
                assets.level.number,
                record,
                layout,
            )?),
            _ => return Err(NativeLevelAssetsSaveError::ExpandedSettingsPairMismatch),
        };
        let writes = expanded_write.as_slice();
        let description = format!("save native level assets {:03x}", assets.level.number);
        let mut saved = match commit {
            NativeAssetsCommit::WithReclamation(reclamation) => self
                .save_tagged_payloads_with_checksum_writes_and_reclamation(
                    description,
                    &requests,
                    writes,
                    reclamation.checksum_field,
                    reclamation.manifest,
                )?,
            NativeAssetsCommit::WithChecksum(checksum_field) => self
                .save_tagged_payloads_with_checksum_and_writes(
                    description,
                    &requests,
                    writes,
                    checksum_field,
                )?,
        };
        Ok(SavedNativeLevelAssets {
            layer1: saved.remove(0),
            sprites: saved.remove(0),
            palette: saved.remove(0),
            exanimation: saved.remove(0),
            expanded_settings_saved: expanded_write.is_some(),
        })
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsSaveError {
    Level(LevelSaveError),
    Palette(PaletteIoError),
    ExAnimation(ExAnimationIoError),
    Payload(PayloadSaveError),
    ExpandedSettings(ExpandedLevelSettingsIoError),
    ExpandedSettingsPairMismatch,
}

impl fmt::Display for NativeLevelAssetsSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native level asset save failed: {self:?}")
    }
}

impl std::error::Error for NativeLevelAssetsSaveError {}

impl From<LevelSaveError> for NativeLevelAssetsSaveError {
    fn from(value: LevelSaveError) -> Self {
        Self::Level(value)
    }
}

impl From<PaletteIoError> for NativeLevelAssetsSaveError {
    fn from(value: PaletteIoError) -> Self {
        Self::Palette(value)
    }
}

impl From<ExAnimationIoError> for NativeLevelAssetsSaveError {
    fn from(value: ExAnimationIoError) -> Self {
        Self::ExAnimation(value)
    }
}

impl From<PayloadSaveError> for NativeLevelAssetsSaveError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Payload(value)
    }
}

impl From<ExpandedLevelSettingsIoError> for NativeLevelAssetsSaveError {
    fn from(value: ExpandedLevelSettingsIoError) -> Self {
        Self::ExpandedSettings(value)
    }
}

#[cfg(test)]
#[path = "native_level_assets_save_tests.rs"]
mod tests;
