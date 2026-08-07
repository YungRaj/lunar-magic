use crate::{
    level_editor_forms, native_clipboard,
    overworld_editor_forms::{EndpointForm, RevealForm, SUBMAP_NAMES, SpriteForm},
};
use eframe::egui;
use lm_app::OverworldControllerEdit;
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
    ) -> Option<Result<OverworldControllerEdit, String>> {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, RecordPanel::Reveals, "Reveals");
            ui.selectable_value(&mut self.panel, RecordPanel::Endpoints, "Endpoints");
            ui.selectable_value(&mut self.panel, RecordPanel::Messages, "Messages");
            ui.selectable_value(&mut self.panel, RecordPanel::Sprites, "Sprites");
        });
        ui.separator();
        match self.panel {
            RecordPanel::Reveals => self.show_reveals(ui, world, revision),
            RecordPanel::Endpoints => self.show_endpoints(ui, world, revision),
            RecordPanel::Messages => self.show_messages(ui, world, revision),
            RecordPanel::Sprites => self.show_sprites(ui, world, revision),
        }
    }

    fn show_reveals(
        &mut self,
        ui: &mut egui::Ui,
        world: &CompleteOverworldFile,
        revision: u64,
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.event_reveals.entries;
        if entries.is_empty() {
            ui.label("This fixed-shape document contains no event reveals.");
            return None;
        }
        self.reveal_index = self.reveal_index.min(entries.len() - 1);
        ui.add(egui::Slider::new(&mut self.reveal_index, 0..=entries.len() - 1).text("Reveal"));
        let key = (revision, self.reveal_index);
        if self.reveal_key != Some(key) {
            self.reveal = RevealForm::load(entries[self.reveal_index]);
            self.reveal_key = Some(key);
        }
        for (label, field) in [
            ("Source tile (hex)", &mut self.reveal.source),
            ("Destination tile (hex)", &mut self.reveal.destination),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
        if ui.button("Apply reveal").clicked() {
            return Some(self.reveal.parse().map(|reveal| {
                OverworldControllerEdit::ReplaceEventReveal {
                    index: self.reveal_index,
                    reveal,
                }
            }));
        }
        ui.separator();
        ui.label("Move event-tile selection");
        let maximum = entries.len() - 1;
        self.reveal_selection_start = self.reveal_selection_start.min(maximum);
        self.reveal_selection_end = self.reveal_selection_end.min(maximum);
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.reveal_selection_start)
                    .range(0..=maximum)
                    .prefix("First "),
            );
            ui.add(
                egui::DragValue::new(&mut self.reveal_selection_end)
                    .range(0..=maximum)
                    .prefix("Last "),
            );
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.reveal_delta_x)
                    .range(-63..=63)
                    .prefix("X tiles "),
            );
            ui.add(
                egui::DragValue::new(&mut self.reveal_delta_y)
                    .range(-127..=127)
                    .prefix("Y tiles "),
            );
        });
        ui.small(
            "The complete selection uses Lunar Magic's seam-aware shared displacement and 6x6 footprint bounds.",
        );
        ui.button("Move selected event tiles").clicked().then(|| {
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
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.endpoints;
        if entries.is_empty() {
            ui.label("This fixed-shape document contains no endpoints.");
            return None;
        }
        self.endpoint_index = self.endpoint_index.min(entries.len() - 1);
        ui.add(egui::Slider::new(&mut self.endpoint_index, 0..=entries.len() - 1).text("Endpoint"));
        let key = (revision, self.endpoint_index);
        if self.endpoint_key != Some(key) {
            self.endpoint = EndpointForm::load(entries[self.endpoint_index]);
            self.endpoint_key = Some(key);
        }
        for (label, field) in [
            ("X (hex)", &mut self.endpoint.x),
            ("Y (hex)", &mut self.endpoint.y),
            ("Submap (hex)", &mut self.endpoint.submap),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
        ui.button("Apply endpoint").clicked().then(|| {
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
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.messages;
        if entries.is_empty() {
            ui.label("This fixed-shape document contains no messages.");
            return None;
        }
        self.message = self.message.min(entries.len() - 1);
        ui.add(egui::Slider::new(&mut self.message, 0..=entries.len() - 1).text("Message"));
        ui.add(egui::Slider::new(&mut self.message_column, 0..=17).text("Column"));
        ui.add(egui::Slider::new(&mut self.message_row, 0..=7).text("Row"));
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
            ui.label("Tile (hex)");
            ui.text_edit_singleline(&mut self.message_tile);
        });
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Copy message").clicked() {
                match native_clipboard::encode_overworld_message(&entries[self.message]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste message").clicked() {
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
        ui.button("Apply message tile").clicked().then(|| {
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
    ) -> Option<Result<OverworldControllerEdit, String>> {
        let entries = &world.data.sprites;
        if entries.is_empty() {
            ui.label("This fixed-shape document contains no sprites.");
            return None;
        }
        self.sprite_index = self.sprite_index.min(entries.len() - 1);
        ui.add(egui::Slider::new(&mut self.sprite_index, 0..=entries.len() - 1).text("Sprite"));
        let key = (revision, self.sprite_index);
        if self.sprite_key != Some(key) {
            self.sprite = SpriteForm::load(&entries[self.sprite_index]);
            self.sprite_key = Some(key);
        }
        for (label, field) in [
            ("ID (hex)", &mut self.sprite.id),
            ("X (hex)", &mut self.sprite.x),
            ("Y (hex)", &mut self.sprite.y),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
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
        ui.label("Unowned extension bytes:");
        ui.text_edit_singleline(&mut self.sprite.extra);
        let extra_len = world.shape.sprite_record_len.saturating_sub(7);
        let mut copy_error = None;
        ui.horizontal(|ui| {
            if ui.button("Copy sprite").clicked() {
                match native_clipboard::encode_overworld_sprite(&entries[self.sprite_index]) {
                    Ok(text) => ui.ctx().copy_text(text),
                    Err(error) => copy_error = Some(error),
                }
            }
            if ui.button("Paste sprite").clicked() {
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
        ui.button("Apply sprite").clicked().then(|| {
            self.sprite
                .parse(extra_len)
                .map(|sprite| OverworldControllerEdit::ReplaceSprite {
                    index: self.sprite_index,
                    sprite,
                })
        })
    }
}
