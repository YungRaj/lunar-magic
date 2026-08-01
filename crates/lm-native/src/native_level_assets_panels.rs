mod animation;
mod layer2;
mod level;
mod palette;
mod settings;

use crate::exanimation_form;
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
    object: String,
    layer2_object_index: usize,
    layer2_object: String,
    layer2_tile_index: usize,
    layer2_tile: String,
    layer2_tile_anchor: Option<(usize, usize)>,
    layer2_tile_cursor: Option<(usize, usize)>,
    layer2_fill_pattern: Option<Layer2FillPattern>,
    layer2_remap_script: String,
    layer2_remap_offset: i32,
    layer2_remap_selection_only: bool,
    sprite_index: usize,
    sprite: String,
    sprite_header: String,
    selected_color: usize,
    global: exanimation_form::GlobalForm,
    trigger_index: usize,
    trigger_enabled: bool,
    trigger_value: String,
    record_index: usize,
    frame_index: usize,
    record: exanimation_form::RecordForm,
    settings: [String; 16],
    bypass_enabled: bool,
    bypass_foreground_background: [u16; 6],
    bypass_sprites: [u16; 4],
    exanimation_features: Option<ExAnimationFeatureOptions>,
    loaded_revision: Option<u64>,
    paste_target: Option<PasteTarget>,
}

impl AggregatePanels {
    pub(crate) fn invalidate(&mut self) {
        self.loaded_revision = None;
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
        modes: &[bool; 256],
        ownership: &PaletteOwnership,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        let (layer2, layer2_descriptor) = layer2;
        self.load(revision, file, features, modes);
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
            (_, 0) => self.level_panel(ui, file),
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
        features: Option<LoadedExAnimationFeatures>,
        modes: &[bool; 256],
    ) {
        if self.loaded_revision == Some(revision) {
            return;
        }
        let assets = &file.assets;
        self.exanimation_features = features.map(|features| features.options);
        self.sprite_header = format!("{:02X}", assets.level.sprites.header);
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
            let bypass = lm_level::ExpandedLevelHeader::from(settings).super_graphics_bypass();
            self.bypass_enabled = bypass.enabled;
            self.bypass_foreground_background = bypass.foreground_background;
            self.bypass_sprites = bypass.sprites;
            for (index, field) in self.settings.iter_mut().enumerate() {
                *field = format!("{:04X}", settings.word(index).expect("bounded word"));
            }
        }
        self.loaded_revision = Some(revision);
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
