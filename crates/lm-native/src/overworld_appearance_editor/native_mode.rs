use super::OverworldAppearanceEditor;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, NativeOverworldAppearanceEdit};
use lm_level::{Map16Tile, Subtile};
use lm_overworld::{
    NativeOverworldSpriteAppearance, NativeOverworldSpriteDisplay, NativeOverworldSpriteMap16Part,
    NativeOverworldSpriteRange, NativeOverworldSpriteTooltip,
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
    ranges_key: Option<u64>,
    graphics_ranges: Vec<NativeOverworldSpriteRange>,
    palette_ranges: Vec<NativeOverworldSpriteRange>,
}

impl NativeAppearanceForm {
    pub(super) fn invalidate(&mut self) {
        self.key = None;
        self.map16_key = None;
        self.ranges_key = None;
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

    fn load_ranges(&mut self, revision: u64, value: &lm_app::NativeOverworldAppearanceValue) {
        if self.ranges_key == Some(revision) {
            return;
        }
        self.graphics_ranges = value.definitions.graphics_ranges.clone();
        self.palette_ranges = value.definitions.palette_ranges.clone();
        self.ranges_key = Some(revision);
    }
}

impl OverworldAppearanceEditor {
    pub(super) fn native_contents(
        &mut self,
        ui: &mut egui::Ui,
        catalog: Option<&LocalizationCatalog>,
    ) {
        let Some(controller) = self.native_controller.as_ref() else {
            return;
        };
        let revision = controller.revision();
        let value = controller.value().clone();
        self.native_form.load(revision, &value);
        self.native_form.load_map16(revision, &value);
        self.native_form.load_ranges(revision, &value);

        let mut history = None;
        let mut save = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    controller.can_undo(),
                    egui::Button::new(text(catalog, Key::AppearanceUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    controller.can_redo(),
                    egui::Button::new(text(catalog, Key::AppearanceRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, Key::OverworldAppearanceSaveNative)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if controller.is_modified() {
                    Key::AppearanceModified
                } else {
                    Key::AppearanceSaved
                },
            ));
            ui.label(
                text(catalog, Key::OverworldAppearanceNativeSummaryFormat)
                    .replace("{tooltips}", &value.definitions.tooltips.len().to_string())
                    .replace(
                        "{appearances}",
                        &value.definitions.appearances.len().to_string(),
                    )
                    .replace(
                        "{graphics}",
                        &value.definitions.graphics_ranges.len().to_string(),
                    )
                    .replace(
                        "{palettes}",
                        &value.definitions.palette_ranges.len().to_string(),
                    ),
            );
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
            ui.label(text(catalog, Key::OverworldAppearanceNativeSpriteId));
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
            ui.heading(text(catalog, Key::OverworldAppearanceTooltip));
            ui.checkbox(
                &mut self.native_form.tooltip_enabled,
                text(catalog, Key::OverworldAppearanceDefinitionEnabled),
            );
            ui.checkbox(
                &mut self.native_form.disable_position_text,
                text(catalog, Key::OverworldAppearanceDisablePositionText),
            );
            ui.add(egui::TextEdit::multiline(&mut self.native_form.tooltip).desired_rows(3));
            if ui
                .button(text(catalog, Key::OverworldAppearanceApplyTooltip))
                .clicked()
            {
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
            ui.heading(text(catalog, Key::OverworldAppearanceExternalRanges));
            ui.label(text(catalog, Key::OverworldAppearanceRangesNotice));
            let mut edit = None;
            Self::native_ranges(
                ui,
                text(catalog, Key::OverworldAppearanceGraphics),
                &mut self.native_form.graphics_ranges,
                &mut edit,
                NativeOverworldAppearanceEdit::ReplaceGraphicsRanges,
                catalog,
            );
            Self::native_ranges(
                ui,
                text(catalog, Key::OverworldAppearancePalette),
                &mut self.native_form.palette_ranges,
                &mut edit,
                NativeOverworldAppearanceEdit::ReplacePaletteRanges,
                catalog,
            );
            if let Some(edit) = edit {
                self.apply_native_edit(edit);
            }
        });

        ui.group(|ui| {
            ui.heading(text(catalog, Key::OverworldAppearanceDisplay));
            ui.checkbox(
                &mut self.native_form.appearance_enabled,
                text(catalog, Key::OverworldAppearanceDefinitionEnabled),
            );
            ui.checkbox(
                &mut self.native_form.shadow,
                text(catalog, Key::OverworldAppearanceEditorShadow),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.native_form.label_mode,
                    false,
                    text(catalog, Key::OverworldAppearanceMap16Tiles),
                );
                ui.selectable_value(
                    &mut self.native_form.label_mode,
                    true,
                    text(catalog, Key::OverworldAppearanceTextLabel),
                );
            });
            if self.native_form.label_mode {
                ui.horizontal(|ui| {
                    ui.label(text(catalog, Key::OverworldAppearanceX));
                    ui.add(egui::DragValue::new(&mut self.native_form.label_x));
                    ui.label(text(catalog, Key::OverworldAppearanceY));
                    ui.add(egui::DragValue::new(&mut self.native_form.label_y));
                });
                ui.text_edit_singleline(&mut self.native_form.label);
            } else {
                self.native_parts(ui, catalog);
            }
            if ui
                .button(text(catalog, Key::OverworldAppearanceApplyDisplay))
                .clicked()
            {
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
            ui.heading(text(catalog, Key::OverworldAppearanceCustomMap16));
            ui.horizontal(|ui| {
                ui.label(text(catalog, Key::OverworldAppearanceNativeTile));
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
                for (label, word) in [
                    Key::OverworldAppearanceTopLeft,
                    Key::OverworldAppearanceTopRight,
                    Key::OverworldAppearanceBottomLeft,
                    Key::OverworldAppearanceBottomRight,
                ]
                .into_iter()
                .zip(&mut self.native_form.map16_words)
                {
                    ui.label(text(catalog, label));
                    ui.add(egui::DragValue::new(word).hexadecimal(4, false, true));
                }
            });
            if ui
                .button(text(catalog, Key::OverworldAppearanceApplyMap16))
                .clicked()
            {
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

    fn native_parts(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.horizontal(|ui| {
            ui.label(
                text(catalog, Key::OverworldAppearanceNativePartsFormat)
                    .replace("{count}", &self.native_form.parts.len().to_string()),
            );
            if !self.native_form.parts.is_empty() {
                ui.add(
                    egui::Slider::new(
                        &mut self.native_form.selected_part,
                        0..=self.native_form.parts.len() - 1,
                    )
                    .text(text(catalog, Key::OverworldAppearancePart)),
                );
            }
            if ui
                .button(text(catalog, Key::OverworldAppearanceAddPart))
                .clicked()
            {
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
                    egui::Button::new(text(catalog, Key::OverworldAppearanceRemovePartNative)),
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
            if ui
                .add_enabled(
                    self.native_form.selected_part > 0,
                    egui::Button::new(text(catalog, Key::OverworldAppearanceSendBackward)),
                )
                .clicked()
            {
                let selected = self.native_form.selected_part;
                self.native_form.parts.swap(selected, selected - 1);
                self.native_form.selected_part -= 1;
            }
            if ui
                .add_enabled(
                    self.native_form.selected_part + 1 < self.native_form.parts.len(),
                    egui::Button::new(text(catalog, Key::OverworldAppearanceBringForward)),
                )
                .clicked()
            {
                let selected = self.native_form.selected_part;
                self.native_form.parts.swap(selected, selected + 1);
                self.native_form.selected_part += 1;
            }
        });
        if let Some(part) = self
            .native_form
            .parts
            .get_mut(self.native_form.selected_part)
        {
            ui.horizontal(|ui| {
                ui.label(text(catalog, Key::OverworldAppearanceX));
                ui.add(egui::DragValue::new(&mut part.x));
                ui.label(text(catalog, Key::OverworldAppearanceY));
                ui.add(egui::DragValue::new(&mut part.y));
                ui.label(text(catalog, Key::OverworldAppearanceMap16));
                ui.add(
                    egui::DragValue::new(&mut part.tile)
                        .range(0..=0xcff)
                        .hexadecimal(3, false, true),
                );
                ui.checkbox(
                    &mut part.translucent,
                    text(catalog, Key::OverworldAppearanceTranslucent),
                );
            });
        }
    }

    fn native_ranges(
        ui: &mut egui::Ui,
        label: String,
        ranges: &mut Vec<NativeOverworldSpriteRange>,
        edit: &mut Option<NativeOverworldAppearanceEdit>,
        replacement: fn(Vec<NativeOverworldSpriteRange>) -> NativeOverworldAppearanceEdit,
        catalog: Option<&LocalizationCatalog>,
    ) {
        ui.horizontal(|ui| {
            ui.strong(&label);
            if ui
                .button(text(catalog, Key::OverworldAppearanceAddRange))
                .clicked()
            {
                ranges.push(NativeOverworldSpriteRange {
                    kind: 0,
                    first_tile: 0x400,
                    last_tile: 0x400,
                    base: 0,
                });
            }
            if ui
                .button(
                    text(catalog, Key::OverworldAppearanceApplyRangesFormat)
                        .replace("{kind}", &label),
                )
                .clicked()
            {
                *edit = Some(replacement(ranges.clone()));
            }
        });
        let mut remove = None;
        for (index, range) in ranges.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("#{index}"));
                ui.label(text(catalog, Key::OverworldAppearanceKind));
                ui.add(egui::DragValue::new(&mut range.kind).hexadecimal(4, false, true));
                ui.label(text(catalog, Key::OverworldAppearanceFirst));
                ui.add(
                    egui::DragValue::new(&mut range.first_tile)
                        .range(0..=0xbff)
                        .hexadecimal(3, false, true),
                );
                ui.label(text(catalog, Key::OverworldAppearanceLast));
                ui.add(
                    egui::DragValue::new(&mut range.last_tile)
                        .range(0..=0xbff)
                        .hexadecimal(3, false, true),
                );
                ui.label(text(catalog, Key::OverworldAppearanceBase));
                ui.add(egui::DragValue::new(&mut range.base).hexadecimal(4, false, true));
                if ui
                    .small_button(text(catalog, Key::OverworldAppearanceRemoveRange))
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            ranges.remove(index);
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

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_overworld_appearance_surface_has_no_literal_widget_text() {
        let source = include_str!("native_mode.rs");
        for literal in [
            "ui.button(\"",
            "ui.label(\"",
            "ui.heading(\"",
            "ui.strong(\"",
            "Button::new(\"",
            ".text(\"",
        ] {
            assert!(
                !source.contains(literal),
                "native overworld appearance surface bypasses localization with {literal}"
            );
        }
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("OverworldAppearance"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}"))
                    || matches!(
                        key,
                        Key::OverworldAppearancePortableTitle
                            | Key::OverworldAppearanceNativeTitle
                            | Key::OverworldAppearanceImportNative
                            | Key::OverworldAppearanceExportNative
                            | Key::OverworldAppearanceDefinitionsFormat
                            | Key::OverworldAppearanceDefinition
                            | Key::OverworldAppearanceEmptyNotice
                            | Key::OverworldAppearanceSpriteId
                            | Key::OverworldAppearanceInsertDefinition
                            | Key::OverworldAppearanceRemoveDefinition
                            | Key::OverworldAppearanceMoveToEnd
                            | Key::OverworldAppearanceMoveDefinition
                            | Key::OverworldAppearancePartsTitleFormat
                            | Key::OverworldAppearancePartsCountFormat
                            | Key::OverworldAppearanceReplacePart
                            | Key::OverworldAppearanceRemovePart
                            | Key::OverworldAppearanceCopyPart
                            | Key::OverworldAppearancePasteOverPart
                            | Key::OverworldAppearancePasteAfterPart
                            | Key::OverworldAppearanceDuplicatePart
                            | Key::OverworldAppearanceCopyComposition
                            | Key::OverworldAppearanceReplaceComposition
                            | Key::OverworldAppearanceAppendComposition
                            | Key::OverworldAppearancePasteNewDefinition
                            | Key::OverworldAppearanceMovePart
                            | Key::OverworldAppearanceInsertPart
                            | Key::OverworldAppearancePreviewTitle
                            | Key::OverworldAppearancePreviewNotice
                    ),
                "native overworld appearance surface does not consume {key:?}"
            );
        }
    }
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
    fn native_form_loads_graphics_and_palette_ranges_without_narrowing() {
        let controller = NativeOverworldAppearanceController::decode(
            "sprites.sscov".into(),
            "sprites.s16ov".into(),
            b"10000\t12\t400-4FF,1234\n20000\tABCD\t800-BFF,FFFF\n",
            &[],
        )
        .unwrap();
        let mut form = NativeAppearanceForm::default();
        form.load_ranges(controller.revision(), controller.value());
        assert_eq!(
            form.graphics_ranges,
            [NativeOverworldSpriteRange {
                kind: 0x12,
                first_tile: 0x400,
                last_tile: 0x4ff,
                base: 0x1234,
            }]
        );
        assert_eq!(form.palette_ranges[0].kind, 0xabcd);
        assert_eq!(form.palette_ranges[0].base, 0xffff);
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
