use crate::{
    level_editor_forms, native_clipboard,
    overworld_editor_forms::{EndpointForm, RevealForm, SUBMAP_NAMES, SpriteForm},
};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog, OverworldControllerEdit};
use lm_project::CompleteOverworldFile;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RecordPanel {
    #[default]
    Reveals,
    Endpoints,
    Messages,
    Sprites,
}

#[derive(Default)]
pub(crate) struct OverworldRecordPanels {
    panel: RecordPanel,
    reveal_index: usize,
    reveal: RevealForm,
    reveal_key: Option<(u64, usize)>,
    reveal_selection_start: usize,
    reveal_selection_end: usize,
    reveal_delta_x: i16,
    reveal_delta_y: i16,
    endpoint_index: usize,
    endpoint: EndpointForm,
    endpoint_key: Option<(u64, usize)>,
    message: usize,
    message_column: usize,
    message_row: usize,
    message_tile: String,
    message_key: Option<(u64, usize, usize, usize)>,
    sprite_index: usize,
    sprite: SpriteForm,
    sprite_key: Option<(u64, usize)>,
}

impl OverworldRecordPanels {
    pub(crate) fn invalidate(&mut self) {
        self.reveal_key = None;
        self.endpoint_key = None;
        self.message_key = None;
        self.sprite_key = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.panel,
                RecordPanel::Reveals,
                records_text(catalog, Key::OverworldRecordsReveals),
            );
            ui.selectable_value(
                &mut self.panel,
                RecordPanel::Endpoints,
                records_text(catalog, Key::OverworldRecordsEndpoints),
            );
            ui.selectable_value(
                &mut self.panel,
                RecordPanel::Messages,
                records_text(catalog, Key::OverworldRecordsMessages),
            );
            ui.selectable_value(
                &mut self.panel,
                RecordPanel::Sprites,
                records_text(catalog, Key::OverworldRecordsSprites),
            );
        });
        ui.separator();
        match self.panel {
            RecordPanel::Reveals => self.show_reveals(ui, world, revision, catalog),
            RecordPanel::Endpoints => self.show_endpoints(ui, world, revision, catalog),
            RecordPanel::Messages => self.show_messages(ui, world, revision, catalog),
            RecordPanel::Sprites => self.show_sprites(ui, world, revision, catalog),
        }
    }

    fn show_reveals(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.event_reveals.entries;
        if entries.is_empty() {
            ui.label(records_text(catalog, Key::OverworldRecordsNoReveals));
            return None;
        }
        self.reveal_index = self.reveal_index.min(entries.len() - 1);
        ui.add(
            egui::Slider::new(&mut self.reveal_index, 0..=entries.len() - 1)
                .text(records_text(catalog, Key::OverworldRecordsReveal)),
        );
        let key = (revision, self.reveal_index);
        if self.reveal_key != Some(key) {
            self.reveal = RevealForm::load(entries[self.reveal_index]);
            self.reveal_key = Some(key);
        }
        for (label, field) in [
            (
                records_text(catalog, Key::OverworldRecordsSourceTile),
                &mut self.reveal.source,
            ),
            (
                records_text(catalog, Key::OverworldRecordsDestinationTile),
                &mut self.reveal.destination,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(&label);
                ui.text_edit_singleline(field);
            });
        }
        if ui
            .button(records_text(catalog, Key::OverworldRecordsApplyReveal))
            .clicked()
        {
            return Some(self.reveal.parse().map(|reveal| {
                OverworldControllerEdit::ReplaceEventReveal {
                    index: self.reveal_index,
                    reveal,
                }
            }));
        }
        ui.separator();
        ui.label(records_text(catalog, Key::OverworldRecordsMoveSelection));
        let maximum = entries.len() - 1;
        self.reveal_selection_start = self.reveal_selection_start.min(maximum);
        self.reveal_selection_end = self.reveal_selection_end.min(maximum);
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.reveal_selection_start)
                    .range(0..=maximum)
                    .prefix(records_text(catalog, Key::OverworldRecordsFirstPrefix)),
            );
            ui.add(
                egui::DragValue::new(&mut self.reveal_selection_end)
                    .range(0..=maximum)
                    .prefix(records_text(catalog, Key::OverworldRecordsLastPrefix)),
            );
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.reveal_delta_x)
                    .range(-63..=63)
                    .prefix(records_text(catalog, Key::OverworldRecordsXTilesPrefix)),
            );
            ui.add(
                egui::DragValue::new(&mut self.reveal_delta_y)
                    .range(-127..=127)
                    .prefix(records_text(catalog, Key::OverworldRecordsYTilesPrefix)),
            );
        });
        ui.small(records_text(catalog, Key::OverworldRecordsMoveNotice));
        ui.button(records_text(catalog, Key::OverworldRecordsMoveSelected))
            .clicked()
            .then(|| {
                let start = self.reveal_selection_start.min(self.reveal_selection_end);
                let end = self.reveal_selection_start.max(self.reveal_selection_end);
                Ok(OverworldControllerEdit::RelocateEventReveals {
                    selection: (start..=end).collect(),
                    delta_x: self.reveal_delta_x,
                    delta_y: self.reveal_delta_y,
                })
            })
    }

    fn show_endpoints(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.endpoints;
        if entries.is_empty() {
            ui.label(records_text(catalog, Key::OverworldRecordsNoEndpoints));
            return None;
        }
        self.endpoint_index = self.endpoint_index.min(entries.len() - 1);
        ui.add(
            egui::Slider::new(&mut self.endpoint_index, 0..=entries.len() - 1)
                .text(records_text(catalog, Key::OverworldRecordsEndpoint)),
        );
        let key = (revision, self.endpoint_index);
        if self.endpoint_key != Some(key) {
            self.endpoint = EndpointForm::load(entries[self.endpoint_index]);
            self.endpoint_key = Some(key);
        }
        for (label, field) in [
            (
                records_text(catalog, Key::OverworldRecordsXHex),
                &mut self.endpoint.x,
            ),
            (
                records_text(catalog, Key::OverworldRecordsYHex),
                &mut self.endpoint.y,
            ),
            (
                records_text(catalog, Key::OverworldRecordsSubmapHex),
                &mut self.endpoint.submap,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(&label);
                ui.text_edit_singleline(field);
            });
        }
        ui.button(records_text(catalog, Key::OverworldRecordsApplyEndpoint))
            .clicked()
            .then(|| {
                self.endpoint
                    .parse()
                    .map(|endpoint| OverworldControllerEdit::ReplaceEndpoint {
                        index: self.endpoint_index,
                        endpoint,
                    })
            })
    }

    fn show_messages(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.messages;
        if entries.is_empty() {
            ui.label(records_text(catalog, Key::OverworldRecordsNoMessages));
            return None;
        }
        self.message = self.message.min(entries.len() - 1);
        ui.add(
            egui::Slider::new(&mut self.message, 0..=entries.len() - 1)
                .text(records_text(catalog, Key::OverworldRecordsMessage)),
        );
        ui.add(
            egui::Slider::new(&mut self.message_column, 0..=17)
                .text(records_text(catalog, Key::OverworldRecordsColumn)),
        );
        ui.add(
            egui::Slider::new(&mut self.message_row, 0..=7)
                .text(records_text(catalog, Key::OverworldRecordsRow)),
        );
        let key = (
            revision,
            self.message,
            self.message_column,
            self.message_row,
        );
        if self.message_key != Some(key) {
            let index = self.message_row * 18 + self.message_column;
            self.message_tile = format!("{:02X}", entries[self.message].0[index]);
            self.message_key = Some(key);
        }
        ui.horizontal(|ui| {
            ui.label(records_text(catalog, Key::OverworldRecordsTileHex));
            ui.text_edit_singleline(&mut self.message_tile);
        });
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(records_text(catalog, Key::OverworldRecordsCopyMessage))
                .clicked()
            {
                match native_clipboard::encode_overworld_message(&entries[self.message]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui
                .button(records_text(catalog, Key::OverworldRecordsPasteMessage))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if let Some(text) = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        }) {
            return Some(
                native_clipboard::decode_overworld_message(&text).map(|message| {
                    OverworldControllerEdit::ReplaceMessage {
                        index: self.message,
                        message,
                    }
                }),
            );
        }
        ui.button(records_text(catalog, Key::OverworldRecordsApplyMessageTile))
            .clicked()
            .then(|| {
                level_editor_forms::parse_hex_u8(&self.message_tile, "message tile").map(|tile| {
                    OverworldControllerEdit::SetMessageTile {
                        message: self.message,
                        column: self.message_column,
                        row: self.message_row,
                        tile,
                    }
                })
            })
    }

    fn show_sprites(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.sprites;
        if entries.is_empty() {
            ui.label(records_text(catalog, Key::OverworldRecordsNoSprites));
            return None;
        }
        self.sprite_index = self.sprite_index.min(entries.len() - 1);
        ui.add(
            egui::Slider::new(&mut self.sprite_index, 0..=entries.len() - 1)
                .text(records_text(catalog, Key::OverworldRecordsSprite)),
        );
        let key = (revision, self.sprite_index);
        if self.sprite_key != Some(key) {
            self.sprite = SpriteForm::load(&entries[self.sprite_index]);
            self.sprite_key = Some(key);
        }
        for (label, field) in [
            (
                records_text(catalog, Key::OverworldRecordsIdHex),
                &mut self.sprite.id,
            ),
            (
                records_text(catalog, Key::OverworldRecordsXHex),
                &mut self.sprite.x,
            ),
            (
                records_text(catalog, Key::OverworldRecordsYHex),
                &mut self.sprite.y,
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(&label);
                ui.text_edit_singleline(field);
            });
        }
        egui::ComboBox::from_id_salt("overworld-sprite-submap")
            .selected_text(SUBMAP_NAMES[self.sprite.submap.min(6)])
            .show_ui(ui, |ui| {
                for (index, name) in SUBMAP_NAMES.into_iter().enumerate() {
                    ui.selectable_value(&mut self.sprite.submap, index, name);
                }
            });
        ui.label(records_text(catalog, Key::OverworldRecordsUnownedExtension));
        ui.text_edit_singleline(&mut self.sprite.extra);
        let extra_len = world.shape.sprite_record_len.saturating_sub(7);
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui
                .button(records_text(catalog, Key::OverworldRecordsCopySprite))
                .clicked()
            {
                match native_clipboard::encode_overworld_sprite(&entries[self.sprite_index]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui
                .button(records_text(catalog, Key::OverworldRecordsPasteSprite))
                .clicked()
            {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        if let Some(error) = copy_error {
            return Some(Err(error));
        }
        if let Some(text) = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        }) {
            return Some(
                native_clipboard::decode_overworld_sprite(&text).map(|sprite| {
                    OverworldControllerEdit::ReplaceSprite {
                        index: self.sprite_index,
                        sprite,
                    }
                }),
            );
        }
        ui.button(records_text(catalog, Key::OverworldRecordsApplySprite))
            .clicked()
            .then(|| {
                self.sprite
                    .parse(extra_len)
                    .map(|sprite| OverworldControllerEdit::ReplaceSprite {
                        index: self.sprite_index,
                        sprite,
                    })
            })
    }
}

fn records_text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn complete_overworld_records_panel_has_no_literal_widget_text() {
        let source = include_str!("overworld_editor_records.rs");
        for literal_widget in [
            "ui.button(\"",
            "ui.label(\"",
            "ui.small(\"",
            ".text(\"",
            ".prefix(\"",
        ] {
            assert!(
                !source.contains(literal_widget),
                "overworld records panel bypasses typed localization with {literal_widget}"
            );
        }
        for key in Key::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("OverworldRecords"))
        {
            assert!(
                source.contains(&format!("Key::{key:?}")),
                "overworld records panel does not consume {key:?}"
            );
        }
    }
}
