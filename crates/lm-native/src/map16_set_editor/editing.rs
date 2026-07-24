use super::Map16SetEditor;
use crate::map16_subtile_form;
use eframe::egui;
use lm_app::Map16DocumentEdit;
use lm_level::{Map16Address, Map16Page};
use map16_subtile_form::SubtileForm;

impl Map16SetEditor {
    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!("Address {:02X}:{:02X}", self.page, self.tile));
        egui::ComboBox::from_id_salt("map16-set-quadrant")
            .selected_text(map16_subtile_form::quadrant_name(self.quadrant))
            .show_ui(ui, |ui| {
                for index in 0..4 {
                    if ui
                        .selectable_value(
                            &mut self.quadrant,
                            index,
                            map16_subtile_form::quadrant_name(index),
                        )
                        .clicked()
                    {
                        self.loaded_key = None;
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.label("8×8 tile (hex)");
            ui.text_edit_singleline(&mut self.subtile.tile);
        });
        ui.add(egui::Slider::new(&mut self.subtile.palette, 0..=7).text("Palette"));
        ui.checkbox(&mut self.subtile.priority, "Priority");
        ui.checkbox(&mut self.subtile.x_flip, "Horizontal flip");
        ui.checkbox(&mut self.subtile.y_flip, "Vertical flip");
        if ui.button("Apply subtile").clicked() {
            match self.subtile.parse() {
                Ok(subtile) => self.apply_edit(&Map16DocumentEdit::SetSubtile {
                    address: Map16Address {
                        page: self.page,
                        tile: self.tile,
                    },
                    quadrant: map16_subtile_form::quadrant(self.quadrant),
                    subtile,
                    resolution_limit: self.resolution_limit(),
                }),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Acts Like (hex)");
            ui.text_edit_singleline(&mut self.acts_like);
        });
        if ui.button("Apply Acts Like").clicked() {
            match u16::from_str_radix(self.acts_like.trim(), 16) {
                Ok(acts_like) => self.apply_edit(&Map16DocumentEdit::SetActsLike {
                    address: Map16Address {
                        page: self.page,
                        tile: self.tile,
                    },
                    acts_like,
                    resolution_limit: self.resolution_limit(),
                }),
                Err(error) => self.error = Some(format!("invalid Acts Like value: {error}")),
            }
        }
    }

    pub(super) fn resolution_limit(&self) -> usize {
        self.document.as_ref().map_or(0, |document| {
            document
                .controller
                .value()
                .set
                .pages
                .len()
                .saturating_mul(Map16Page::TILE_COUNT)
        })
    }

    pub(super) fn apply_edit(&mut self, edit: &Map16DocumentEdit) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Err(error) = document
            .controller
            .apply_edits(document.controller.revision(), std::slice::from_ref(edit))
        {
            self.error = Some(error.to_string());
        } else {
            self.invalidate();
        }
    }

    pub(super) fn load_form(&mut self) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let key = (
            document.controller.revision(),
            self.page,
            self.tile,
            self.quadrant,
        );
        if self.loaded_key == Some(key) {
            return;
        }
        let Some(tile) = document
            .controller
            .value()
            .set
            .pages
            .get(self.page)
            .and_then(|page| page.tiles.get(self.tile))
        else {
            return;
        };
        self.subtile =
            SubtileForm::from_subtile(map16_subtile_form::quadrant_value(*tile, self.quadrant));
        self.acts_like = format!("{:04X}", tile.acts_like);
        self.loaded_key = Some(key);
    }
}
