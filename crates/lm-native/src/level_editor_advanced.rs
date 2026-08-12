use crate::{level_editor_forms, native_clipboard};
use eframe::egui;
use lm_app::{CompleteLevelDocumentEdit, ExtendedUiTextKey as Key, LocalizationCatalog};
use lm_level::{CompleteLevelFile, ExpandedLevelHeader, Layer3Data, Layer3Edit, LevelPropertyEdit};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AdvancedPanel {
    #[default]
    ExpandedHeader,
    Layer3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasteTarget {
    Tilemap,
    Remap,
}

#[derive(Default)]
pub(crate) struct LevelAdvancedPanelState {
    panel: AdvancedPanel,
    loaded_layer3_revision: Option<u64>,
    selectors: [u8; 4],
    graphics: [u16; 4],
    reserved: String,
    tilemap: String,
    remap: String,
    paste_target: Option<PasteTarget>,
}

impl LevelAdvancedPanelState {
    pub(crate) fn invalidate(&mut self) {
        self.loaded_layer3_revision = None;
        self.paste_target = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.panel,
                AdvancedPanel::ExpandedHeader,
                advanced_text(catalog, Key::LevelAdvancedExpandedHeader),
            );
            ui.selectable_value(
                &mut self.panel,
                AdvancedPanel::Layer3,
                advanced_text(catalog, Key::LevelAdvancedLayer3),
            );
        });
        ui.separator();
        match self.panel {
            AdvancedPanel::ExpandedHeader => show_expanded_header(ui, level, catalog),
            AdvancedPanel::Layer3 => self.show_layer3(ui, level, revision, catalog),
        }
    }

    fn show_layer3(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let Some(layer3) = level.0.layer3.as_ref() else {
            return ui
                .button(advanced_text(catalog, Key::LevelAdvancedEnableLayer3))
                .clicked()
                .then(|| {
                    Ok(vec![CompleteLevelDocumentEdit::Layer3(Layer3Edit::Enable(
                        Layer3Data::default(),
                    ))])
                });
        };
        if self.loaded_layer3_revision != Some(revision) {
            self.selectors = [
                layer3.settings.start_position,
                layer3.settings.tilemap_size,
                layer3.settings.liquid_type,
                layer3.settings.flags,
            ];
            self.graphics = layer3.settings.graphics_files;
            self.reserved = level_editor_forms::format_bytes(&layer3.settings.reserved);
            self.tilemap = level_editor_forms::format_bytes(&layer3.tilemap);
            self.remap = level_editor_forms::format_bytes(&layer3.remap_commands);
            self.loaded_layer3_revision = Some(revision);
        }
        for (label, value) in [
            Key::LevelAdvancedStartPosition,
            Key::LevelAdvancedTilemapSize,
            Key::LevelAdvancedLiquidType,
            Key::LevelAdvancedFlags,
        ]
        .into_iter()
        .zip(self.selectors.iter_mut())
        {
            ui.add(egui::Slider::new(value, 0..=u8::MAX).text(advanced_text(catalog, label)));
        }
        for (slot, value) in self.graphics.iter_mut().enumerate() {
            ui.add(
                egui::Slider::new(value, 0..=0x0fff).text(
                    advanced_text(catalog, Key::LevelAdvancedGraphicsFormat)
                        .replace("{slot}", &slot.to_string()),
                ),
            );
        }
        ui.label(advanced_text(catalog, Key::LevelAdvancedReservedBytes));
        ui.text_edit_singleline(&mut self.reserved);
        ui.label(advanced_text(catalog, Key::LevelAdvancedRawTilemap));
        ui.add(egui::TextEdit::multiline(&mut self.tilemap).desired_rows(5));
        ui.label(advanced_text(catalog, Key::LevelAdvancedRemapBytes));
        ui.add(egui::TextEdit::multiline(&mut self.remap).desired_rows(5));
        if let Some(edit) = self.clipboard_controls(ui, layer3, catalog) {
            return Some(edit);
        }
        let mut apply = false;
        let mut disable = false;
        ui.horizontal(|ui| {
            apply = ui
                .button(advanced_text(catalog, Key::LevelAdvancedApplyLayer3))
                .clicked();
            disable = ui
                .button(advanced_text(catalog, Key::LevelAdvancedDisableLayer3))
                .clicked();
        });
        if disable {
            Some(Ok(vec![CompleteLevelDocumentEdit::Layer3(
                Layer3Edit::Disable,
            )]))
        } else if apply {
            Some(self.layer3_edits())
        } else {
            None
        }
    }

    fn clipboard_controls(
        &mut self,
        ui: &mut egui::Ui,
        layer3: &Layer3Data,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui
                .button(advanced_text(catalog, Key::LevelAdvancedCopyTilemap))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_layer3_tilemap(&layer3.tilemap));
            }
            if ui
                .button(advanced_text(catalog, Key::LevelAdvancedPasteTilemap))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Tilemap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui
                .button(advanced_text(catalog, Key::LevelAdvancedCopyRemap))
                .clicked()
            {
                copy_result = Some(native_clipboard::encode_layer3_remap(
                    &layer3.remap_commands,
                ));
            }
            if ui
                .button(advanced_text(catalog, Key::LevelAdvancedPasteRemap))
                .clicked()
            {
                self.paste_target = Some(PasteTarget::Remap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(result) = copy_result {
            match result {
                Ok(text) => ui.ctx().copy_text(text),
                Err(error) => return Some(Err(error)),
            }
        }
        let text = pasted_text(ui)?;
        let target = self.paste_target.take()?;
        Some(
            match target {
                PasteTarget::Tilemap => {
                    native_clipboard::decode_layer3_tilemap(&text).map(Layer3Edit::ReplaceTilemap)
                }
                PasteTarget::Remap => native_clipboard::decode_layer3_remap(&text)
                    .map(Layer3Edit::ReplaceRemapCommands),
            }
            .map(|edit| vec![CompleteLevelDocumentEdit::Layer3(edit)]),
        )
    }

    fn layer3_edits(&self) -> Result<Vec<CompleteLevelDocumentEdit>, String> {
        let reserved = level_editor_forms::parse_bytes(&self.reserved, "Layer 3 reserved byte")?;
        let reserved: [u8; 16] = reserved.try_into().map_err(|value: Vec<u8>| {
            format!(
                "Layer 3 reserved field requires 16 bytes, got {}",
                value.len()
            )
        })?;
        let mut edits = vec![
            Layer3Edit::SetStartPosition(self.selectors[0]),
            Layer3Edit::SetTilemapSize(self.selectors[1]),
            Layer3Edit::SetLiquidType(self.selectors[2]),
            Layer3Edit::SetFlags(self.selectors[3]),
        ];
        edits.extend(
            self.graphics
                .iter()
                .copied()
                .enumerate()
                .map(|(slot, file)| Layer3Edit::SetGraphicsFile { slot, file }),
        );
        edits.extend([
            Layer3Edit::SetReserved(reserved),
            Layer3Edit::ReplaceTilemap(level_editor_forms::parse_bytes(
                &self.tilemap,
                "Layer 3 tilemap byte",
            )?),
            Layer3Edit::ReplaceRemapCommands(level_editor_forms::parse_bytes(
                &self.remap,
                "Layer 3 remap byte",
            )?),
        ]);
        Ok(edits
            .into_iter()
            .map(CompleteLevelDocumentEdit::Layer3)
            .collect())
    }
}

fn pasted_text(ui: &egui::Ui) -> Option<String> {
    ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        })
    })
}

fn show_expanded_header(
    ui: &mut egui::Ui,
    level: &CompleteLevelFile,
    catalog: Option<&LocalizationCatalog>,
) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
    let mut enabled = level.0.header.expanded.is_some();
    if ui
        .checkbox(
            &mut enabled,
            advanced_text(catalog, Key::LevelAdvancedExpandedEnabled),
        )
        .changed()
    {
        let value = enabled.then(ExpandedLevelHeader::default);
        return Some(Ok(vec![CompleteLevelDocumentEdit::Property(
            LevelPropertyEdit::SetExpandedHeader(value),
        )]));
    }
    let Some(header) = level.0.header.expanded else {
        ui.label(advanced_text(catalog, Key::LevelAdvancedExpandedNotice));
        return None;
    };
    let mut edited_header = header;
    let mut bypass = header.super_graphics_bypass();
    let mut changed = false;
    ui.heading(advanced_text(catalog, Key::LevelAdvancedSuperGfx));
    changed |= ui
        .checkbox(
            &mut bypass.enabled,
            advanced_text(catalog, Key::LevelAdvancedUsePerLevelGfx),
        )
        .changed();
    egui::Grid::new("super-gfx-bypass")
        .num_columns(4)
        .show(ui, |ui| {
            for (slot, label) in ["FG1", "FG2", "FG3", "BG1", "BG2", "BG3"]
                .into_iter()
                .enumerate()
            {
                ui.label(label);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut bypass.foreground_background[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    )
                    .changed();
                if slot % 2 == 1 {
                    ui.end_row();
                }
            }
            for (slot, label) in ["SP1", "SP2", "SP3", "SP4"].into_iter().enumerate() {
                ui.label(label);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut bypass.sprites[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    )
                    .changed();
                if slot % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    edited_header
        .set_super_graphics_bypass(bypass)
        .expect("bounded Super GFX controls produce valid file numbers");
    ui.separator();
    ui.label(advanced_text(catalog, Key::LevelAdvancedRawExpandedWords));
    let mut fields = edited_header.fields;
    egui::Grid::new("expanded-level-header-fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, value) in fields.iter_mut().enumerate() {
                ui.label(
                    advanced_text(catalog, Key::LevelAdvancedFieldFormat)
                        .replace("{index}", &format!("{index:02X}")),
                );
                changed |= ui
                    .add(egui::DragValue::new(value).hexadecimal(4, false, true))
                    .changed();
                ui.end_row();
            }
        });
    changed.then(|| {
        Ok(fields
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                CompleteLevelDocumentEdit::Property(LevelPropertyEdit::SetExpandedField {
                    index,
                    value,
                })
            })
            .collect())
    })
}

fn advanced_text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_level_advanced_panel_has_no_literal_widget_text() {
        let source = include_str!("level_editor_advanced.rs");
        for literal_widget in [
            "ui.button(\"",
            "ui.label(\"",
            "ui.heading(\"",
            "Button::new(\"",
            ".text(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "level advanced panel bypasses typed localization with {literal_widget}"
            );
        }
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("LevelAdvanced"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}")),
                "level advanced panel does not consume {key:?}"
            );
        }
    }

    #[test]
    fn layer3_form_requires_exact_reserved_width() {
        let state = LevelAdvancedPanelState {
            reserved: "00 01".into(),
            ..LevelAdvancedPanelState::default()
        };
        assert!(state.layer3_edits().is_err());
    }
}
