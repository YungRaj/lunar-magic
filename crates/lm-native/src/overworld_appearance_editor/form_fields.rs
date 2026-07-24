use crate::overworld_appearance_editor_forms::PartForm;
use eframe::egui;

pub(super) fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

pub(super) fn part_value_fields(ui: &mut egui::Ui, form: &mut PartForm) {
    for (label, field) in [
        ("Tile index (hex)", &mut form.tile_index),
        ("X offset (decimal)", &mut form.x_offset),
        ("Y offset (decimal)", &mut form.y_offset),
    ] {
        text_field(ui, label, field);
    }
    ui.add(egui::Slider::new(&mut form.palette_index, 0..=7).text("Palette row"));
    ui.horizontal(|ui| {
        ui.checkbox(&mut form.x_flip, "Horizontal flip");
        ui.checkbox(&mut form.y_flip, "Vertical flip");
    });
}
