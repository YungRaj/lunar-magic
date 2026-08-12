use crate::path_editor_forms::{EdgeForm, NodeForm};
use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};

pub(super) fn node_fields(
    ui: &mut egui::Ui,
    form: &mut NodeForm,
    catalog: Option<&LocalizationCatalog>,
) {
    for (label, field) in [
        (Key::PathEditorStableId, &mut form.id),
        (Key::PathEditorX, &mut form.x),
        (Key::PathEditorY, &mut form.y),
        (Key::PathEditorLevel, &mut form.level),
        (Key::PathEditorRawFlags, &mut form.raw_flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(text(catalog, label));
            ui.text_edit_singleline(field);
        });
    }
    submap_combo(ui, &mut form.submap, "path-node-submap", catalog);
}

pub(super) fn edge_fields(
    ui: &mut egui::Ui,
    form: &mut EdgeForm,
    catalog: Option<&LocalizationCatalog>,
) {
    for (label, field) in [
        (Key::PathEditorFromNode, &mut form.from),
        (Key::PathEditorToNode, &mut form.to),
        (Key::PathEditorExit, &mut form.exit),
        (Key::PathEditorRawFlags, &mut form.raw_flags),
    ] {
        ui.horizontal(|ui| {
            ui.label(text(catalog, label));
            ui.text_edit_singleline(field);
        });
    }
    egui::ComboBox::from_id_salt("path-edge-direction")
        .selected_text(direction_names(catalog)[form.direction.min(3)].clone())
        .show_ui(ui, |ui| {
            for (index, name) in direction_names(catalog).into_iter().enumerate() {
                ui.selectable_value(&mut form.direction, index, name);
            }
        });
    if ui
        .checkbox(&mut form.one_way, text(catalog, Key::PathEditorOneWay))
        .changed()
        && form.one_way
    {
        form.reciprocal = false;
    }
    if ui
        .checkbox(
            &mut form.reciprocal,
            text(catalog, Key::PathEditorReciprocalPair),
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
            (Key::PathEditorReverseExit, &mut form.reverse_exit),
            (Key::PathEditorReverseRawFlags, &mut form.reverse_raw_flags),
        ] {
            ui.horizontal(|ui| {
                ui.label(text(catalog, label));
                ui.text_edit_singleline(field);
            });
        }
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: Key) -> String {
    catalog.map_or_else(
        || key.english().to_owned(),
        |catalog| catalog.extended_text(key).to_owned(),
    )
}

fn submap_combo(
    ui: &mut egui::Ui,
    value: &mut usize,
    id: &str,
    catalog: Option<&LocalizationCatalog>,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(submap_names(catalog)[(*value).min(6)].clone())
        .show_ui(ui, |ui| {
            for (index, name) in submap_names(catalog).into_iter().enumerate() {
                ui.selectable_value(value, index, name);
            }
        });
}

fn direction_names(catalog: Option<&LocalizationCatalog>) -> [String; 4] {
    [
        Key::PathEditorDirectionUp,
        Key::PathEditorDirectionRight,
        Key::PathEditorDirectionDown,
        Key::PathEditorDirectionLeft,
    ]
    .map(|key| text(catalog, key))
}

fn submap_names(catalog: Option<&LocalizationCatalog>) -> [String; 7] {
    [
        Key::MetadataSubmapMain,
        Key::MetadataSubmapYoshiIsland,
        Key::MetadataSubmapVanillaDome,
        Key::MetadataSubmapForestIllusion,
        Key::MetadataSubmapValleyBowser,
        Key::MetadataSubmapSpecialWorld,
        Key::MetadataSubmapStarWorld,
    ]
    .map(|key| text(catalog, key))
}
