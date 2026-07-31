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
        None => {
            ui.label("Ownership: invalid (read-only)");
        }
    }
    is_editable(owner)
}

const fn is_editable(owner: Option<GraphicsTileOwner>) -> bool {
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
            None,
        ] {
            assert!(!is_editable(owner));
        }
    }
}
