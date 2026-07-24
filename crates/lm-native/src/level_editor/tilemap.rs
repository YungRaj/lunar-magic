use super::LevelEditor;
use crate::{level_editor_forms, level_editor_render};
use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
use lm_level::{LayerDimensions, LevelLayer, LevelPropertyEdit, TileCoordinate};

impl LevelEditor {
    pub(super) fn level_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .selectable_value(&mut self.edit_layer, 0, "Layer 1")
                .clicked()
                || ui
                    .selectable_value(&mut self.edit_layer, 1, "Layer 2")
                    .clicked()
            {
                self.loaded_tile = None;
            }
        });
        let Some(texture) = self.texture.clone() else {
            ui.label("Preview unavailable; editing remains available.");
            return;
        };
        egui::ScrollArea::both().show(ui, |ui| {
            let response = ui.add(egui::Image::new(&texture).sense(egui::Sense::click()));
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
            {
                let dimensions = self.active_dimensions();
                let canvas = self.canvas_dimensions();
                if let Some(coordinate) = level_editor_render::selected_coordinate(
                    response.rect,
                    position,
                    canvas.width,
                    canvas.height,
                ) && coordinate.0 < dimensions.width
                    && coordinate.1 < dimensions.height
                {
                    self.selected = coordinate;
                    self.loaded_tile = None;
                }
            }
        });
    }

    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tilemap");
        ui.label(format!(
            "Coordinate {}, {}",
            self.selected.0, self.selected.1
        ));
        ui.horizontal(|ui| {
            ui.label("Map16 tile (hex)");
            ui.text_edit_singleline(&mut self.tile_value);
        });
        let dimensions = self.active_dimensions();
        if ui
            .add_enabled(
                dimensions.width > 0 && dimensions.height > 0,
                egui::Button::new("Apply tile"),
            )
            .clicked()
        {
            self.apply_tile();
        }
        ui.separator();
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let result = self.panels.show(
            ui,
            document.controller.value(),
            document.controller.revision(),
        );
        if let Some(result) = result {
            match result {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn apply_tile(&mut self) {
        match level_editor_forms::parse_hex_u16(&self.tile_value, "Map16 tile") {
            Ok(tile) => {
                let dimensions = self.active_dimensions();
                self.apply_edits(&[CompleteLevelDocumentEdit::Property(
                    LevelPropertyEdit::SetTile {
                        layer: self.active_layer(),
                        dimensions,
                        coordinate: TileCoordinate {
                            x: self.selected.0,
                            y: self.selected.1,
                        },
                        tile,
                    },
                )]);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn apply_edits(&mut self, edits: &[CompleteLevelDocumentEdit]) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Err(error) = document
            .controller
            .apply_edits(document.controller.revision(), edits)
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    pub(super) fn load_tile(&mut self) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let key = (
            document.controller.revision(),
            self.edit_layer,
            self.selected.0,
            self.selected.1,
        );
        if self.loaded_tile == Some(key) {
            return;
        }
        let dimensions = self.active_dimensions();
        if self.selected.0 >= dimensions.width || self.selected.1 >= dimensions.height {
            self.selected = (0, 0);
        }
        let index = self.selected.1 * dimensions.width + self.selected.0;
        let tiles = if self.edit_layer == 0 {
            &document.controller.value().0.layer1.raw_tilemap
        } else {
            &document.controller.value().0.layer2.raw_tilemap
        };
        if let Some(tile) = tiles.get(index) {
            self.tile_value = format!("{tile:04X}");
        }
        self.loaded_tile = Some(key);
    }

    fn active_dimensions(&self) -> LayerDimensions {
        let Some(document) = self.document.as_ref() else {
            return LayerDimensions {
                width: 1,
                height: 1,
            };
        };
        if self.edit_layer == 0 {
            LayerDimensions {
                width: document.dimensions.layer1_width,
                height: document.dimensions.layer1_height,
            }
        } else {
            LayerDimensions {
                width: document.dimensions.layer2_width,
                height: document.dimensions.layer2_height,
            }
        }
    }

    fn active_layer(&self) -> LevelLayer {
        if self.edit_layer == 0 {
            LevelLayer::Layer1
        } else {
            LevelLayer::Layer2
        }
    }

    fn canvas_dimensions(&self) -> LayerDimensions {
        let Some(document) = self.document.as_ref() else {
            return LayerDimensions {
                width: 0,
                height: 0,
            };
        };
        LayerDimensions {
            width: document
                .dimensions
                .layer1_width
                .max(document.dimensions.layer2_width),
            height: document
                .dimensions
                .layer1_height
                .max(document.dimensions.layer2_height),
        }
    }
}
