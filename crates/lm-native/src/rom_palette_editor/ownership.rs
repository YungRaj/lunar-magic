use eframe::egui;
use lm_graphics::PaletteEntryOwner;

pub(super) fn show(ui: &mut egui::Ui, owner: Option<PaletteEntryOwner>) -> bool {
    match owner {
        Some(PaletteEntryOwner::Editable) => {
            ui.label("Ownership: editable");
            true
        }
        Some(PaletteEntryOwner::Fixed) => {
            ui.label("Ownership: fixed (read-only)");
            false
        }
        Some(PaletteEntryOwner::ExAnimation { record }) => {
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
