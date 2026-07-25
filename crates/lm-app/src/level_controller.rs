use crate::{ControllerSnapshot, EditorMode};
use lm_level::{
    HeaderValueError, LegacyHeaderEdit, LevelEditError, ObjectEdit, ObjectEditError,
    ObjectStreamError, SpriteToken,
};
use lm_project::{
    LevelLoadError, LevelRomLayout, LevelSaveError, LoadedLevelSlot, PayloadReadPolicy, Project,
    TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

mod commit;

/// One ordered mutation of the two native per-level streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLevelEdit {
    LegacyHeader(LegacyHeaderEdit),
    Objects(Vec<ObjectEdit>),
    SetSpriteHeader(u8),
    InsertSprite { index: usize, token: SpriteToken },
    ReplaceSprite { index: usize, token: SpriteToken },
    RemoveSprite { index: usize },
    MoveSpriteBefore { from: usize, before: usize },
    SortLegacySpritesByScreen { selected: usize },
}

#[derive(Debug)]
pub enum LevelControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    Rom(RomError),
    Load(LevelLoadError),
    ObjectEdit {
        command: usize,
        error: ObjectEditError,
    },
    SpriteEdit {
        command: usize,
        error: LevelEditError,
    },
    HeaderEdit {
        command: usize,
        error: HeaderValueError,
    },
    InvalidSpriteSerialization(lm_level::NativeSpriteEncodingError),
    InvalidSpriteEncoding(lm_level::SpriteStreamError),
    NonCanonicalSpriteEncoding,
    InvalidObjectEncoding(ObjectStreamError),
    NonCanonicalObjectEncoding,
    Save(LevelSaveError),
    Mutation(TransactionError),
}

impl fmt::Display for LevelControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "native level controller failed: {self:?}")
    }
}

impl std::error::Error for LevelControllerError {}

/// A decoded native level tied to one immutable application snapshot.
#[derive(Clone, Debug)]
pub struct LevelController {
    revision: u64,
    layout: LevelRomLayout,
    checksum_field_offset: usize,
    source_file_bytes: Vec<u8>,
    sprite_lengths: lm_level::SpriteLengthTable,
    baseline: LoadedLevelSlot,
    level: LoadedLevelSlot,
    previous_layer1: Option<RatsBlock>,
    previous_sprites: Option<RatsBlock>,
}

impl LevelController {
    /// Decodes the level selected by a controller snapshot using explicit revision layout data.
    ///
    /// # Errors
    ///
    /// Returns [`LevelControllerError`] unless the snapshot is in level mode, the mapper agrees
    /// with its detected identity, and both native streams parse under the supplied length table.
    pub fn decode(
        snapshot: &ControllerSnapshot,
        layout: LevelRomLayout,
        sprite_lengths: &lm_level::SpriteLengthTable,
    ) -> Result<Self, LevelControllerError> {
        let EditorMode::Level(number) = snapshot.mode else {
            return Err(LevelControllerError::WrongMode(snapshot.mode));
        };
        if snapshot.identity.mapper != layout.mapper {
            return Err(LevelControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: layout.mapper,
            });
        }
        let image =
            RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(LevelControllerError::Rom)?;
        let project = Project::new(image);
        let level_number = usize::from(number);
        let previous_layer1 = project
            .load_payload(
                layout
                    .layer1
                    .pointer_offset(level_number)
                    .map_err(LevelControllerError::Load)?,
                layout.mapper,
                &PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                },
            )
            .map_err(|error| LevelControllerError::Load(error.into()))?
            .block;
        let previous_sprites = project
            .load_payload_from_pointer(
                layout
                    .sprites
                    .read_snes_pointer(&project.rom, level_number)
                    .map_err(LevelControllerError::Load)?,
                layout.mapper,
                &PayloadReadPolicy::TaggedOrBounded {
                    maximum_len: 0x8000,
                    bank_size: Some(0x8000),
                },
            )
            .map_err(|error| LevelControllerError::Load(error.into()))?
            .block;
        let level = project
            .load_level_slot(level_number, layout, sprite_lengths)
            .map_err(LevelControllerError::Load)?;
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            sprite_lengths: sprite_lengths.clone(),
            baseline: level.clone(),
            level,
            previous_layer1,
            previous_sprites,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn level(&self) -> &LoadedLevelSlot {
        &self.level
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.level != self.baseline
    }

    #[must_use]
    pub fn layer1_is_modified(&self) -> bool {
        self.level.layer1 != self.baseline.layer1
    }

    #[must_use]
    pub fn sprites_are_modified(&self) -> bool {
        self.level.sprites != self.baseline.sprites
    }

    /// Applies ordered native edits to a staged clone.
    ///
    /// # Errors
    ///
    /// Returns [`LevelControllerError`] with the failing command index. A failure leaves both the
    /// decoded model and its source snapshot unchanged.
    pub fn apply_edits(&mut self, edits: &[NativeLevelEdit]) -> Result<(), LevelControllerError> {
        crate::native_level_edit_batch::apply_loaded_level_edits(
            &mut self.level,
            edits,
            &self.sprite_lengths,
        )
    }
}

#[cfg(test)]
mod tests;
