mod animation;
mod layer2;
mod level;
mod palette;
mod settings;

use crate::{exanimation_form, native_level_document_form};
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_graphics::{ExAnimationFeatureOptions, PaletteOwnership};
use lm_project::{LoadedExAnimationFeatures, NativeLevelAssetsFile};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Layer2FillPattern {
    width: u8,
    height: u8,
    words: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingSelectionMove {
    Object(usize),
    Layer2Object(usize),
    Sprite(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PasteTarget {
    Object,
    Layer2Object,
    Layer2Tilemap,
    Sprite,
    PaletteColor,
    PaletteRow,
    AnimationRecord,
    AnimationFrame,
}

#[derive(Default)]
pub(crate) struct AggregatePanels {
    tab: usize,
    object_index: usize,
    loaded_object_index: Option<usize>,
    level_record: native_level_document_form::NativeLevelRecordForm,
    header: native_level_document_form::NativeLevelHeaderForm,
    layer2_object_index: usize,
    loaded_layer2_object_index: Option<usize>,
    layer2_record: native_level_document_form::NativeLevelRecordForm,
    layer2_tile_index: usize,
    layer2_tile: String,
    layer2_tile_anchor: Option<(usize, usize)>,
    layer2_tile_cursor: Option<(usize, usize)>,
    layer2_fill_pattern: Option<Layer2FillPattern>,
    layer2_remap_script: String,
    layer2_remap_offset: i32,
    layer2_remap_selection_only: bool,
    sprite_index: usize,
    loaded_sprite_index: Option<usize>,
    sprite_header: native_level_document_form::NativeSpriteHeaderForm,
    sprite_vertical_spawn_range: u8,
    sprite_smart_spawn: bool,
    sprite_spawn_available: bool,
    selected_color: usize,
    global: exanimation_form::GlobalForm,
    trigger_index: usize,
    trigger_enabled: bool,
    trigger_value: String,
    record_index: usize,
    frame_index: usize,
    record: exanimation_form::RecordForm,
    settings: [String; 16],
    layer3_settings: crate::expanded_settings_editor_form::ExpandedSettingsForm,
    bypass_enabled: bool,
    bypass_foreground_background: [u16; 6],
    bypass_sprites: [u16; 4],
    sprites_beyond_boundaries_use_air: bool,
    exanimation_features: Option<ExAnimationFeatureOptions>,
    loaded_revision: Option<u64>,
    paste_target: Option<PasteTarget>,
    pending_selection_move: Option<PendingSelectionMove>,
}

impl AggregatePanels {
    pub(crate) fn invalidate(&mut self) {
        if let Some(pending) = self.pending_selection_move.take() {
            match pending {
                PendingSelectionMove::Object(index) => self.object_index = index,
                PendingSelectionMove::Layer2Object(index) => self.layer2_object_index = index,
                PendingSelectionMove::Sprite(index) => self.sprite_index = index,
            }
        }
        self.loaded_revision = None;
    }

    pub(crate) fn reject_pending_edit(&mut self) {
        self.pending_selection_move = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        file: &NativeLevelAssetsFile,
        layer2: (
            Option<&lm_level::NativeLayer2Data>,
            Option<lm_level::MwlLayer2Descriptor>,
        ),
        features: Option<LoadedExAnimationFeatures>,
        lfix3_fields: Option<lm_project::Lfix3LevelFields>,
        modes: &[bool; 256],
        ownership: &PaletteOwnership,
        sprite_lengths: &lm_level::SpriteLengthTable,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let (layer2, layer2_descriptor) = layer2;
        self.load(revision, file, layer2, features, lfix3_fields, modes);
        ui.horizontal(|ui| {
            let tabs = if layer2.is_some() {
                &["Level", "Layer 2", "Palette", "ExAnimation", "Settings"][..]
            } else {
                &["Level", "Palette", "ExAnimation", "Settings"][..]
            };
            for (index, name) in tabs.iter().enumerate() {
                ui.selectable_value(&mut self.tab, index, *name);
            }
        });
        ui.separator();
        match (layer2, self.tab) {
            (_, 0) => self.level_panel(ui, file, sprite_lengths),
            (Some(layer2), 1) => self.layer2_panel(ui, layer2, layer2_descriptor),
            (Some(_), 2) | (None, 1) => self.palette_panel(ui, file, ownership),
            (Some(_), 3) | (None, 2) => self.animation_panel(ui, file, modes),
            _ => self.settings_panel(ui, file),
        }
    }

    fn load(
        &mut self,
        revision: u64,
        file: &NativeLevelAssetsFile,
        layer2: Option<&lm_level::NativeLayer2Data>,
        features: Option<LoadedExAnimationFeatures>,
        lfix3_fields: Option<lm_project::Lfix3LevelFields>,
        modes: &[bool; 256],
    ) {
        if self.loaded_revision == Some(revision) {
            return;
        }
        let assets = &file.assets;
        self.object_index = self
            .object_index
            .min(assets.level.layer1.objects.records.len());
        self.sprite_index = self.sprite_index.min(assets.level.sprites.tokens.len());
        self.loaded_object_index = None;
        self.loaded_sprite_index = None;
        self.sync_level_object_form(&assets.level, true);
        self.sync_sprite_form(&assets.level, true);
        self.loaded_layer2_object_index = None;
        if let Some(lm_level::NativeLayer2Data::Objects(objects)) = layer2 {
            self.layer2_object_index = self.layer2_object_index.min(objects.objects.records.len());
            self.sync_layer2_object_form(objects, true);
        } else {
            self.layer2_record = Default::default();
        }
        self.header = native_level_document_form::NativeLevelHeaderForm::load(&assets.level);
        self.exanimation_features = features.map(|features| features.options);
        self.sprite_header =
            native_level_document_form::NativeSpriteHeaderForm::load(assets.level.sprites.header);
        self.sprite_spawn_available = lfix3_fields.is_some();
        if let Some(fields) = lfix3_fields {
            let spawn = fields.sprite_spawn_settings();
            self.sprite_vertical_spawn_range = spawn.vertical_range();
            self.sprite_smart_spawn = spawn.smart_spawn();
        }
        self.global = exanimation_form::GlobalForm::load(
            assets.exanimation.setting,
            assets.exanimation.header_value,
        );
        self.trigger_enabled = assets.exanimation.trigger_mask & (1 << self.trigger_index) != 0;
        self.trigger_value = format!(
            "{:02X}",
            assets.exanimation.trigger_values[self.trigger_index]
        );
        let records = &assets.exanimation.records;
        self.record_index = self.record_index.min(records.len().saturating_sub(1));
        if let Some(record) = records.get(self.record_index) {
            let double_size = modes[usize::from(record.size_mode())];
            let frames = lm_graphics::exanimation_frames(record, double_size).unwrap_or_default();
            self.record = exanimation_form::RecordForm::load(record, &frames);
        }
        if let Some(settings) = &assets.expanded_settings {
            self.layer3_settings =
                crate::expanded_settings_editor_form::ExpandedSettingsForm::load(settings);
            let bypass = lm_level::ExpandedLevelHeader::from(settings).super_graphics_bypass();
            self.bypass_enabled = bypass.enabled;
            self.bypass_foreground_background = bypass.foreground_background;
            self.bypass_sprites = bypass.sprites;
            self.sprites_beyond_boundaries_use_air =
                lm_level::ExpandedLevelHeader::from(settings).sprites_beyond_boundaries_use_air();
            for (index, field) in self.settings.iter_mut().enumerate() {
                *field = format!("{:04X}", settings.word(index).expect("bounded word"));
            }
        }
        self.loaded_revision = Some(revision);
    }

    fn sync_level_object_form(&mut self, level: &lm_project::LoadedLevelSlot, force: bool) {
        if !force && self.loaded_object_index == Some(self.object_index) {
            return;
        }
        let screen = level::object_stream_screen(&level.layer1.objects, self.object_index);
        self.level_record
            .load_object(level.layer1.objects.records.get(self.object_index), screen);
        self.loaded_object_index = Some(self.object_index);
    }

    fn sync_sprite_form(&mut self, level: &lm_project::LoadedLevelSlot, force: bool) {
        if !force && self.loaded_sprite_index == Some(self.sprite_index) {
            return;
        }
        self.level_record
            .load_sprite(level.sprites.tokens.get(self.sprite_index));
        self.loaded_sprite_index = Some(self.sprite_index);
    }

    fn sync_layer2_object_form(&mut self, objects: &lm_level::LevelObjectData, force: bool) {
        if !force && self.loaded_layer2_object_index == Some(self.layer2_object_index) {
            return;
        }
        let screen = level::object_stream_screen(&objects.objects, self.layer2_object_index);
        self.layer2_record.load_object(
            objects.objects.records.get(self.layer2_object_index),
            screen,
        );
        self.loaded_layer2_object_index = Some(self.layer2_object_index);
    }
}

pub(super) fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

pub(super) fn index(ui: &mut egui::Ui, value: &mut usize, len: usize) {
    ui.horizontal(|ui| {
        ui.label("Index");
        ui.add(egui::DragValue::new(value).range(0..=len));
    });
}

pub(super) const fn move_before_indexes(
    selected: usize,
    count: usize,
    down: bool,
) -> Option<(usize, usize)> {
    if down {
        if selected.saturating_add(1) < count {
            Some((selected + 2, selected + 1))
        } else {
            None
        }
    } else if selected > 0 && selected < count {
        Some((selected - 1, selected - 1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{
        LegacyLevelHeader, LevelObjectData, NativeSpriteStream, ObjectRecord, ObjectStream,
        SpriteRecord, SpriteToken,
    };
    use lm_project::LoadedLevelSlot;

    fn object(bytes: [u8; 3]) -> ObjectRecord {
        ObjectRecord::new(bytes.to_vec()).unwrap()
    }

    fn level() -> LoadedLevelSlot {
        LoadedLevelSlot {
            number: 0,
            layer1: LevelObjectData {
                header: LegacyLevelHeader::default(),
                objects: ObjectStream {
                    records: vec![object([0x45, 0x26, 0x42]), object([0x46, 0x37, 0x43])],
                },
            },
            sprites: NativeSpriteStream {
                header: 0,
                expanded: false,
                tokens: vec![
                    SpriteToken::Record(SpriteRecord {
                        encoded: vec![0, 1, 2],
                    }),
                    SpriteToken::Control(0x80),
                ],
            },
        }
    }

    #[test]
    fn aggregate_forms_follow_selection_but_preserve_unapplied_fields() {
        let level = level();
        let mut panels = AggregatePanels::default();
        panels.sync_level_object_form(&level, true);
        assert_eq!(panels.level_record.object, "45 26 42");
        assert!(panels.level_record.object_fields_loaded);

        panels.level_record.object_parameter = 0xaa;
        panels.sync_level_object_form(&level, false);
        assert_eq!(panels.level_record.object_parameter, 0xaa);
        panels.object_index = 1;
        panels.sync_level_object_form(&level, false);
        assert_eq!(panels.level_record.object, "46 37 43");

        panels.sync_sprite_form(&level, true);
        assert!(panels.level_record.sprite_fields_loaded);
        panels.sprite_index = 1;
        panels.sync_sprite_form(&level, false);
        assert_eq!(panels.level_record.sprite, "control 80");
        assert!(!panels.level_record.sprite_fields_loaded);
    }

    #[test]
    fn revision_reload_refreshes_canonical_layer1_and_layer2_records() {
        let mut level = level();
        let mut panels = AggregatePanels::default();
        panels.sync_level_object_form(&level, true);
        level.layer1.objects.records[0] = object([7, 8, 9]);
        panels.sync_level_object_form(&level, true);
        assert_eq!(panels.level_record.object, "07 08 09");

        let mut layer2 = LevelObjectData {
            header: LegacyLevelHeader::default(),
            objects: ObjectStream {
                records: vec![object([0x0a, 0x0b, 0x0c])],
            },
        };
        panels.sync_layer2_object_form(&layer2, true);
        assert_eq!(panels.layer2_record.object, "0A 0B 0C");
        layer2.objects.records[0] = object([0x0d, 0x0e, 0x0f]);
        panels.sync_layer2_object_form(&layer2, true);
        assert_eq!(panels.layer2_record.object, "0D 0E 0F");
    }

    #[test]
    fn move_indexes_and_pending_selection_follow_native_before_semantics() {
        assert_eq!(move_before_indexes(1, 4, false), Some((0, 0)));
        assert_eq!(move_before_indexes(1, 4, true), Some((3, 2)));
        assert_eq!(move_before_indexes(0, 4, false), None);
        assert_eq!(move_before_indexes(3, 4, true), None);

        let mut panels = AggregatePanels {
            object_index: 1,
            pending_selection_move: Some(PendingSelectionMove::Object(0)),
            ..AggregatePanels::default()
        };
        panels.invalidate();
        assert_eq!(panels.object_index, 0);
        assert_eq!(panels.pending_selection_move, None);
        panels.pending_selection_move = Some(PendingSelectionMove::Sprite(2));
        panels.reject_pending_edit();
        assert_eq!(panels.pending_selection_move, None);
    }
}
