use super::form_fields::{part_value_fields, text_field};
use super::{AppearancePasteMode, AppearancePasteTarget, OverworldAppearanceEditor};
use crate::{native_clipboard, overworld_appearance_editor_forms::PartForm};
use eframe::egui;
use lm_app::OverworldAppearanceDocumentEdit;
use lm_overworld::SpriteAppearanceDefinition;

impl OverworldAppearanceEditor {
    pub(super) fn definition_fields(
        &mut self,
        ui: &mut egui::Ui,
        definitions: &[SpriteAppearanceDefinition],
    ) -> Option<Result<OverworldAppearanceDocumentEdit, String>> {
        text_field(ui, "Sprite ID (hex)", &mut self.definition.sprite_id);
        let mut edit = None;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.definition.insert_index)
                    .range(0..=definitions.len()),
            );
            if ui.button("Insert definition at index").clicked() {
                edit = Some(self.definition.sprite_id().map(|sprite_id| {
                    OverworldAppearanceDocumentEdit::InsertDefinition {
                        index: self.definition.insert_index,
                        sprite_id,
                    }
                }));
            }
            if ui
                .add_enabled(
                    !definitions.is_empty(),
                    egui::Button::new("Remove selected definition"),
                )
                .clicked()
            {
                edit = Some(Ok(OverworldAppearanceDocumentEdit::RemoveDefinition {
                    sprite_id: definitions[self.definition_index].sprite_id,
                }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.definition.move_before)
                    .range(0..=definitions.len().saturating_sub(1)),
            );
            ui.checkbox(&mut self.definition.move_to_end, "Move to end");
            if ui
                .add_enabled(
                    definitions.len() > 1,
                    egui::Button::new("Move selected definition"),
                )
                .clicked()
            {
                let before = if self.definition.move_to_end {
                    None
                } else {
                    definitions
                        .get(self.definition.move_before)
                        .map(|value| value.sprite_id)
                };
                edit = Some(Ok(OverworldAppearanceDocumentEdit::MoveDefinitionBefore {
                    sprite_id: definitions[self.definition_index].sprite_id,
                    before,
                }));
            }
        });
        edit
    }

    pub(super) fn part_fields(
        &mut self,
        ui: &mut egui::Ui,
        revision: u64,
        definition: &SpriteAppearanceDefinition,
    ) -> Option<Result<OverworldAppearanceDocumentEdit, String>> {
        ui.heading(format!(
            "Tile parts for sprite {:04X}",
            definition.sprite_id
        ));
        ui.label(format!("Painter-ordered parts: {}", definition.parts.len()));
        ui.add(
            egui::Slider::new(
                &mut self.part_index,
                0..=definition.parts.len().saturating_sub(1),
            )
            .text("Part"),
        );
        let key = (revision, definition.sprite_id, self.part_index);
        if self.part_key != Some(key) {
            self.part = definition
                .parts
                .get(self.part_index)
                .copied()
                .map_or_else(PartForm::default, |part| {
                    PartForm::load(part, self.part_index)
                });
            self.part_key = Some(key);
        }
        part_value_fields(ui, &mut self.part);
        let mut edit = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !definition.parts.is_empty(),
                    egui::Button::new("Replace selected part"),
                )
                .clicked()
            {
                edit = Some(self.part.parse().map(|value| {
                    OverworldAppearanceDocumentEdit::ReplacePart {
                        sprite_id: definition.sprite_id,
                        index: self.part_index,
                        value,
                    }
                }));
            }
            if ui
                .add_enabled(
                    !definition.parts.is_empty(),
                    egui::Button::new("Remove selected part"),
                )
                .clicked()
            {
                edit = Some(Ok(OverworldAppearanceDocumentEdit::RemovePart {
                    sprite_id: definition.sprite_id,
                    index: self.part_index,
                }));
            }
        });
        ui.horizontal(|ui| {
            let selected = definition.parts.get(self.part_index).copied();
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Copy part"))
                .clicked()
                && let Some(part) = selected
            {
                match native_clipboard::encode_overworld_appearance_part(part) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Paste over part"))
                .clicked()
            {
                self.clipboard_paste_target = Some(AppearancePasteTarget {
                    revision,
                    sprite_id: definition.sprite_id,
                    mode: AppearancePasteMode::ReplacePart {
                        index: self.part_index,
                    },
                });
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Paste after part"))
                .clicked()
            {
                self.clipboard_paste_target = Some(AppearancePasteTarget {
                    revision,
                    sprite_id: definition.sprite_id,
                    mode: AppearancePasteMode::InsertPartAfter {
                        index: self.part_index,
                    },
                });
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Duplicate part"))
                .clicked()
                && let Some(value) = selected
            {
                let index = self.part_index.saturating_add(1);
                self.part_index = index;
                edit = Some(Ok(OverworldAppearanceDocumentEdit::InsertPart {
                    sprite_id: definition.sprite_id,
                    index,
                    value,
                }));
            }
        });
        ui.horizontal_wrapped(|ui| {
            let has_parts = !definition.parts.is_empty();
            if ui
                .add_enabled(has_parts, egui::Button::new("Copy composition"))
                .clicked()
            {
                match native_clipboard::encode_overworld_appearance_parts(&definition.parts) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => self.error = Some(error),
                }
            }
            for (label, mode) in [
                (
                    "Replace composition",
                    AppearancePasteMode::ReplaceComposition,
                ),
                ("Append composition", AppearancePasteMode::AppendComposition),
            ] {
                if ui.button(label).clicked() {
                    self.clipboard_paste_target = Some(AppearancePasteTarget {
                        revision,
                        sprite_id: definition.sprite_id,
                        mode,
                    });
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                }
            }
            if ui.button("Paste as new definition").clicked() {
                match self.definition.sprite_id() {
                    Ok(sprite_id) => {
                        self.clipboard_paste_target = Some(AppearancePasteTarget {
                            revision,
                            sprite_id,
                            mode: AppearancePasteMode::InsertDefinition {
                                index: self.definition.insert_index,
                            },
                        });
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.part.move_before)
                    .range(0..=definition.parts.len().saturating_sub(1)),
            );
            ui.checkbox(&mut self.part.move_to_end, "Move to end");
            if ui
                .add_enabled(
                    definition.parts.len() > 1,
                    egui::Button::new("Move selected part"),
                )
                .clicked()
            {
                edit = Some(Ok(OverworldAppearanceDocumentEdit::MovePartBefore {
                    sprite_id: definition.sprite_id,
                    index: self.part_index,
                    before: if self.part.move_to_end {
                        None
                    } else {
                        Some(self.part.move_before)
                    },
                }));
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.part.insert_index).range(0..=definition.parts.len()),
            );
            if ui.button("Insert part at index").clicked() {
                edit = Some(self.part.parse().map(|value| {
                    OverworldAppearanceDocumentEdit::InsertPart {
                        sprite_id: definition.sprite_id,
                        index: self.part.insert_index,
                        value,
                    }
                }));
            }
        });
        edit
    }
}
