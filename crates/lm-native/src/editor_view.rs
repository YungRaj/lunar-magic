use eframe::egui;
use lm_app::EditorMode;

pub(crate) fn show(ui: &mut egui::Ui, mode: EditorMode) {
    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.heading(title(mode));
        ui.add_space(12.0);
        ui.label(description(mode));
    });
}

fn title(mode: EditorMode) -> String {
    match mode {
        EditorMode::NoProject => "No ROM open".into(),
        EditorMode::Level(level) => format!("Level {level:03X}"),
        EditorMode::Overworld => "Overworld Editor".into(),
        EditorMode::Map16 => "Map16 Editor".into(),
        EditorMode::Graphics(file) => format!("Graphics {file:03X}"),
        EditorMode::Palette(palette) => format!("Palette {palette:03X}"),
        EditorMode::ExAnimation(slot) => format!("ExAnimation {slot:03X}"),
        EditorMode::Layer3(level) => format!("Layer 3 — Level {level:03X}"),
    }
}

fn description(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::NoProject => "Open a supported Super Mario World ROM to begin.",
        EditorMode::Level(_) => {
            "The native level canvas will consume the shared renderer snapshot."
        }
        EditorMode::Overworld => {
            "The overworld model and renderer are available through the shared application boundary."
        }
        EditorMode::Map16 => "Map16 editing uses the transactional workspace controller.",
        EditorMode::Graphics(_) => "Graphics editing uses ownership-aware atomic tile operations.",
        EditorMode::Palette(_) => "Palette editing uses ownership-aware atomic color operations.",
        EditorMode::ExAnimation(_) => {
            "ExAnimation editing retains revision-specific frame interpretation."
        }
        EditorMode::Layer3(_) => {
            "Layer 3 editing preserves unrecovered bytes and provider-resolved previews."
        }
    }
}
