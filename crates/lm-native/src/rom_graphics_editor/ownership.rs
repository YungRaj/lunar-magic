use eframe::egui;
use lm_graphics::GraphicsTileOwner;

pub(super) fn show(ui: &mut egui::Ui, owner: Option<GraphicsTileOwner>) -> bool {
    match owner {
        Some(GraphicsTileOwner::Editable) => {
            ui.label("Ownership: editable");
        }
        Some(GraphicsTileOwner::Fixed) => {
            ui.label("Ownership: fixed (read-only)");
        }
        Some(GraphicsTileOwner::ExAnimation { record }) => {
            ui.label(format!(
                "Ownership: ExAnimation record {record:04X} (read-only)"
            ));
        }
        Some(GraphicsTileOwner::OriginalAnimation { slot }) => {
            ui.label(format!(
                "Ownership: original animation slot {slot:02X} (read-only)"
            ));
        }
        Some(GraphicsTileOwner::LevelExAnimation { slot }) => {
            ui.label(format!(
                "Ownership: level ExAnimation slot {slot:02X} (read-only)"
            ));
        }
        Some(GraphicsTileOwner::GlobalExAnimation { slot }) => {
            ui.label(format!(
                "Ownership: global ExAnimation slot {slot:02X} (read-only)"
            ));
        }
        None => {
            ui.label("Ownership: invalid (read-only)");
        }
    }
    is_editable(owner)
}

pub(super) const fn is_editable(owner: Option<GraphicsTileOwner>) -> bool {
    matches!(owner, Some(GraphicsTileOwner::Editable))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicitly_editable_tiles_admit_pixel_and_flip_mutations() {
        assert!(is_editable(Some(GraphicsTileOwner::Editable)));
        for owner in [
            Some(GraphicsTileOwner::Fixed),
            Some(GraphicsTileOwner::ExAnimation { record: 3 }),
            Some(GraphicsTileOwner::OriginalAnimation { slot: 2 }),
            Some(GraphicsTileOwner::LevelExAnimation { slot: 3 }),
            Some(GraphicsTileOwner::GlobalExAnimation { slot: 4 }),
            None,
        ] {
            assert!(!is_editable(owner));
        }
    }
}
