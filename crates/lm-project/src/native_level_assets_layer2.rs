//! Atomic aggregate boundary that adds native Layer 2 to the established per-level asset group.

use crate::exanimation_io::exanimation_save_request;
use crate::expanded_settings_io::expanded_settings_write;
use crate::level_layer2_io::level_layer2_save_request;
use crate::level_save::level_save_requests;
use crate::palette_io::palette_save_request;
use crate::{
    ExpandedLevelSettingsIoError, LevelLayer2IoError, LevelLayer2RomLayout, LevelLayer2SaveOptions,
    LevelSaveError, LoadedNativeLevelAssets, NativeLevelAssets, NativeLevelAssetsLayout,
    NativeLevelAssetsLoadError, NativeLevelAssetsSaveOptions, PaletteIoError, PayloadSaveError,
    PayloadSaveResult, Project, SavedNativeLevelAssets,
};
use lm_level::{NativeLayer2Data, SpriteLengthTable};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsLayer2Layout {
    pub core: NativeLevelAssetsLayout,
    pub layer2: LevelLayer2RomLayout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelAssetsLayer2SaveOptions {
    pub core: NativeLevelAssetsSaveOptions,
    pub layer2: LevelLayer2SaveOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedNativeLevelAssetsLayer2 {
    pub core: LoadedNativeLevelAssets,
    pub layer2: NativeLayer2Data,
}

#[derive(Clone, Copy, Debug)]
pub struct NativeLevelAssetsLayer2<'a> {
    pub core: NativeLevelAssets<'a>,
    pub layer2: &'a NativeLayer2Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedNativeLevelAssetsLayer2 {
    pub core: SavedNativeLevelAssets,
    pub layer2: PayloadSaveResult,
}

impl LoadedNativeLevelAssetsLayer2 {
    #[must_use]
    pub fn as_save_assets(&self) -> NativeLevelAssetsLayer2<'_> {
        NativeLevelAssetsLayer2 {
            core: self.core.as_save_assets(),
            layer2: &self.layer2,
        }
    }
}

impl Project {
    /// Loads the established native asset aggregate and native Layer 2 through one coherent layout.
    ///
    /// # Errors
    ///
    /// Returns a typed core or Layer 2 decoding error without mutating the project.
    pub fn load_native_level_assets_with_layer2(
        &self,
        slot: usize,
        layout: NativeLevelAssetsLayer2Layout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
    ) -> Result<LoadedNativeLevelAssetsLayer2, NativeLevelAssetsLayer2LoadError> {
        let core =
            self.load_native_level_assets(slot, layout.core, sprite_lengths, double_size_modes)?;
        let mode = core.level.layer1.header.level_mode();
        let layer2 = self.load_level_layer2(slot, mode, layout.layer2)?;
        Ok(LoadedNativeLevelAssetsLayer2 { core, layer2 })
    }

    /// Saves Layer 1, Layer 2, sprites, palette, `ExAnimation`, optional expanded settings, and the
    /// SNES checksum as one undoable transaction.
    ///
    /// # Errors
    ///
    /// Any preparation, allocation, mapping, direct-write, or checksum error leaves ROM bytes and
    /// history unchanged.
    pub fn save_native_level_assets_with_layer2(
        &mut self,
        assets: NativeLevelAssetsLayer2<'_>,
        layout: NativeLevelAssetsLayer2Layout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        checksum_field: usize,
        options: &NativeLevelAssetsLayer2SaveOptions,
    ) -> Result<SavedNativeLevelAssetsLayer2, NativeLevelAssetsLayer2SaveError> {
        let [layer1, sprites] = level_save_requests(
            layout.core.level,
            assets.core.level,
            sprite_lengths,
            &options.core.level,
        )?;
        let layer2 = level_layer2_save_request(
            assets.core.level.number,
            assets.core.level.layer1.header.level_mode(),
            assets.layer2,
            layout.layer2,
            &options.layer2,
        )?;
        let palette = palette_save_request(
            assets.core.level.number,
            assets.core.palette,
            layout.core.palette,
            &options.core.palette,
        )?;
        let exanimation = exanimation_save_request(
            assets.core.level.number,
            assets.core.exanimation,
            layout.core.exanimation,
            double_size_modes,
            &options.core.exanimation,
        )?;
        let expanded_write = match (assets.core.expanded_settings, layout.core.expanded_settings) {
            (None, None) => None,
            (Some(record), Some(settings_layout)) => Some(expanded_settings_write(
                self,
                assets.core.level.number,
                record,
                settings_layout,
            )?),
            _ => return Err(NativeLevelAssetsLayer2SaveError::ExpandedSettingsPairMismatch),
        };
        let requests = [layer1, sprites, layer2, palette, exanimation];
        let mut saved = self.save_tagged_payloads_with_checksum_and_writes(
            format!(
                "save native level assets with layer 2 {:03x}",
                assets.core.level.number
            ),
            &requests,
            expanded_write.as_slice(),
            checksum_field,
        )?;
        Ok(SavedNativeLevelAssetsLayer2 {
            core: SavedNativeLevelAssets {
                layer1: saved.remove(0),
                sprites: saved.remove(0),
                palette: saved.remove(1),
                exanimation: saved.remove(1),
                expanded_settings_saved: expanded_write.is_some(),
            },
            layer2: saved.remove(0),
        })
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsLayer2LoadError {
    Core(NativeLevelAssetsLoadError),
    Layer2(LevelLayer2IoError),
}

impl fmt::Display for NativeLevelAssetsLayer2LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native level assets with Layer 2 load failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeLevelAssetsLayer2LoadError {}

impl From<NativeLevelAssetsLoadError> for NativeLevelAssetsLayer2LoadError {
    fn from(value: NativeLevelAssetsLoadError) -> Self {
        Self::Core(value)
    }
}

impl From<LevelLayer2IoError> for NativeLevelAssetsLayer2LoadError {
    fn from(value: LevelLayer2IoError) -> Self {
        Self::Layer2(value)
    }
}

#[derive(Debug)]
pub enum NativeLevelAssetsLayer2SaveError {
    Level(LevelSaveError),
    Layer2(LevelLayer2IoError),
    Palette(PaletteIoError),
    ExAnimation(crate::ExAnimationIoError),
    ExpandedSettings(ExpandedLevelSettingsIoError),
    ExpandedSettingsPairMismatch,
    Payload(PayloadSaveError),
}

impl fmt::Display for NativeLevelAssetsLayer2SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native level assets with Layer 2 save failed: {self:?}"
        )
    }
}

impl std::error::Error for NativeLevelAssetsLayer2SaveError {}

macro_rules! from_error {
    ($source:ty, $variant:ident) => {
        impl From<$source> for NativeLevelAssetsLayer2SaveError {
            fn from(value: $source) -> Self {
                Self::$variant(value)
            }
        }
    };
}

from_error!(LevelSaveError, Level);
from_error!(LevelLayer2IoError, Layer2);
from_error!(PaletteIoError, Palette);
from_error!(crate::ExAnimationIoError, ExAnimation);
from_error!(ExpandedLevelSettingsIoError, ExpandedSettings);
from_error!(PayloadSaveError, Payload);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExAnimationRomLayout, ExAnimationSaveOptions, LevelPointerTable, LevelRomLayout,
        LevelSaveOptions, LoadedLevelSlot, PaletteRomLayout, PaletteSaveOptions,
    };
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_level::{LevelObjectData, NATIVE_LAYER2_TILEMAP_LEN, NativeSpriteStream};
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::{Mapper, RomImage};

    fn table(offset: usize) -> LevelPointerTable {
        LevelPointerTable {
            offset,
            entries: 1,
            stride: 3,
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn aggregate_commits_reopens_and_undoes_layer2_with_every_core_payload() {
        let allocation = AllocationPolicy {
            search: 0x100..0x8000,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![ProtectedRange(0x20..0x63), ProtectedRange(0x7fdc..0x8000)],
        };
        let core_layout = NativeLevelAssetsLayout {
            level: LevelRomLayout {
                mapper: Mapper::LoRom,
                layer1: table(0x20),
                sprites: table(0x30).into(),
                expanded_sprites: false,
            },
            palette: PaletteRomLayout {
                mapper: Mapper::LoRom,
                pointers: table(0x40),
                colors_per_palette: 2,
            },
            exanimation: ExAnimationRomLayout {
                mapper: Mapper::LoRom,
                pointers: table(0x50),
                maximum_records: 8,
                maximum_encoded_len: 0x100,
            },
            expanded_settings: None,
        };
        let layout = NativeLevelAssetsLayer2Layout {
            core: core_layout,
            layer2: LevelLayer2RomLayout {
                mapper: Mapper::LoRom,
                pointers: table(0x60),
                maximum_compressed_len: 0x8000,
                tilemap_encoding: crate::LevelLayer2TilemapEncoding::SplitPlanes,
            },
        };
        let level = LoadedLevelSlot {
            number: 0,
            layer1: LevelObjectData::parse(&[1, 0, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0, 1, 2, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        };
        let palette = Palette {
            colors: vec![Bgr555(1), Bgr555(2)],
        };
        let animation = CompactExAnimation {
            setting: 0,
            header_value: 0,
            trigger_mask: 0,
            trigger_values: [0; 16],
            records: Vec::new(),
        };
        let layer2 = NativeLayer2Data::Tilemap(vec![0x34; NATIVE_LAYER2_TILEMAP_LEN]);
        let core_options = NativeLevelAssetsSaveOptions {
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
                allocation: allocation.clone(),
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        };
        let options = NativeLevelAssetsLayer2SaveOptions {
            core: core_options,
            layer2: LevelLayer2SaveOptions {
                allocation,
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        };
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        project
            .save_native_level_assets_with_layer2(
                NativeLevelAssetsLayer2 {
                    core: NativeLevelAssets {
                        level: &level,
                        palette: &palette,
                        exanimation: &animation,
                        expanded_settings: None,
                    },
                    layer2: &layer2,
                },
                layout,
                &SpriteLengthTable::standard(),
                &[false; 256],
                0x7fdc,
                &options,
            )
            .unwrap();
        assert_eq!(project.history.undo_len(), 1);
        let reopened = project
            .load_native_level_assets_with_layer2(
                0,
                layout,
                &SpriteLengthTable::standard(),
                &[false; 256],
            )
            .unwrap();
        assert_eq!(reopened.core.level, level);
        assert_eq!(reopened.core.palette, palette);
        assert_eq!(reopened.core.exanimation, animation);
        assert_eq!(reopened.layer2, layer2);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }
}
