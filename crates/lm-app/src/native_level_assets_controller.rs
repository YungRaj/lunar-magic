//! Revision-bound, cross-domain native level editing and atomic commit preparation.

use crate::{
    ControllerSnapshot, EditorMode, ExAnimationControllerEdit, ExAnimationControllerEditFailure,
    NativeLevelEdit, PaletteControllerEdit,
};
use lm_graphics::{
    ExAnimationFeatureOptions, PaletteBatchEditError, PaletteChange, PaletteInterchangeFile,
    PaletteOwnership,
};
use lm_level::{
    ExpandedLevelSettingsError, HeaderValueError, Layer2Storage, LegacyHeaderEdit, LevelEditError,
    LevelObjectData, LevelScreenExtentMode, MwlLayer2Descriptor, NATIVE_LAYER2_TILEMAP_LEN,
    NativeLayer2Data, NativeLayer2RemapError, NativeLayer2RemapProgram, NativeSpriteEncodingError,
    ObjectEdit, ObjectEditError, SpriteLengthTable, SpriteStreamError, level_mode_layer2_storage,
    native_level_screen_count,
};
use lm_project::{
    InstalledExAnimationFeatureRomLayout, InstalledLayout, LevelLayer2IoError,
    LevelLayer2RomLayout, LevelLoadError, LevelPointerTable, LoadedExAnimationFeatures,
    LoadedNativeLevelAssets, MwlNativeLevel, NativeLevelAssetsLayout, NativeLevelAssetsLoadError,
    NativeLevelAssetsSaveError, PayloadLoadError, PayloadReadPolicy, Project, SpritePointerTable,
    TransactionError,
};
use lm_rats::RatsBlock;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

mod commit;
mod mwl_export;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLevelAssetsControllerEdit {
    Level(Vec<NativeLevelEdit>),
    Layer2Objects(Vec<ObjectEdit>),
    Layer2TilemapWords(Vec<(usize, u16)>),
    Layer2TilemapRemap {
        script: String,
        global_offset: i32,
        selection: Option<Vec<usize>>,
    },
    Palette(Vec<PaletteControllerEdit>),
    ExAnimation(Vec<ExAnimationControllerEdit>),
    ExAnimationFeatures(ExAnimationFeatureOptions),
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
    Layer2Unavailable {
        command: usize,
    },
    Layer2StorageMismatch {
        command: usize,
        expected: &'static str,
    },
    Layer2ModeChangeRequiresReset {
        command: usize,
        from: u8,
        to: u8,
    },
    Layer2ObjectEdit {
        command: usize,
        error: ObjectEditError,
    },
    Layer2TileIndex {
        command: usize,
        index: usize,
    },
    Layer2TileDuplicate {
        command: usize,
        index: usize,
    },
    Layer2Remap {
        command: usize,
        error: NativeLayer2RemapError,
    },
    Layer2RemapRequiresInstalledBank {
        command: usize,
        bank: u8,
    },
    PaletteEdit {
        command: usize,
        inner: usize,
        error: PaletteBatchEditError,
    },
    ImportPaletteShape {
        expected: usize,
        actual: usize,
    },
    ExAnimationEdit {
        command: usize,
        inner: usize,
        error: ExAnimationControllerEditFailure,
    },
    ExAnimationFeaturesUnavailable {
        command: usize,
    },
    ExAnimationFeatures(lm_project::ExAnimationFeatureIoError),
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
    MwlTargetMismatch {
        expected: usize,
        actual: u16,
    },
    MwlLayer2Unavailable,
    MwlExpandedSettingsPairMismatch {
        destination_installed: bool,
        source_present: bool,
    },
    MwlPaletteShape {
        expected: usize,
        actual: usize,
    },
    MwlExAnimation(lm_graphics::ExAnimationError),
    MwlExAnimationTrailingBytes {
        consumed: usize,
        actual: usize,
    },
    MwlSpriteEncoding(NativeSpriteEncodingError),
    MwlSpriteCanonicalization(LevelEditError),
    MwlHeaderExtent(HeaderValueError),
    MwlSpriteParse(SpriteStreamError),
    MwlNonCanonicalSprites,
    MwlLfix3Unavailable,
    MwlSecondaryExitIndex(usize),
    MwlSecondaryExitDuplicate(usize),
    MwlVanillaEntrance(lm_project::VanillaEntranceIoError),
    MwlLfix3Fields(lm_project::Lfix3LevelFieldsIoError),
    MwlExpandedLevelMode(lm_project::ExpandedLevelModeIoError),
    MwlSeparateMidway(lm_project::SeparateMidwayPatchError),
    MwlSecondaryExits(lm_project::SecondaryExitPatchError),
    Save(NativeLevelAssetsSaveError),
    Layer2Load(LevelLayer2IoError),
    Layer2Save(lm_project::NativeLevelAssetsLayer2SaveError),
    Layer2SaveOptionsRequired,
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
    layer2_layout: Option<LevelLayer2RomLayout>,
    feature_installation: InstalledLayout<InstalledExAnimationFeatureRomLayout>,
    checksum_field: usize,
    source_file_bytes: Vec<u8>,
    sprite_lengths: SpriteLengthTable,
    double_size_modes: [bool; 256],
    palette_ownership: PaletteOwnership,
    baseline: LoadedNativeLevelAssets,
    assets: LoadedNativeLevelAssets,
    baseline_features: Option<LoadedExAnimationFeatures>,
    features: Option<LoadedExAnimationFeatures>,
    baseline_layer2: Option<NativeLayer2Data>,
    layer2: Option<NativeLayer2Data>,
    dormant_layer2_objects: Option<LevelObjectData>,
    baseline_layer2_descriptor: Option<MwlLayer2Descriptor>,
    layer2_descriptor: Option<MwlLayer2Descriptor>,
    normalized_reserved_level_mode: Option<u8>,
    previous_blocks: [Option<RatsBlock>; 5],
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
        Self::decode_with_layer2(
            snapshot,
            layout,
            None,
            sprite_lengths,
            double_size_modes,
            palette_ownership,
        )
    }

    /// Decodes the established aggregate plus an optional profile-described Layer 2 payload.
    ///
    /// # Errors
    ///
    /// Returns the same aggregate errors as [`Self::decode`] plus typed Layer 2 layout, pointer,
    /// codec, or model errors.
    pub fn decode_with_layer2(
        snapshot: &ControllerSnapshot,
        layout: NativeLevelAssetsLayout,
        layer2_layout: Option<LevelLayer2RomLayout>,
        sprite_lengths: &SpriteLengthTable,
        double_size_modes: &[bool],
        palette_ownership: PaletteOwnership,
    ) -> Result<Self, NativeLevelAssetsControllerError> {
        Self::decode_with_layer2_and_features(
            snapshot,
            layout,
            layer2_layout,
            InstalledLayout::Absent,
            sprite_lengths,
            double_size_modes,
            palette_ownership,
        )
    }

    /// Decodes the aggregate, optional Layer 2, and marker-gated animation feature byte.
    ///
    /// # Errors
    ///
    /// Returns all ordinary aggregate errors plus installed feature locator/storage failures.
    pub fn decode_with_layer2_and_features(
        snapshot: &ControllerSnapshot,
        layout: NativeLevelAssetsLayout,
        layer2_layout: Option<LevelLayer2RomLayout>,
        feature_installation: InstalledLayout<InstalledExAnimationFeatureRomLayout>,
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
        let mut assets = project
            .load_native_level_assets(usize::from(slot), layout, sprite_lengths, &modes)
            .map_err(NativeLevelAssetsControllerError::Load)?;
        let baseline = assets.clone();
        let source_level_mode = assets.level.layer1.header.level_mode();
        let normalized_reserved_level_mode = assets
            .level
            .layer1
            .header
            .canonicalize_lunar_magic_level_mode()
            .then_some(source_level_mode);
        let features = if matches!(feature_installation, InstalledLayout::Absent) {
            None
        } else {
            Some(
                project
                    .load_installed_exanimation_features(usize::from(slot), feature_installation)
                    .map_err(NativeLevelAssetsControllerError::ExAnimationFeatures)?,
            )
        };
        let loaded_layer2 = layer2_layout
            .map(|layer2_layout| {
                project.load_level_layer2_with_descriptor(
                    usize::from(slot),
                    assets.level.layer1.header.level_mode(),
                    layer2_layout,
                )
            })
            .transpose()
            .map_err(NativeLevelAssetsControllerError::Layer2Load)?;
        let layer2 = loaded_layer2.as_ref().map(|loaded| loaded.data.clone());
        let layer2_descriptor = loaded_layer2.and_then(|loaded| loaded.descriptor);
        let dormant_layer2_objects = layer2.as_ref().map(|layer2| match layer2 {
            NativeLayer2Data::Objects(objects) => objects.clone(),
            NativeLayer2Data::Tilemap(_) => LevelObjectData::default(),
        });
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
            match layer2_layout {
                Some(layer2_layout) => {
                    snapshot_block(&project, layer2_layout.pointers, slot, layer2_layout.mapper)?
                }
                None => None,
            },
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
            layer2_layout,
            feature_installation,
            checksum_field: snapshot.identity.internal_header_offset + 0x1c,
            source_file_bytes: snapshot.rom_bytes.clone(),
            sprite_lengths: sprite_lengths.clone(),
            double_size_modes: modes,
            palette_ownership,
            baseline,
            assets,
            baseline_features: features,
            features,
            baseline_layer2: layer2.clone(),
            layer2,
            dormant_layer2_objects,
            baseline_layer2_descriptor: layer2_descriptor,
            layer2_descriptor,
            normalized_reserved_level_mode,
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
    pub const fn layer2(&self) -> Option<&NativeLayer2Data> {
        self.layer2.as_ref()
    }

    #[must_use]
    pub const fn layer2_descriptor(&self) -> Option<MwlLayer2Descriptor> {
        self.layer2_descriptor
    }

    /// Returns the reserved source mode that Lunar Magic compatibility normalized to mode `$00`.
    #[must_use]
    pub const fn normalized_reserved_level_mode(&self) -> Option<u8> {
        self.normalized_reserved_level_mode
    }

    #[must_use]
    pub const fn exanimation_features(&self) -> Option<LoadedExAnimationFeatures> {
        self.features
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.assets != self.baseline
            || self.features != self.baseline_features
            || self.layer2 != self.baseline_layer2
            || self.layer2_descriptor != self.baseline_layer2_descriptor
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
        self.apply_edits_with_layer2_reset(edits, false)
    }

    /// Applies a mixed aggregate batch and explicitly authorizes Lunar Magic's destructive Layer
    /// 2 reset when the final level mode crosses the object/tilemap storage boundary.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::apply_edits`]. Without authorization, a mode transition
    /// that would invalidate the staged Layer 2 payload is rejected atomically.
    pub fn apply_edits_with_layer2_reset(
        &mut self,
        edits: &[NativeLevelAssetsControllerEdit],
        reset_layer2: bool,
    ) -> Result<(), NativeLevelAssetsControllerError> {
        let mut staged = self.assets.clone();
        let mut staged_layer2 = self.layer2.clone();
        let mut staged_dormant_layer2_objects = self.dormant_layer2_objects.clone();
        let mut staged_layer2_descriptor = self.layer2_descriptor;
        let mut staged_features = self.features;
        let source_level_mode = staged.level.layer1.header.level_mode();
        apply_native_level_assets_edits(
            &mut staged,
            (
                (&mut staged_layer2, &mut staged_layer2_descriptor),
                &mut staged_features,
            ),
            edits,
            &self.sprite_lengths,
            self.layout.exanimation.maximum_records,
            &self.double_size_modes,
            &self.palette_ownership,
        )?;
        reset_aggregate_layer2_after_mode_change(
            &mut staged_layer2,
            &mut staged_dormant_layer2_objects,
            &mut staged_layer2_descriptor,
            source_level_mode,
            staged.level.layer1.header.level_mode(),
            reset_layer2,
            edits,
        )?;
        self.assets = staged;
        self.layer2 = staged_layer2;
        self.dormant_layer2_objects = staged_dormant_layer2_objects;
        self.layer2_descriptor = staged_layer2_descriptor;
        self.features = staged_features;
        if let Some(mode) = edits
            .iter()
            .rev()
            .find_map(normalized_mode_from_aggregate_edit)
        {
            self.normalized_reserved_level_mode = Some(mode);
        }
        Ok(())
    }

    /// Atomically stages a complete portable palette through the active ownership map.
    ///
    /// The file's source palette is provenance only, allowing intentional cross-level copies.
    /// Every changed protected color rejects the complete import without changing staged assets.
    ///
    /// # Errors
    ///
    /// Returns [`NativeLevelAssetsControllerError`] for a different palette shape or any
    /// ownership-protected imported color.
    pub fn replace_palette_file(
        &mut self,
        file: &PaletteInterchangeFile,
    ) -> Result<(), NativeLevelAssetsControllerError> {
        let expected = self.assets.palette.colors.len();
        let actual = file.palette.colors.len();
        if actual != expected {
            return Err(NativeLevelAssetsControllerError::ImportPaletteShape { expected, actual });
        }
        let changes = self
            .assets
            .palette
            .colors
            .iter()
            .zip(&file.palette.colors)
            .enumerate()
            .filter_map(|(index, (current, imported))| {
                (current != imported).then_some(PaletteChange {
                    index,
                    color: *imported,
                })
            })
            .collect();
        self.apply_edits(&[NativeLevelAssetsControllerEdit::Palette(vec![
            PaletteControllerEdit::ApplyChanges(changes),
        ])])
    }

    /// Replaces every modeled per-level ROM asset from one fully preflighted MWL aggregate.
    ///
    /// This stages Layer 1, Layer 2, sprites, palette, `ExAnimation`, and expanded settings
    /// together. Main/midway entrances and the global secondary-exit table intentionally remain
    /// outside this method so the eventual import coordinator can include them in the same
    /// prepared ROM mutation instead of silently dropping either domain.
    ///
    /// # Errors
    ///
    /// Rejects a different target slot, unavailable Layer 2 or expanded-settings storage, and
    /// sprite streams that cannot be represented by the destination ROM's native format.
    pub fn replace_modeled_assets_from_mwl(
        &mut self,
        source: &MwlNativeLevel,
    ) -> Result<(), NativeLevelAssetsControllerError> {
        let expected = self.assets.level.number;
        let actual = source.header.level_number();
        if usize::from(actual) != expected {
            return Err(NativeLevelAssetsControllerError::MwlTargetMismatch { expected, actual });
        }
        if self.layer2.is_none() {
            return Err(NativeLevelAssetsControllerError::MwlLayer2Unavailable);
        }
        let destination_installed = self.assets.expanded_settings.is_some();
        let source_present = source.expanded_settings.is_some();
        if destination_installed != source_present {
            return Err(
                NativeLevelAssetsControllerError::MwlExpandedSettingsPairMismatch {
                    destination_installed,
                    source_present,
                },
            );
        }
        if source.palette.colors.len() != self.assets.palette.colors.len() {
            return Err(NativeLevelAssetsControllerError::MwlPaletteShape {
                expected: self.assets.palette.colors.len(),
                actual: source.palette.colors.len(),
            });
        }

        let mut sprites = source.sprites.clone();
        sprites
            .canonicalize_for_orientation(source.layer1.header.is_vertical())
            .map_err(NativeLevelAssetsControllerError::MwlSpriteCanonicalization)?;
        let encoded = sprites
            .encode_for_table(&self.sprite_lengths)
            .map_err(NativeLevelAssetsControllerError::MwlSpriteEncoding)?;
        let sprites =
            lm_level::NativeSpriteStream::parse(&encoded, sprites.expanded, &self.sprite_lengths)
                .map_err(NativeLevelAssetsControllerError::MwlSpriteParse)?;
        if sprites
            .encode_for_table(&self.sprite_lengths)
            .map_err(NativeLevelAssetsControllerError::MwlSpriteEncoding)?
            != encoded
        {
            return Err(NativeLevelAssetsControllerError::MwlNonCanonicalSprites);
        }

        let exanimation = source
            .exanimation
            .clone()
            .unwrap_or_else(empty_compact_exanimation);
        let encoded_animation = exanimation
            .encode(&self.double_size_modes)
            .map_err(NativeLevelAssetsControllerError::MwlExAnimation)?;
        let (exanimation, consumed) = lm_graphics::CompactExAnimation::decode(
            &encoded_animation,
            self.layout.exanimation.maximum_records,
            &self.double_size_modes,
        )
        .map_err(NativeLevelAssetsControllerError::MwlExAnimation)?;
        if consumed != encoded_animation.len() {
            return Err(
                NativeLevelAssetsControllerError::MwlExAnimationTrailingBytes {
                    consumed,
                    actual: encoded_animation.len(),
                },
            );
        }

        let mut native_palette = source.palette.clone();
        // Lunar Magic's installed 257-word ROM payload is the MWL working-buffer order rotated
        // right by one word. Restore that exact native order before allocation.
        native_palette.colors.rotate_right(1);

        let mut layer1 = source.layer1.clone();
        let source_level_mode = layer1.header.level_mode();
        let normalized_reserved_level_mode = layer1
            .header
            .canonicalize_lunar_magic_level_mode()
            .then_some(source_level_mode);
        let vertical = layer1.header.is_vertical();
        layer1.objects.canonicalize_import_controls(vertical);
        let screen_count =
            native_level_screen_count(&layer1.objects, &sprites, LevelScreenExtentMode::Auto);
        layer1
            .header
            .set_last_screen(screen_count - 1)
            .map_err(NativeLevelAssetsControllerError::MwlHeaderExtent)?;

        let mut staged = self.assets.clone();
        staged.level.layer1 = layer1;
        staged.level.sprites = sprites;
        staged.palette = native_palette;
        staged.exanimation = exanimation;
        staged
            .expanded_settings
            .clone_from(&source.expanded_settings);

        self.assets = staged;
        self.layer2 = Some(source.layer2.clone());
        self.dormant_layer2_objects = Some(match &source.layer2 {
            NativeLayer2Data::Objects(objects) => objects.clone(),
            NativeLayer2Data::Tilemap(_) => LevelObjectData::default(),
        });
        self.layer2_descriptor = self.layer2_descriptor.map(|_| source.layer2_descriptor);
        if let Some(mode) = normalized_reserved_level_mode {
            self.normalized_reserved_level_mode = Some(mode);
        }
        Ok(())
    }
}

fn reset_aggregate_layer2_after_mode_change(
    layer2: &mut Option<NativeLayer2Data>,
    dormant_objects: &mut Option<LevelObjectData>,
    descriptor: &mut Option<MwlLayer2Descriptor>,
    from: u8,
    to: u8,
    approved: bool,
    edits: &[NativeLevelAssetsControllerEdit],
) -> Result<(), NativeLevelAssetsControllerError> {
    let from_storage = level_mode_layer2_storage(from);
    let to_storage = level_mode_layer2_storage(to);
    if layer2.is_none() || from_storage == to_storage {
        return Ok(());
    }
    let command = edits
        .iter()
        .rposition(|edit| matches!(edit, NativeLevelAssetsControllerEdit::Level(level) if level.iter().any(|edit| matches!(edit, NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(_))))))
        .unwrap_or(0);
    if !approved {
        return Err(
            NativeLevelAssetsControllerError::Layer2ModeChangeRequiresReset { command, from, to },
        );
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

fn normalized_mode_from_aggregate_edit(edit: &NativeLevelAssetsControllerEdit) -> Option<u8> {
    let NativeLevelAssetsControllerEdit::Level(edits) = edit else {
        return None;
    };
    edits.iter().rev().find_map(|edit| {
        let NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(mode)) = edit else {
            return None;
        };
        (lm_level::lunar_magic_canonical_level_mode(*mode) != *mode).then_some(*mode)
    })
}

fn empty_compact_exanimation() -> lm_graphics::CompactExAnimation {
    lm_graphics::CompactExAnimation {
        setting: 0,
        header_value: 0,
        trigger_mask: 0,
        trigger_values: [0; 16],
        records: Vec::new(),
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
    auxiliary: (
        (
            &mut Option<NativeLayer2Data>,
            &mut Option<MwlLayer2Descriptor>,
        ),
        &mut Option<LoadedExAnimationFeatures>,
    ),
    edits: &[NativeLevelAssetsControllerEdit],
    sprite_lengths: &SpriteLengthTable,
    maximum_animation_records: usize,
    double_size_modes: &[bool; 256],
    palette_ownership: &PaletteOwnership,
) -> Result<(), NativeLevelAssetsControllerError> {
    let (layer2, staged_features) = auxiliary;
    let (staged_layer2, staged_layer2_descriptor) = layer2;
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
            NativeLevelAssetsControllerEdit::Layer2Objects(edits) => {
                apply_layer2_object_edits(staged_layer2, command, edits)?;
            }
            NativeLevelAssetsControllerEdit::Layer2TilemapWords(edits) => {
                apply_layer2_tilemap_edits(staged_layer2, command, edits)?;
            }
            NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
                script,
                global_offset,
                selection,
            } => {
                apply_layer2_tilemap_remap(
                    staged_layer2,
                    staged_layer2_descriptor,
                    command,
                    script,
                    *global_offset,
                    selection.as_deref(),
                )?;
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
            NativeLevelAssetsControllerEdit::ExAnimationFeatures(options) => {
                let features = staged_features.as_mut().ok_or(
                    NativeLevelAssetsControllerError::ExAnimationFeaturesUnavailable { command },
                )?;
                features.options = *options;
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

fn apply_layer2_object_edits(
    layer2: &mut Option<NativeLayer2Data>,
    command: usize,
    edits: &[ObjectEdit],
) -> Result<(), NativeLevelAssetsControllerError> {
    let layer2 = layer2
        .as_mut()
        .ok_or(NativeLevelAssetsControllerError::Layer2Unavailable { command })?;
    let NativeLayer2Data::Objects(objects) = layer2 else {
        return Err(NativeLevelAssetsControllerError::Layer2StorageMismatch {
            command,
            expected: "objects",
        });
    };
    objects
        .objects
        .apply_edits(edits)
        .map_err(|error| NativeLevelAssetsControllerError::Layer2ObjectEdit { command, error })
}

fn apply_layer2_tilemap_edits(
    layer2: &mut Option<NativeLayer2Data>,
    command: usize,
    edits: &[(usize, u16)],
) -> Result<(), NativeLevelAssetsControllerError> {
    let layer2 = layer2
        .as_mut()
        .ok_or(NativeLevelAssetsControllerError::Layer2Unavailable { command })?;
    let NativeLayer2Data::Tilemap(bytes) = layer2 else {
        return Err(NativeLevelAssetsControllerError::Layer2StorageMismatch {
            command,
            expected: "tilemap",
        });
    };
    apply_layer2_tilemap_byte_edits(bytes, command, edits)
}

fn apply_layer2_tilemap_byte_edits(
    bytes: &mut [u8],
    command: usize,
    edits: &[(usize, u16)],
) -> Result<(), NativeLevelAssetsControllerError> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLevelAssetsControllerError::Layer2TileIndex {
            command,
            index: bytes.len() / 2,
        });
    }
    let mut seen = vec![false; bytes.len() / 2];
    for &(index, value) in edits {
        let Some(seen) = seen.get_mut(index) else {
            return Err(NativeLevelAssetsControllerError::Layer2TileIndex { command, index });
        };
        if std::mem::replace(seen, true) {
            return Err(NativeLevelAssetsControllerError::Layer2TileDuplicate { command, index });
        }
        bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn apply_layer2_tilemap_remap(
    layer2: &mut Option<NativeLayer2Data>,
    descriptor: &mut Option<MwlLayer2Descriptor>,
    command: usize,
    script: &str,
    global_offset: i32,
    selection: Option<&[usize]>,
) -> Result<(), NativeLevelAssetsControllerError> {
    let layer2 = layer2
        .as_mut()
        .ok_or(NativeLevelAssetsControllerError::Layer2Unavailable { command })?;
    let NativeLayer2Data::Tilemap(bytes) = layer2 else {
        return Err(NativeLevelAssetsControllerError::Layer2StorageMismatch {
            command,
            expected: "tilemap",
        });
    };
    let program = NativeLayer2RemapProgram::parse(script)
        .map_err(|error| NativeLevelAssetsControllerError::Layer2Remap { command, error })?;
    let active_bank = descriptor.map_or(0, MwlLayer2Descriptor::active_bank);
    let result = program
        .apply(bytes, active_bank, global_offset, selection)
        .map_err(|error| NativeLevelAssetsControllerError::Layer2Remap { command, error })?;
    if result.active_bank != active_bank && descriptor.is_none() {
        return Err(
            NativeLevelAssetsControllerError::Layer2RemapRequiresInstalledBank {
                command,
                bank: result.active_bank,
            },
        );
    }
    if let Some(current) = descriptor {
        *current = current
            .after_native_remap(result.active_bank)
            .expect("remap engine returns a bounded active bank");
    }
    apply_layer2_tilemap_byte_edits(bytes, command, &result.edits)
}

#[cfg(test)]
#[path = "native_level_assets_controller_tests.rs"]
mod tests;
