use super::OverworldAppearanceEditor;
use eframe::egui;
use lm_app::NativeOverworldAppearanceEdit;
use lm_level::{Map16Tile, Subtile};
use lm_overworld::{
    NativeOverworldSpriteAppearance, NativeOverworldSpriteDisplay, NativeOverworldSpriteMap16Part,
    NativeOverworldSpriteTooltip,
};

#[derive(Default)]
pub(super) struct NativeAppearanceForm {
    key: Option<(u64, u16)>,
    sprite_id: u16,
    tooltip_enabled: bool,
    disable_position_text: bool,
    tooltip: String,
    appearance_enabled: bool,
    shadow: bool,
    label_mode: bool,
    label_x: i16,
    label_y: i16,
    label: String,
    parts: Vec<NativeOverworldSpriteMap16Part>,
    selected_part: usize,
    map16_tile: u16,
    map16_words: [u16; 4],
    map16_key: Option<(u64, u16)>,
}

impl NativeAppearanceForm {
    pub(super) fn invalidate(&mut self) {
        self.key = None;
        self.map16_key = None;
    }

    fn load(&mut self, revision: u64, value: &lm_app::NativeOverworldAppearanceValue) {
        if self.key == Some((revision, self.sprite_id)) {
            return;
        }
        let tooltip = value.definitions.tooltips.get(&self.sprite_id);
        self.tooltip_enabled = tooltip.is_some();
        self.disable_position_text =
            tooltip.is_some_and(|value| value.disable_original_position_text);
        self.tooltip = tooltip.map_or_else(String::new, |value| value.text.clone());
        let appearance = value.definitions.appearances.get(&self.sprite_id);
        self.appearance_enabled = appearance.is_some();
        self.shadow = appearance.is_some_and(|value| value.shadow);
        match appearance.map(|value| &value.display) {
            Some(NativeOverworldSpriteDisplay::Label { x, y, text }) => {
                self.label_mode = true;
                self.label_x = *x;
                self.label_y = *y;
                self.label = text.clone();
                self.parts.clear();
            }
            Some(NativeOverworldSpriteDisplay::Tiles(parts)) => {
                self.label_mode = false;
                self.parts = parts.clone();
            }
            None => {
                self.label_mode = false;
                self.parts.clear();
                self.label.clear();
            }
        }
        self.selected_part = self.selected_part.min(self.parts.len().saturating_sub(1));
        self.key = Some((revision, self.sprite_id));
    }

    fn load_map16(&mut self, revision: u64, value: &lm_app::NativeOverworldAppearanceValue) {
        self.map16_tile = self.map16_tile.clamp(0x400, 0xbff);
        if self.map16_key == Some((revision, self.map16_tile)) {
            return;
        }
        let tile = value
            .sprite_map16
            .native_tile(usize::from(self.map16_tile))
            .unwrap_or_default();
        self.map16_words = [
            tile.top_left.0,
            tile.top_right.0,
            tile.bottom_left.0,
            tile.bottom_right.0,
        ];
        self.map16_key = Some((revision, self.map16_tile));
    }
}

impl OverworldAppearanceEditor {
    pub(super) fn native_contents(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = self.native_controller.as_ref() else {
            return;
        };
        let revision = controller.revision();
        let value = controller.value().clone();
        self.native_form.load(revision, &value);
        self.native_form.load_map16(revision, &value);

        let mut history = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(controller.can_undo(), egui::Button::new("Undo"))
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(controller.can_redo(), egui::Button::new("Redo"))
                .clicked()
            {
                history = Some(false);
            }
            save = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new("Save Native Pair"),
                )
                .clicked();
            ui.label(if controller.is_modified() {
                "Modified"
            } else {
                "Saved"
            });
            ui.label(format!(
                "{} tooltips, {} appearances, {} graphics ranges, {} palette ranges",
                value.definitions.tooltips.len(),
                value.definitions.appearances.len(),
                value.definitions.graphics_ranges.len(),
                value.definitions.palette_ranges.len(),
            ));
        });
        if let Some(undo) = history {
            let result = if undo {
                self.native_controller.as_mut().unwrap().undo(revision)
            } else {
                self.native_controller.as_mut().unwrap().redo(revision)
            };
            match result {
                Ok(true) => self.native_form.invalidate(),
                Ok(false) => {}
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if save
            && let Err(error) = self
                .persistence
                .begin_pair(self.native_controller.as_mut().unwrap())
        {
            self.error = Some(error);
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Sprite ID");
            if ui
                .add(
                    egui::DragValue::new(&mut self.native_form.sprite_id)
                        .range(0..=0x17f)
                        .hexadecimal(3, false, true),
                )
                .changed()
            {
                self.native_form.invalidate();
            }
        });
        self.native_form.load(revision, &value);

        ui.group(|ui| {
            ui.heading("Tooltip");
            ui.checkbox(&mut self.native_form.tooltip_enabled, "Definition enabled");
            ui.checkbox(
                &mut self.native_form.disable_position_text,
                "Disable original position text",
            );
            ui.add(egui::TextEdit::multiline(&mut self.native_form.tooltip).desired_rows(3));
            if ui.button("Apply Tooltip").clicked() {
                let value =
                    self.native_form
                        .tooltip_enabled
                        .then(|| NativeOverworldSpriteTooltip {
                            disable_original_position_text: self.native_form.disable_position_text,
                            text: self.native_form.tooltip.clone(),
                        });
                self.apply_native_edit(NativeOverworldAppearanceEdit::SetTooltip {
                    sprite_id: self.native_form.sprite_id,
                    value,
                });
            }
        });

        ui.group(|ui| {
            ui.heading("Display Appearance");
            ui.checkbox(
                &mut self.native_form.appearance_enabled,
                "Definition enabled",
            );
            ui.checkbox(&mut self.native_form.shadow, "Editor shadow");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.native_form.label_mode, false, "Map16 tiles");
                ui.selectable_value(&mut self.native_form.label_mode, true, "Text label");
            });
            if self.native_form.label_mode {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut self.native_form.label_x));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut self.native_form.label_y));
                });
                ui.text_edit_singleline(&mut self.native_form.label);
            } else {
                self.native_parts(ui);
            }
            if ui.button("Apply Appearance").clicked() {
                let display = if self.native_form.label_mode {
                    NativeOverworldSpriteDisplay::Label {
                        x: self.native_form.label_x,
                        y: self.native_form.label_y,
                        text: self.native_form.label.clone(),
                    }
                } else {
                    NativeOverworldSpriteDisplay::Tiles(self.native_form.parts.clone())
                };
                let value =
                    self.native_form
                        .appearance_enabled
                        .then(|| NativeOverworldSpriteAppearance {
                            shadow: self.native_form.shadow,
                            display,
                        });
                self.apply_native_edit(NativeOverworldAppearanceEdit::SetAppearance {
                    sprite_id: self.native_form.sprite_id,
                    value,
                });
            }
        });

        ui.group(|ui| {
            ui.heading("Custom Sprite Map16 Definition");
            ui.horizontal(|ui| {
                ui.label("Native tile");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.native_form.map16_tile)
                            .range(0x400..=0xbff)
                            .hexadecimal(3, false, true),
                    )
                    .changed()
                {
                    self.native_form.map16_key = None;
                }
                for (label, word) in ["TL", "TR", "BL", "BR"]
                    .into_iter()
                    .zip(&mut self.native_form.map16_words)
                {
                    ui.label(label);
                    ui.add(egui::DragValue::new(word).hexadecimal(4, false, true));
                }
            });
            if ui.button("Apply Sprite Map16").clicked() {
                self.apply_native_edit(NativeOverworldAppearanceEdit::SetCustomMap16 {
                    native_tile: self.native_form.map16_tile,
                    value: Map16Tile {
                        top_left: Subtile(self.native_form.map16_words[0]),
                        top_right: Subtile(self.native_form.map16_words[1]),
                        bottom_left: Subtile(self.native_form.map16_words[2]),
                        bottom_right: Subtile(self.native_form.map16_words[3]),
                        acts_like: 0,
                    },
                });
            }
        });
    }

    fn native_parts(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!("Parts: {}", self.native_form.parts.len()));
            if !self.native_form.parts.is_empty() {
                ui.add(
                    egui::Slider::new(
                        &mut self.native_form.selected_part,
                        0..=self.native_form.parts.len() - 1,
                    )
                    .text("Part"),
                );
            }
            if ui.button("Add Part").clicked() {
                self.native_form.parts.push(NativeOverworldSpriteMap16Part {
                    x: 0,
                    y: 0,
                    tile: 0x400,
                    translucent: false,
                });
                self.native_form.selected_part = self.native_form.parts.len() - 1;
            }
            if ui
                .add_enabled(
                    !self.native_form.parts.is_empty(),
                    egui::Button::new("Remove Part"),
                )
                .clicked()
            {
                self.native_form
                    .parts
                    .remove(self.native_form.selected_part);
                self.native_form.selected_part = self
                    .native_form
                    .selected_part
                    .min(self.native_form.parts.len().saturating_sub(1));
            }
        });
        if let Some(part) = self
            .native_form
            .parts
            .get_mut(self.native_form.selected_part)
        {
            ui.horizontal(|ui| {
                ui.label("X");
                ui.add(egui::DragValue::new(&mut part.x));
                ui.label("Y");
                ui.add(egui::DragValue::new(&mut part.y));
                ui.label("Map16");
                ui.add(
                    egui::DragValue::new(&mut part.tile)
                        .range(0..=0xcff)
                        .hexadecimal(3, false, true),
                );
                ui.checkbox(&mut part.translucent, "Translucent");
            });
        }
    }

    fn apply_native_edit(&mut self, edit: NativeOverworldAppearanceEdit) {
        let Some(controller) = self.native_controller.as_mut() else {
            return;
        };
        match controller.apply_edits(controller.revision(), &[edit]) {
            Ok(()) => self.native_form.invalidate(),
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::NativeOverworldAppearanceController;

    #[test]
    fn native_form_loads_every_native_only_display_field_without_conversion() {
        let controller = NativeOverworldAppearanceController::decode(
            "sprites.sscov".into(),
            "sprites.s16ov".into(),
            b"05\t1\tTooltip\n05\t3\t-4,6,8400 12,-8,C01\n",
            &[1, 0, 2, 0, 3, 0, 4, 0],
        )
        .unwrap();
        let mut form = NativeAppearanceForm {
            sprite_id: 5,
            map16_tile: 0x400,
            ..NativeAppearanceForm::default()
        };
        form.load(controller.revision(), controller.value());
        form.load_map16(controller.revision(), controller.value());
        assert!(form.tooltip_enabled);
        assert!(form.disable_position_text);
        assert_eq!(form.tooltip, "Tooltip");
        assert!(form.appearance_enabled);
        assert!(form.shadow);
        assert!(!form.label_mode);
        assert_eq!(form.parts.len(), 2);
        assert_eq!(form.parts[0].x, -4);
        assert!(form.parts[0].translucent);
        assert_eq!(form.parts[1].tile, 0xc01);
        assert_eq!(form.map16_words, [1, 2, 3, 4]);
    }

    #[test]
    fn native_form_loads_positioned_labels_as_labels() {
        let controller = NativeOverworldAppearanceController::decode(
            "sprites.sscov".into(),
            "sprites.s16ov".into(),
            b"06\t2\t7,-9,*Native Label*\n",
            &[],
        )
        .unwrap();
        let mut form = NativeAppearanceForm {
            sprite_id: 6,
            map16_tile: 0x400,
            ..NativeAppearanceForm::default()
        };
        form.load(controller.revision(), controller.value());
        assert!(form.label_mode);
        assert_eq!((form.label_x, form.label_y), (7, -9));
        assert_eq!(form.label, "Native Label");
    }
}
