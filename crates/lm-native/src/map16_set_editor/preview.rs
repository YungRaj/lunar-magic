use super::Map16SetEditor;
use crate::map16_editor_render;
use eframe::egui;
use lm_level::Map16PageFile;

impl Map16SetEditor {
    pub(super) fn page_view(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = &self.texture else {
            ui.label("Preview unavailable");
            return;
        };
        let response = ui.add(egui::Image::new(texture).sense(egui::Sense::click()));
        if response.clicked()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(tile) = map16_editor_render::selected_tile(response.rect, position)
        {
            self.tile = tile;
            self.loaded_key = None;
        }
        let column = self.tile % 16;
        let row = self.tile / 16;
        let cell = response.rect.width() / 16.0;
        let minimum = response.rect.min
            + egui::vec2(
                f32::from(u8::try_from(column).unwrap_or(0)) * cell,
                f32::from(u8::try_from(row).unwrap_or(0)) * cell,
            );
        ui.painter().rect_stroke(
            egui::Rect::from_min_size(minimum, egui::Vec2::splat(cell)),
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }

    pub(super) fn refresh_texture(&mut self, context: &egui::Context) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let key = (document.controller.revision(), self.page);
        if self.rendered_key == Some(key) {
            return;
        }
        let Some(page) = document
            .controller
            .value()
            .set
            .pages
            .get(self.page)
            .cloned()
        else {
            self.texture = None;
            return;
        };
        match map16_editor_render::render_texture(
            context,
            &Map16PageFile {
                source_page: u16::try_from(self.page).unwrap_or(u16::MAX),
                page,
            },
            &document.graphics,
            &document.palette,
        ) {
            Ok(texture) => {
                self.texture = Some(texture);
                self.rendered_key = Some(key);
            }
            Err(error) => self.error = Some(error),
        }
    }
}
