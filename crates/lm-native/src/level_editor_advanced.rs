use crate::{level_editor_forms, native_clipboard};
use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
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
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.panel,
                AdvancedPanel::ExpandedHeader,
                "Expanded header",
            );
            ui.selectable_value(&mut self.panel, AdvancedPanel::Layer3, "Layer 3");
        });
        ui.separator();
        match self.panel {
            AdvancedPanel::ExpandedHeader => show_expanded_header(ui, level),
            AdvancedPanel::Layer3 => self.show_layer3(ui, level, revision),
        }
    }

    fn show_layer3(
        &mut self,
        ui: &mut egui::Ui,
        level: &CompleteLevelFile,
        revision: u64,
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let Some(layer3) = level.0.layer3.as_ref() else {
            return ui
                .button("Enable Layer 3 with recovered defaults")
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
        for (label, value) in ["Start position", "Tilemap size", "Liquid/type", "Flags"]
            .into_iter()
            .zip(self.selectors.iter_mut())
        {
            ui.add(egui::Slider::new(value, 0..=u8::MAX).text(label));
        }
        for (slot, value) in self.graphics.iter_mut().enumerate() {
            ui.add(egui::Slider::new(value, 0..=0x0fff).text(format!("Graphics {slot}")));
        }
        ui.label("Reserved bytes (exactly 16 hexadecimal bytes):");
        ui.text_edit_singleline(&mut self.reserved);
        ui.label("Raw tilemap bytes:");
        ui.add(egui::TextEdit::multiline(&mut self.tilemap).desired_rows(5));
        ui.label("Literal remap-command bytes:");
        ui.add(egui::TextEdit::multiline(&mut self.remap).desired_rows(5));
        if let Some(edit) = self.clipboard_controls(ui, layer3) {
            return Some(edit);
        }
        let mut apply = false;
        let mut disable = false;
        ui.horizontal(|ui| {
            apply = ui.button("Apply Layer 3").clicked();
            disable = ui.button("Disable Layer 3").clicked();
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
    ) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
        let mut copy_result = None;
        ui.horizontal(|ui| {
            if ui.button("Copy tilemap").clicked() {
                copy_result = Some(native_clipboard::encode_layer3_tilemap(&layer3.tilemap));
            }
            if ui.button("Paste tilemap").clicked() {
                self.paste_target = Some(PasteTarget::Tilemap);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui.button("Copy remap commands").clicked() {
                copy_result = Some(native_clipboard::encode_layer3_remap(
                    &layer3.remap_commands,
                ));
            }
            if ui.button("Paste remap commands").clicked() {
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
) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
    let mut enabled = level.0.header.expanded.is_some();
    if ui
        .checkbox(&mut enabled, "Expanded header enabled")
        .changed()
    {
        let value = enabled.then(ExpandedLevelHeader::default);
        return Some(Ok(vec![CompleteLevelDocumentEdit::Property(
            LevelPropertyEdit::SetExpandedHeader(value),
        )]));
    }
    let Some(header) = level.0.header.expanded else {
        ui.label("Enable the exact 16-word expanded record to edit its opaque fields.");
        return None;
    };
    let mut fields = header.fields;
    let mut changed = false;
    egui::Grid::new("expanded-level-header-fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, value) in fields.iter_mut().enumerate() {
                ui.label(format!("Field {index:02X}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer3_form_requires_exact_reserved_width() {
        let state = LevelAdvancedPanelState {
            reserved: "00 01".into(),
            ..LevelAdvancedPanelState::default()
        };
        assert!(state.layer3_edits().is_err());
    }
}
