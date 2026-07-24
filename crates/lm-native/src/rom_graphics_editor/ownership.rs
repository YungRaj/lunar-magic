use eframe::egui;
use lm_graphics::GraphicsTileOwner;

pub(super) fn show(ui: &mut egui::Ui, owner: Option<GraphicsTileOwner>) -> bool {
    match owner {
        Some(GraphicsTileOwner::Editable) => {
            ui.label("Ownership: editable");
            true
        }
        Some(GraphicsTileOwner::Fixed) => {
            ui.label("Ownership: fixed (read-only)");
            false
        }
        Some(GraphicsTileOwner::ExAnimation { record }) => {
            ui.label(format!(
                "Ownership: ExAnimation record {record:04X} (read-only)"
            ));
            false
        }
        None => {
            ui.label("Ownership: invalid (read-only)");
            false
        }
    }
}
