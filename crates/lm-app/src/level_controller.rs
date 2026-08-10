use crate::{ControllerSnapshot, EditorMode};
use lm_level::{
    CustomTimeError, CustomTimeSettings, HeaderValueError, LegacyHeaderEdit, LevelEditError,
    MwlLayer2Descriptor, NATIVE_LAYER2_TILEMAP_LEN, NativeLayer2Data, NativeSpriteRecordFields,
    ObjectEdit, ObjectEditError, ObjectStreamError, SpriteRecord, SpriteToken,
};
use lm_project::{
    LevelLayer2IoError, LevelLayer2RomLayout, LevelLoadError, LevelRomLayout, LevelSaveError,
    LoadedLevelSlot, PayloadReadPolicy, Project, TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

mod commit;

/// One ordered mutation of the two native per-level streams.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLevelEdit {
    LegacyHeader(LegacyHeaderEdit),
    SetCustomTime(Option<CustomTimeSettings>),
    ClearObjects,
    Objects(Vec<ObjectEdit>),
    ClearSprites,
    SetSpriteHeader(u8),
    SetSpriteHeaderProperties {
        memory: u8,
        buoyancy_1: bool,
        buoyancy_2: bool,
    },
    SetSpriteFields {
        index: usize,
        fields: NativeSpriteRecordFields,
    },
    InsertSprite {
        index: usize,
        token: SpriteToken,
    },
    ReplaceSprite {
        index: usize,
        token: SpriteToken,
    },
    RemoveSprite {
        index: usize,
    },
    MoveSpriteBefore {
        from: usize,
        before: usize,
    },
    SortLegacySpritesByScreen {
        selected: usize,
    },
    PlaceSpriteAtPosition {
        record: SpriteRecord,
        screen: u8,
        x: u8,
        y: u16,
    },
    RelocateSpritePosition {
        selected: usize,
        screen: u8,
        x: u8,
        y: u16,
    },
    DuplicateSpriteGroup {
        selected: Vec<usize>,
        major_delta: i32,
        minor_delta: i32,
    },
    RelocateSpriteGroup {
        selected: Vec<usize>,
        major_delta: i32,
        minor_delta: i32,
    },
    AdjustSpriteZOrder {
        selected: Vec<usize>,
        increase: bool,
    },
    RelocateExpandedSprite {
        selected: usize,
        screen: u8,
        x: u8,
        y: u16,
    },
}

#[derive(Debug)]
pub enum LevelControllerError {
    WrongMode(EditorMode),
    MapperMismatch {
        snapshot: Mapper,
        layout: Mapper,
    },
    Rom(RomError),
    ExpansionRebase(String),
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
    CustomTimeEdit {
        command: usize,
        error: CustomTimeError,
    },
    SpriteHeaderEdit {
        command: usize,
        error: lm_level::NativeSpriteMemoryError,
    },
    InvalidSpriteSerialization(lm_level::NativeSpriteEncodingError),
    InvalidSpriteEncoding(lm_level::SpriteStreamError),
    NonCanonicalSpriteEncoding,
    SpriteCanonicalization(LevelEditError),
    InvalidObjectEncoding(ObjectStreamError),
    NonCanonicalObjectEncoding,
    NonCanonicalLevelEncoding,
    Save(LevelSaveError),
    Mutation(TransactionError),
    Layer2Load(LevelLayer2IoError),
    Layer2Unavailable,
    Layer2StorageMismatch {
        expected: &'static str,
    },
    Layer2ModeChangeRequiresReset {
        from: u8,
        to: u8,
    },
    Layer2ObjectEdit(ObjectEditError),
    Layer2TileIndex(usize),
    Layer2TileDuplicate(usize),
    NonCanonicalLayer2Encoding,
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
    layer2_layout: Option<LevelLayer2RomLayout>,
    baseline_layer2: Option<NativeLayer2Data>,
    layer2: Option<NativeLayer2Data>,
    dormant_layer2_objects: Option<lm_level::LevelObjectData>,
    baseline_layer2_descriptor: Option<MwlLayer2Descriptor>,
    layer2_descriptor: Option<MwlLayer2Descriptor>,
    normalized_reserved_level_mode: Option<u8>,
    undo: Vec<LevelControllerState>,
    redo: Vec<LevelControllerState>,
    previous_layer1: Option<RatsBlock>,
    previous_sprites: Option<RatsBlock>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LevelControllerState {
    level: LoadedLevelSlot,
    layer2: Option<NativeLayer2Data>,
    dormant_layer2_objects: Option<lm_level::LevelObjectData>,
    layer2_descriptor: Option<MwlLayer2Descriptor>,
}

impl LevelController {
    const HISTORY_LIMIT: usize = 256;

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
        Self::decode_with_layer2(snapshot, layout, None, sprite_lengths)
    }

    /// Decodes the selected native level and, when supplied, its Layer 2 stream into one staged
    /// revision-bound controller.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::decode`] plus typed Layer 2 load failures.
    pub fn decode_with_layer2(
        snapshot: &ControllerSnapshot,
        layout: LevelRomLayout,
        layer2_layout: Option<LevelLayer2RomLayout>,
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
        let mut level = project
            .load_level_slot(level_number, layout, sprite_lengths)
            .map_err(LevelControllerError::Load)?;
        let baseline = level.clone();
        let source_level_mode = level.layer1.header.level_mode();
        let normalized_reserved_level_mode = level
            .layer1
            .header
            .canonicalize_lunar_magic_level_mode()
            .then_some(source_level_mode);
        let loaded_layer2 = layer2_layout
            .map(|layer2_layout| {
                project.load_level_layer2_with_descriptor(
                    level_number,
                    level.layer1.header.level_mode(),
                    layer2_layout,
                )
            })
            .transpose()
            .map_err(LevelControllerError::Layer2Load)?;
        let layer2 = loaded_layer2.as_ref().map(|loaded| loaded.data.clone());
        let layer2_descriptor = loaded_layer2.and_then(|loaded| loaded.descriptor);
        let dormant_layer2_objects = layer2.as_ref().map(|layer2| match layer2 {
            NativeLayer2Data::Objects(objects) => objects.clone(),
            NativeLayer2Data::Tilemap(_) => lm_level::LevelObjectData::default(),
        });
        Ok(Self {
            revision: snapshot.revision,
            layout,
            checksum_field_offset: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            sprite_lengths: sprite_lengths.clone(),
            baseline,
            level,
            layer2_layout,
            baseline_layer2: layer2.clone(),
            layer2,
            dormant_layer2_objects,
            baseline_layer2_descriptor: layer2_descriptor,
            layer2_descriptor,
            normalized_reserved_level_mode,
            undo: Vec::new(),
            redo: Vec::new(),
            previous_layer1,
            previous_sprites,
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Rebinds staged level data to a project snapshot produced solely by ROM expansion.
    ///
    /// Expansion changes the project revision and the internal ROM size/checksum fields, but it
    /// must not invalidate edits staged against the unchanged level streams. This method verifies
    /// that the new logical image is an append-only expansion (apart from those authenticated
    /// header fields) before adopting it as the commit source.
    pub fn rebase_after_rom_expansion(
        &mut self,
        snapshot: &ControllerSnapshot,
    ) -> Result<(), LevelControllerError> {
        let level = u16::try_from(self.level.number).map_err(|_| {
            LevelControllerError::ExpansionRebase("staged level number exceeds u16".into())
        })?;
        if snapshot.mode != EditorMode::Level(level) {
            return Err(LevelControllerError::WrongMode(snapshot.mode));
        }
        if snapshot.identity.mapper != self.layout.mapper {
            return Err(LevelControllerError::MapperMismatch {
                snapshot: snapshot.identity.mapper,
                layout: self.layout.mapper,
            });
        }
        let old = RomImage::from_bytes(self.source_file_bytes.clone())
            .map_err(LevelControllerError::Rom)?;
        let new =
            RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(LevelControllerError::Rom)?;
        if new.logical_len() <= old.logical_len() {
            return Err(LevelControllerError::ExpansionRebase(
                "replacement snapshot is not larger than the staged source ROM".into(),
            ));
        }
        let header = snapshot.identity.internal_header_offset;
        let mutable_header = [
            header + 0x17,
            header + 0x1c,
            header + 0x1d,
            header + 0x1e,
            header + 0x1f,
        ];
        let unchanged = old
            .logical_bytes()
            .iter()
            .zip(new.logical_bytes())
            .enumerate()
            .all(|(offset, (before, after))| before == after || mutable_header.contains(&offset));
        if !unchanged {
            return Err(LevelControllerError::ExpansionRebase(
                "expanded snapshot changed bytes outside the ROM size/checksum header fields"
                    .into(),
            ));
        }
        self.source_file_bytes.clone_from(&snapshot.rom_bytes);
        self.revision = snapshot.revision;
        Ok(())
    }

    #[must_use]
    pub const fn level(&self) -> &LoadedLevelSlot {
        &self.level
    }

    #[must_use]
    pub const fn sprite_lengths(&self) -> &lm_level::SpriteLengthTable {
        &self.sprite_lengths
    }

    #[must_use]
    pub const fn layer2(&self) -> Option<&NativeLayer2Data> {
        self.layer2.as_ref()
    }

    /// Returns the reserved source mode that Lunar Magic compatibility normalized to mode `$00`.
    #[must_use]
    pub const fn normalized_reserved_level_mode(&self) -> Option<u8> {
        self.normalized_reserved_level_mode
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.level != self.baseline
            || self.layer2 != self.baseline_layer2
            || self.layer2_descriptor != self.baseline_layer2_descriptor
    }

    #[must_use]
    pub fn layer1_is_modified(&self) -> bool {
        self.level.layer1 != self.baseline.layer1
    }

    #[must_use]
    pub fn sprites_are_modified(&self) -> bool {
        self.level.sprites != self.baseline.sprites
    }

    #[must_use]
    pub fn layer2_is_modified(&self) -> bool {
        self.layer2 != self.baseline_layer2
    }

    /// Returns the canonical original and currently staged native sprite-stream lengths.
    ///
    /// Frontends use this to explain whether a pristine shared-bank save can remain in place or
    /// requires the copy-on-write relocation path. The result is derived with the controller's
    /// exact SSC-aware record-length table rather than assuming three-byte records.
    ///
    /// # Errors
    ///
    /// Returns a typed canonicalization or serializer error if either snapshot is not
    /// representable under the controller's bound orientation and record-length interpretation.
    pub fn sprite_encoded_lengths(&self) -> Result<(usize, usize), LevelControllerError> {
        let vertical = self.level.layer1.header.is_vertical();
        let mut baseline = self.baseline.sprites.clone();
        baseline
            .canonicalize_for_orientation(vertical)
            .map_err(LevelControllerError::SpriteCanonicalization)?;
        let mut staged = self.level.sprites.clone();
        staged
            .canonicalize_for_orientation(vertical)
            .map_err(LevelControllerError::SpriteCanonicalization)?;
        Ok((
            baseline
                .encode_for_table(&self.sprite_lengths)
                .map_err(LevelControllerError::InvalidSpriteSerialization)?
                .len(),
            staged
                .encode_for_table(&self.sprite_lengths)
                .map_err(LevelControllerError::InvalidSpriteSerialization)?
                .len(),
        ))
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Restores the previous staged native level without touching the ROM snapshot.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = self.state();
        self.restore(previous);
        push_bounded(&mut self.redo, current);
        true
    }

    /// Reapplies the next staged native level without touching the ROM snapshot.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.state();
        self.restore(next);
        push_bounded(&mut self.undo, current);
        true
    }

    /// Applies ordered native edits to a staged clone.
    ///
    /// # Errors
    ///
    /// Returns [`LevelControllerError`] with the failing command index. A failure leaves both the
    /// decoded model and its source snapshot unchanged.
    pub fn apply_edits(&mut self, edits: &[NativeLevelEdit]) -> Result<(), LevelControllerError> {
        self.apply_edits_with_layer2_reset(edits, false)
    }

    /// Applies ordered native edits and explicitly authorizes Lunar Magic's destructive Layer 2
    /// reset when the final level mode crosses its object/tilemap storage boundary.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::apply_edits`]. Without `reset_layer2`, a storage-class
    /// transition is rejected atomically instead of leaving a payload that can only fail at save.
    pub fn apply_edits_with_layer2_reset(
        &mut self,
        edits: &[NativeLevelEdit],
        reset_layer2: bool,
    ) -> Result<(), LevelControllerError> {
        let previous = self.state();
        let mut next = previous.clone();
        crate::native_level_edit_batch::apply_loaded_level_edits(
            &mut next.level,
            edits,
            &self.sprite_lengths,
        )?;
        reset_layer2_after_mode_change(
            &mut next.layer2,
            &mut next.dormant_layer2_objects,
            &mut next.layer2_descriptor,
            previous.level.layer1.header.level_mode(),
            next.level.layer1.header.level_mode(),
            reset_layer2,
        )?;
        self.restore(next);
        if let Some(mode) = last_normalized_mode_edit(edits) {
            self.normalized_reserved_level_mode = Some(mode);
        }
        if self.state() != previous {
            push_bounded(&mut self.undo, previous);
            self.redo.clear();
        }
        Ok(())
    }

    /// Applies ordered Layer 2 object edits to the same history used by Layer 1 and sprites.
    ///
    /// # Errors
    ///
    /// Rejects unavailable or tilemap-backed Layer 2 and preserves the staged state on failure.
    pub fn apply_layer2_object_edits(
        &mut self,
        edits: &[ObjectEdit],
    ) -> Result<(), LevelControllerError> {
        let previous = self.state();
        let mut staged = self
            .layer2
            .clone()
            .ok_or(LevelControllerError::Layer2Unavailable)?;
        let NativeLayer2Data::Objects(objects) = &mut staged else {
            return Err(LevelControllerError::Layer2StorageMismatch {
                expected: "objects",
            });
        };
        objects
            .objects
            .apply_edits(edits)
            .map_err(LevelControllerError::Layer2ObjectEdit)?;
        self.layer2 = Some(staged);
        self.finish_edit(previous);
        Ok(())
    }

    /// Replaces selected little-endian Layer 2 tilemap words atomically.
    ///
    /// # Errors
    ///
    /// Rejects unavailable/object-backed Layer 2, invalid or duplicate word indices.
    pub fn apply_layer2_tilemap_words(
        &mut self,
        edits: &[(usize, u16)],
    ) -> Result<(), LevelControllerError> {
        let previous = self.state();
        let mut staged = self
            .layer2
            .clone()
            .ok_or(LevelControllerError::Layer2Unavailable)?;
        let NativeLayer2Data::Tilemap(bytes) = &mut staged else {
            return Err(LevelControllerError::Layer2StorageMismatch {
                expected: "tilemap",
            });
        };
        if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
            return Err(LevelControllerError::Layer2TileIndex(bytes.len() / 2));
        }
        let mut seen = vec![false; bytes.len() / 2];
        for &(index, word) in edits {
            let Some(seen) = seen.get_mut(index) else {
                return Err(LevelControllerError::Layer2TileIndex(index));
            };
            if std::mem::replace(seen, true) {
                return Err(LevelControllerError::Layer2TileDuplicate(index));
            }
            bytes[index * 2..index * 2 + 2].copy_from_slice(&word.to_le_bytes());
        }
        self.layer2 = Some(staged);
        self.finish_edit(previous);
        Ok(())
    }

    fn state(&self) -> LevelControllerState {
        LevelControllerState {
            level: self.level.clone(),
            layer2: self.layer2.clone(),
            dormant_layer2_objects: self.dormant_layer2_objects.clone(),
            layer2_descriptor: self.layer2_descriptor,
        }
    }

    fn restore(&mut self, state: LevelControllerState) {
        self.level = state.level;
        self.layer2 = state.layer2;
        self.dormant_layer2_objects = state.dormant_layer2_objects;
        self.layer2_descriptor = state.layer2_descriptor;
    }

    fn finish_edit(&mut self, previous: LevelControllerState) {
        if self.state() != previous {
            push_bounded(&mut self.undo, previous);
            self.redo.clear();
        }
    }
}

fn reset_layer2_after_mode_change(
    layer2: &mut Option<NativeLayer2Data>,
    dormant_objects: &mut Option<lm_level::LevelObjectData>,
    descriptor: &mut Option<MwlLayer2Descriptor>,
    from: u8,
    to: u8,
    approved: bool,
) -> Result<(), LevelControllerError> {
    use lm_level::{Layer2Storage, level_mode_layer2_storage};

    let from_storage = level_mode_layer2_storage(from);
    let to_storage = level_mode_layer2_storage(to);
    if layer2.is_none() || from_storage == to_storage {
        return Ok(());
    }
    if !approved {
        return Err(LevelControllerError::Layer2ModeChangeRequiresReset { from, to });
    }
    *layer2 = Some(match to_storage {
        Layer2Storage::Objects => {
            NativeLayer2Data::Objects(dormant_objects.clone().unwrap_or_default())
        }
        Layer2Storage::CompressedTilemap => {
            if let Some(NativeLayer2Data::Objects(objects)) = layer2.as_ref() {
                *dormant_objects = Some(objects.clone());
            }
            NativeLayer2Data::Tilemap(vec![0; NATIVE_LAYER2_TILEMAP_LEN])
        }
    });
    if let Some(value) = descriptor {
        *value = match to_storage {
            Layer2Storage::Objects => value.after_tilemap_to_object_mode_change(),
            Layer2Storage::CompressedTilemap => value.after_object_to_tilemap_mode_change(),
        };
    }
    Ok(())
}

fn last_normalized_mode_edit(edits: &[NativeLevelEdit]) -> Option<u8> {
    edits.iter().rev().find_map(|edit| {
        let NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(mode)) = edit else {
            return None;
        };
        (lm_level::lunar_magic_canonical_level_mode(*mode) != *mode).then_some(*mode)
    })
}

fn push_bounded(history: &mut Vec<LevelControllerState>, value: LevelControllerState) {
    if history.len() == LevelController::HISTORY_LIMIT {
        history.remove(0);
    }
    history.push(value);
}

#[cfg(test)]
mod tests;
