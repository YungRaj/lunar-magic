use crate::{
    ExAnimationIoError, ExpandedLevelSettingsIoError, LevelLoadError, NativeLevelAssetsLayout,
    PaletteIoError, Project,
};
use lm_graphics::{CompactExAnimation, Palette};
use lm_level::{ExpandedLevelSettingsRecord, SpriteLengthTable};
use std::fmt;

/// One coherent decoded view of every currently modeled native per-level asset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedNativeLevelAssets {
    pub level: crate::LoadedLevelSlot,
    pub palette: Palette,
    pub exanimation: CompactExAnimation,
    pub expanded_settings: Option<ExpandedLevelSettingsRecord>,
}

impl LoadedNativeLevelAssets {
    /// Borrows this coherent snapshot in the shape accepted by the grouped native save.
    #[must_use]
    pub fn as_save_assets(&self) -> crate::NativeLevelAssets<'_> {
        crate::NativeLevelAssets {
            level: &self.level,
            palette: &self.palette,
            exanimation: &self.exanimation,
            expanded_settings: self.expanded_settings.as_ref(),
        }
    }
}

impl Project {
    /// Loads all modeled native assets for one level through one revision-derived layout.
    ///
    /// The optional installed expanded-settings table is controlled solely by the layout: a
    /// declared table produces one exact record, while an absent table produces `None`.
    ///
    /// # Errors
    ///
    /// Returns the first domain-specific decoding or bounds failure without changing the project.
    pub fn load_native_level_assets(
        &self,
        slot: usize,
        layout: NativeLevelAssetsLayout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
    ) -> Result<LoadedNativeLevelAssets, NativeLevelAssetsLoadError> {
        let level = self.load_level_slot(slot, layout.level, sprite_lengths)?;
        let palette = self.load_palette(slot, layout.palette)?;
        let exanimation = self.load_exanimation(slot, layout.exanimation, double_size_modes)?;
        let expanded_settings = layout
            .expanded_settings
            .map(|settings| self.load_expanded_level_settings(slot, settings))
            .transpose()?;
        Ok(LoadedNativeLevelAssets {
            level,
            palette,
            exanimation,
            expanded_settings,
        })
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsLoadError {
    Level(LevelLoadError),
    Palette(PaletteIoError),
    ExAnimation(ExAnimationIoError),
    ExpandedSettings(ExpandedLevelSettingsIoError),
}

impl fmt::Display for NativeLevelAssetsLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native level asset load failed: {self:?}")
    }
}

impl std::error::Error for NativeLevelAssetsLoadError {}

impl From<LevelLoadError> for NativeLevelAssetsLoadError {
    fn from(value: LevelLoadError) -> Self {
        Self::Level(value)
    }
}

impl From<PaletteIoError> for NativeLevelAssetsLoadError {
    fn from(value: PaletteIoError) -> Self {
        Self::Palette(value)
    }
}

impl From<ExAnimationIoError> for NativeLevelAssetsLoadError {
    fn from(value: ExAnimationIoError) -> Self {
        Self::ExAnimation(value)
    }
}

impl From<ExpandedLevelSettingsIoError> for NativeLevelAssetsLoadError {
    fn from(value: ExpandedLevelSettingsIoError) -> Self {
        Self::ExpandedSettings(value)
    }
}

#[cfg(test)]
#[path = "native_level_assets_load_tests.rs"]
mod tests;
