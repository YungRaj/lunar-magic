use super::{
    MainToolbarImageSet, OriginalTiledImage, OverworldEditor, Panel, tiled_surface_canvas_size,
};
use crate::{level_editor_forms, overworld_editor_render};
use eframe::egui;
use lm_app::LocalizationCatalog;
use lm_app::{OverworldControllerEdit, OverworldLayerId};
use lm_project::CompleteOverworldShape;

impl OverworldEditor {
    pub(super) fn world_view(&mut self, ui: &mut egui::Ui, toolbar_images: &MainToolbarImageSet) {
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
        let reveal_count = self.document.as_ref().map_or(0, |document| {
            document.controller.value().data.event_reveals.entries.len()
        });
        if ui
            .add(
                egui::Slider::new(&mut self.completed_reveals, 0..=reveal_count)
                    .text("Completed reveals"),
            )
            .changed()
        {
            self.rendered_key = None;
        }
        let Some(texture) = self.texture.clone() else {
            ui.label("Preview unavailable; property editing remains available.");
            return;
        };
        let available = ui.available_size();
        egui::ScrollArea::both().show(ui, |ui| {
            let image_size = texture.size_vec2();
            let canvas_size = tiled_surface_canvas_size(image_size, available);
            let (canvas_rect, response) = ui.allocate_exact_size(canvas_size, egui::Sense::click());
            let image_rect = egui::Rect::from_min_size(canvas_rect.min, image_size);
            let painter = ui.painter_at(canvas_rect);
            painter.rect_filled(canvas_rect, 0.0, egui::Color32::BLACK);
            toolbar_images.paint_tiled_surface(
                &painter,
                OriginalTiledImage::OverworldCanvas,
                canvas_rect,
                if image_size.x < image_size.y {
                    egui::pos2(image_rect.max.x, image_rect.min.y)
                } else {
                    egui::pos2(image_rect.min.x, image_rect.max.y)
                },
            );
            painter.image(
                texture.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            if response.clicked()
                && let Some(position) = response.interact_pointer_pos()
                && image_rect.contains(position)
                && let Some(shape) = self.shape()
                && let Some(selected) = overworld_editor_render::selected_tile(
                    image_rect,
                    position,
                    shape.width,
                    shape.height,
                )
            {
                self.selected = selected;
                self.loaded_tile = None;
            }
        });
    }

    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        ui.heading("Tilemap");
        ui.label(format!(
            "Coordinate {}, {}",
            self.selected.0, self.selected.1
        ));
        ui.horizontal(|ui| {
            ui.label("Map16 tile (hex)");
            ui.text_edit_singleline(&mut self.tile_value);
        });
        if ui.button("Apply tile").clicked() {
            match level_editor_forms::parse_hex_u16(&self.tile_value, "overworld Map16 tile") {
                Ok(tile) => self.apply_edit(&OverworldControllerEdit::SetLayerTile {
                    layer: self.layer(),
                    x: self.selected.0,
                    y: self.selected.1,
                    tile,
                }),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.panel, Panel::Records, "Records");
            ui.selectable_value(&mut self.panel, Panel::Palette, "Palette");
            ui.selectable_value(&mut self.panel, Panel::Animation, "Animation");
        });
        ui.separator();
        self.active_panel(ui, catalog);
    }

    fn active_panel(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let revision = document.controller.revision();
        let animation_ownership = crate::overworld_editor_render::overworld_animation_ownership(
            &document.controller.value().data.animation,
            None,
            crate::overworld_editor_render::OverworldAnimationOptions::VANILLA_ENABLED,
            0,
            document.controller.value().data.palette.colors.len(),
        );
        let result = match self.panel {
            Panel::Records => self.records.show(ui, document.controller.value(), revision),
            Panel::Palette => self.palette.show(
                ui,
                &document.controller.value().data.palette,
                &document.ownership,
                &animation_ownership.palette,
            ),
            Panel::Animation => self.animation.show(
                ui,
                &document.controller.value().data.animation,
                None,
                &document.modes,
                revision,
                catalog,
            ),
        };
        if let Some(owner) = self.palette.take_navigation() {
            self.panel = Panel::Animation;
            self.animation.navigate(owner);
        }
        if let Some(result) = result {
            match result {
                Ok(edit) => self.apply_edit(&edit),
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn apply_edit(&mut self, edit: &OverworldControllerEdit) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let source_slot = usize::from(document.controller.value().source_slot);
        if let Err(error) = document.controller.apply_edits(
            document.controller.revision(),
            source_slot,
            &document.ownership,
            std::slice::from_ref(edit),
        ) {
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
        let shape = document.controller.value().shape;
        if self.selected.0 >= shape.width || self.selected.1 >= shape.height {
            self.selected = (0, 0);
        }
        let layer = if self.edit_layer == 0 {
            &document.controller.value().data.layers.layer1
        } else {
            &document.controller.value().data.layers.layer2
        };
        let index = self.selected.1 * shape.width + self.selected.0;
        if let Some(tile) = layer.tiles.get(index) {
            self.tile_value = format!("{tile:04X}");
        }
        self.loaded_tile = Some(key);
    }

    fn shape(&self) -> Option<CompleteOverworldShape> {
        self.document
            .as_ref()
            .map(|document| document.controller.value().shape)
    }

    fn layer(&self) -> OverworldLayerId {
        if self.edit_layer == 0 {
            OverworldLayerId::Layer1
        } else {
            OverworldLayerId::Layer2
        }
    }
}
