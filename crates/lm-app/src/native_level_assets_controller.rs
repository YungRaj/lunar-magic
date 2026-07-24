//! Revision-bound, cross-domain native level editing and atomic commit preparation.

use crate::{
    ControllerSnapshot, EditorMode, ExAnimationControllerEdit, ExAnimationControllerEditFailure,
    NativeLevelEdit, PaletteControllerEdit,
};
use lm_graphics::{PaletteBatchEditError, PaletteOwnership};
use lm_level::{ExpandedLevelSettingsError, SpriteLengthTable};
use lm_project::{
    LevelLoadError, LevelPointerTable, LoadedNativeLevelAssets, NativeLevelAssetsLayout,
    NativeLevelAssetsLoadError, NativeLevelAssetsSaveError, PayloadLoadError, PayloadReadPolicy,
    Project, SpritePointerTable, TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

mod commit;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLevelAssetsControllerEdit {
    Level(Vec<NativeLevelEdit>),
    Palette(Vec<PaletteControllerEdit>),
    ExAnimation(Vec<ExAnimationControllerEdit>),
    ExpandedSettingsWords(Vec<(usize, u16)>),
}

#[derive(Debug)]
pub enum NativeLevelAssetsControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    SizeModeCount(usize),
    Rom(RomError),
    Load(NativeLevelAssetsLoadError),
    Layout(LevelLoadError),
    Payload(PayloadLoadError),
    LevelEdit {
        command: usize,
        error: crate::LevelControllerError,
    },
    PaletteEdit {
        command: usize,
        inner: usize,
        error: PaletteBatchEditError,
    },
    ExAnimationEdit {
        command: usize,
        inner: usize,
        error: ExAnimationControllerEditFailure,
    },
    ExpandedSettingsUnavailable {
        command: usize,
    },
    ExpandedSettingsDuplicate {
        command: usize,
        word: usize,
    },
    ExpandedSettingsEdit {
        command: usize,
        error: ExpandedLevelSettingsError,
    },
    Save(NativeLevelAssetsSaveError),
    Mutation(TransactionError),
}

impl fmt::Display for NativeLevelAssetsControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native level-assets controller failed: {self:?}")
    }
}

impl std::error::Error for NativeLevelAssetsControllerError {}

/// One coherent native level snapshot tied to an immutable application revision.
#[derive(Clone, Debug)]
pub struct NativeLevelAssetsController {
    revision: u64,
    layout: NativeLevelAssetsLayout,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    sprite_lengths: SpriteLengthTable,
    double_size_modes: [bool; 256],
    palette_ownership: PaletteOwnership,
    baseline: LoadedNativeLevelAssets,
    assets: LoadedNativeLevelAssets,
    previous_blocks: [Option<RatsBlock>; 4],
}

impl NativeLevelAssetsController {
    /// Decodes all profile-modeled native assets for the selected level.
    ///
    /// # Errors
    ///
    /// Rejects non-level modes, mapper disagreement, malformed interpretation tables, palette
    /// ownership mismatch, or any native domain decoding failure.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: NativeLevelAssetsLayout,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        palette_ownership: PaletteOwnership,
    ) -> Result<Self, NativeLevelAssetsControllerError> {
        let EditorMode::Level(slot) = snapshot.mode else {
            return Err(NativeLevelAssetsControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.level.mapper {
            return Err(NativeLevelAssetsControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.level.mapper,
            });
        }
        let modes: [bool; 256] = double_size_modes.try_into().map_err(|_| {
            NativeLevelAssetsControllerError::SizeModeCount(double_size_modes.len())
        })?;
        let image = RomImage::from_bytes(snapshot.rom_bytes.clone())
            .map_err(NativeLevelAssetsControllerError::Rom)?;
        let project = Project::new(image);
        let assets = project
            .load_native_level_assets(usize::from(slot), layout, sprite_lengths, &modes)
            .map_err(NativeLevelAssetsControllerError::Load)?;
        let slot = usize::from(slot);
        let previous_blocks = [
            snapshot_block(&project, layout.level.layer1, slot, layout.level.mapper)?,
            snapshot_sprite_block(&project, layout.level.sprites, slot, layout.level.mapper)?,
            snapshot_block(
                &project,
                layout.palette.pointers,
                slot,
                layout.palette.mapper,
            )?,
            snapshot_block(
                &project,
                layout.exanimation.pointers,
                slot,
                layout.exanimation.mapper,
            )?,
        ];
        let mut palette = assets.palette.clone();
        crate::palette_edit_batch::apply_palette_edit_batch(&mut palette, &palette_ownership, &[])
            .map_err(
                |(inner, error)| NativeLevelAssetsControllerError::PaletteEdit {
                    command: 0,
                    inner,
                    error,
                },
            )?;
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            sprite_lengths: sprite_lengths.clone(),
            double_size_modes: modes,
            palette_ownership,
            baseline: assets.clone(),
            assets,
            previous_blocks,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn assets(&self) -> &LoadedNativeLevelAssets {
        &self.assets
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.assets != self.baseline
    }

    /// Applies a mixed cross-domain edit batch to one staged aggregate.
    ///
    /// # Errors
    ///
    /// Reports both the outer and domain-local failing command and preserves the complete previous
    /// aggregate when any late edit or canonical validation fails.
    pub fn apply_edits(
        &mut self,
        edits: &[NativeLevelAssetsControllerEdit],
    ) -> Result<(), NativeLevelAssetsControllerError> {
        let mut staged = self.assets.clone();
        apply_native_level_assets_edits(
            &mut staged,
            edits,
            &self.sprite_lengths,
            self.layout.exanimation.maximum_records,
            &self.double_size_modes,
            &self.palette_ownership,
        )?;
        self.assets = staged;
        Ok(())
    }
}

fn snapshot_block(
    project: &Project,
    table: LevelPointerTable,
    slot: usize,
    mapper: Mapper,
) -> Result<Option<RatsBlock>, NativeLevelAssetsControllerError> {
    let pointer_offset = table
        .pointer_offset(slot)
        .map_err(NativeLevelAssetsControllerError::Layout)?;
    match project.load_payload(pointer_offset, mapper, &PayloadReadPolicy::Tagged) {
        Ok(payload) => Ok(payload.block),
        Err(PayloadLoadError::PointerNotTagged { .. }) => Ok(None),
        Err(error) => Err(NativeLevelAssetsControllerError::Payload(error)),
    }
}

fn snapshot_sprite_block(
    project: &Project,
    table: SpritePointerTable,
    slot: usize,
    mapper: Mapper,
) -> Result<Option<RatsBlock>, NativeLevelAssetsControllerError> {
    let pointer = table
        .read_snes_pointer(&project.rom, slot)
        .map_err(NativeLevelAssetsControllerError::Layout)?;
    match project.load_payload_from_pointer(pointer, mapper, &PayloadReadPolicy::Tagged) {
        Ok(payload) => Ok(payload.block),
        Err(PayloadLoadError::PointerNotTagged { .. }) => Ok(None),
        Err(error) => Err(NativeLevelAssetsControllerError::Payload(error)),
    }
}

pub(crate) fn apply_native_level_assets_edits(
    staged: &mut LoadedNativeLevelAssets,
    edits: &[NativeLevelAssetsControllerEdit],
    sprite_lengths: &SpriteLengthTable,
    maximum_animation_records: usize,
    double_size_modes: &[bool; 256],
    palette_ownership: &PaletteOwnership,
) -> Result<(), NativeLevelAssetsControllerError> {
    let mut next = staged.clone();
    for (command, edit) in edits.iter().enumerate() {
        match edit {
            NativeLevelAssetsControllerEdit::Level(edits) => {
                crate::native_level_edit_batch::apply_loaded_level_edits(
                    &mut next.level,
                    edits,
                    sprite_lengths,
                )
                .map_err(|error| NativeLevelAssetsControllerError::LevelEdit { command, error })?;
            }
            NativeLevelAssetsControllerEdit::Palette(edits) => {
                crate::palette_edit_batch::apply_palette_edit_batch(
                    &mut next.palette,
                    palette_ownership,
                    edits,
                )
                .map_err(|(inner, error)| {
                    NativeLevelAssetsControllerError::PaletteEdit {
                        command,
                        inner,
                        error,
                    }
                })?;
            }
            NativeLevelAssetsControllerEdit::ExAnimation(edits) => {
                crate::exanimation_controller::apply_animation_edits(
                    &mut next.exanimation,
                    edits,
                    maximum_animation_records,
                    double_size_modes,
                )
                .map_err(|(inner, error)| {
                    NativeLevelAssetsControllerError::ExAnimationEdit {
                        command,
                        inner,
                        error,
                    }
                })?;
            }
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(edits) => {
                let record = next.expanded_settings.as_mut().ok_or(
                    NativeLevelAssetsControllerError::ExpandedSettingsUnavailable { command },
                )?;
                let mut seen = [false; lm_level::ExpandedLevelSettingsRecord::WORD_COUNT];
                for &(word, value) in edits {
                    if word < seen.len() && std::mem::replace(&mut seen[word], true) {
                        return Err(
                            NativeLevelAssetsControllerError::ExpandedSettingsDuplicate {
                                command,
                                word,
                            },
                        );
                    }
                    record.set_word(word, value).map_err(|error| {
                        NativeLevelAssetsControllerError::ExpandedSettingsEdit { command, error }
                    })?;
                }
            }
        }
    }
    *staged = next;
    Ok(())
}

#[cfg(test)]
#[path = "native_level_assets_controller_tests.rs"]
mod tests;
