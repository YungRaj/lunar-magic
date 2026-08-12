use crate::overworld_appearance_editor_forms::PartForm;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};

pub(super) fn text_field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

pub(super) fn part_value_fields(
    ui: &mut egui::Ui,
    form: &mut PartForm,
    catalog: Option<&LocalizationCatalog>,
) {
    for (label, field) in [
        (
            text(catalog, Key::AppearanceTileIndexHex),
            &mut form.tile_index,
        ),
        (
            text(catalog, Key::AppearanceXOffsetDecimal),
            &mut form.x_offset,
        ),
        (
            text(catalog, Key::AppearanceYOffsetDecimal),
            &mut form.y_offset,
        ),
    ] {
        text_field(ui, &label, field);
    }
    ui.add(
        egui::Slider::new(&mut form.palette_index, 0..=7)
            .text(text(catalog, Key::AppearancePaletteRow)),
    );
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut form.x_flip,
            text(catalog, Key::AppearanceHorizontalFlip),
        );
        ui.checkbox(&mut form.y_flip, text(catalog, Key::AppearanceVerticalFlip));
    });
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}
