use crate::level_editor_forms::EntranceForm;
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};

pub(super) fn entrance_fields(
    ui: &mut egui::Ui,
    form: &mut EntranceForm,
    catalog: Option<&LocalizationCatalog>,
) {
    let kinds = [
        text(catalog, Key::LevelCoreMain),
        text(catalog, Key::LevelCoreMidway),
        text(catalog, Key::LevelCoreSecondary),
    ];
    egui::ComboBox::from_id_salt("level-entrance-kind")
        .selected_text(&kinds[form.kind.min(2)])
        .show_ui(ui, |ui| {
            for (index, label) in kinds.iter().enumerate() {
                ui.selectable_value(&mut form.kind, index, label);
            }
        });
    for (label, field) in [
        (text(catalog, Key::LevelCoreX), &mut form.x),
        (text(catalog, Key::LevelCoreY), &mut form.y),
        (text(catalog, Key::LevelCoreScreen), &mut form.screen),
        (text(catalog, Key::LevelCoreAction), &mut form.action),
        (text(catalog, Key::LevelCoreRawFlags), &mut form.flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(&label);
            ui.text_edit_singleline(field);
        });
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}
