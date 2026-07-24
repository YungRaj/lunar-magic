use crate::level_editor_forms::EntranceForm;
use eframe::egui;

pub(super) fn entrance_fields(ui: &mut egui::Ui, form: &mut EntranceForm) {
    egui::ComboBox::from_id_salt("level-entrance-kind")
        .selected_text(["Main", "Midway", "Secondary"][form.kind.min(2)])
        .show_ui(ui, |ui| {
            for (index, label) in ["Main", "Midway", "Secondary"].into_iter().enumerate() {
                ui.selectable_value(&mut form.kind, index, label);
            }
        });
    for (label, field) in [
        ("X (hex)", &mut form.x),
        ("Y (hex)", &mut form.y),
        ("Screen (hex)", &mut form.screen),
        ("Action (hex)", &mut form.action),
        ("Raw flags (hex)", &mut form.flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
}
