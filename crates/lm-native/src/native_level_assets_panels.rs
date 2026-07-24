mod animation;
mod level;
mod palette;
mod settings;

use crate::exanimation_form;
use eframe::egui;
use lm_app::NativeLevelAssetsControllerEdit;
use lm_graphics::PaletteOwnership;
use lm_project::NativeLevelAssetsFile;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PasteTarget {
    Object,
    Sprite,
    PaletteColor,
    AnimationRecord,
    AnimationFrame,
}

#[derive(Default)]
pub(crate) struct AggregatePanels {
    tab: usize,
    object_index: usize,
    object: String,
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
        modes: &[bool; 256],
        ownership: &PaletteOwnership,
    ) -> Option<Result<NativeLevelAssetsControllerEdit, String>> {
        self.load(revision, file, modes);
        ui.horizontal(|ui| {
            for (index, name) in ["Level", "Palette", "ExAnimation", "Settings"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.tab, index, *name);
            }
        });
        ui.separator();
        match self.tab {
            0 => self.level_panel(ui, file),
            1 => self.palette_panel(ui, file, ownership),
            2 => self.animation_panel(ui, file, modes),
            _ => self.settings_panel(ui, file),
        }
    }

    fn load(&mut self, revision: u64, file: &NativeLevelAssetsFile, modes: &[bool; 256]) {
        if self.loaded_revision == Some(revision) {
            return;
        }
        let assets = &file.assets;
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
