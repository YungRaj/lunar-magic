use crate::path_editor_forms::{DIRECTION_NAMES, EdgeForm, NodeForm, PATH_SUBMAP_NAMES};
use eframe::egui;

pub(super) fn node_fields(ui: &mut egui::Ui, form: &mut NodeForm) {
    for (label, field) in [
        ("Stable ID (hex)", &mut form.id),
        ("X (hex)", &mut form.x),
        ("Y (hex)", &mut form.y),
        ("Level (hex, blank = none)", &mut form.level),
        ("Raw flags (hex)", &mut form.raw_flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
    submap_combo(ui, &mut form.submap, "path-node-submap");
}

pub(super) fn edge_fields(ui: &mut egui::Ui, form: &mut EdgeForm) {
    for (label, field) in [
        ("From node (hex)", &mut form.from),
        ("To node (hex)", &mut form.to),
        ("Exit (hex, blank = none)", &mut form.exit),
        ("Raw flags (hex)", &mut form.raw_flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.text_edit_singleline(field);
        });
    }
    egui::ComboBox::from_id_salt("path-edge-direction")
        .selected_text(DIRECTION_NAMES[form.direction.min(3)])
        .show_ui(ui, |ui| {
            for (index, name) in DIRECTION_NAMES.into_iter().enumerate() {
                ui.selectable_value(&mut form.direction, index, name);
            }
        });
    if ui
        .checkbox(&mut form.one_way, "Deliberately one-way")
        .changed()
        && form.one_way
    {
        form.reciprocal = false;
    }
    if ui
        .checkbox(
            &mut form.reciprocal,
            "Apply/remove reciprocal pair atomically",
        )
        .changed()
        && form.reciprocal
    {
        form.one_way = false;
        if form.reverse_raw_flags.trim().is_empty() {
            form.reverse_raw_flags = "00".into();
        }
    }
    if form.reciprocal {
        for (label, field) in [
            ("Reverse exit (hex, blank = none)", &mut form.reverse_exit),
            ("Reverse raw flags (hex)", &mut form.reverse_raw_flags),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(field);
            });
        }
    }
}

fn submap_combo(ui: &mut egui::Ui, value: &mut usize, id: &str) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(PATH_SUBMAP_NAMES[(*value).min(6)])
        .show_ui(ui, |ui| {
            for (index, name) in PATH_SUBMAP_NAMES.into_iter().enumerate() {
                ui.selectable_value(value, index, name);
            }
        });
}
