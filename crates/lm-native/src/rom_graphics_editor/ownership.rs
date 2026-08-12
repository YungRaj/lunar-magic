use eframe::egui;
use lm_app::{ExtendedUiTextKey, LocalizationCatalog};
use lm_graphics::GraphicsTileOwner;

pub(super) fn show(
    ui: &mut egui::Ui,
    owner: Option<GraphicsTileOwner>,
    catalog: Option<&LocalizationCatalog>,
) -> bool {
    match owner {
        Some(GraphicsTileOwner::Editable) => {
            ui.label(super::text(
                catalog,
                ExtendedUiTextKey::GraphicsOwnershipEditable,
            ));
        }
        Some(GraphicsTileOwner::Fixed) => {
            ui.label(super::text(
                catalog,
                ExtendedUiTextKey::GraphicsOwnershipFixed,
            ));
        }
        Some(GraphicsTileOwner::ExAnimation { record }) => {
            ui.label(
                super::text(
                    catalog,
                    ExtendedUiTextKey::GraphicsOwnershipExAnimationFormat,
                )
                .replace("{record}", &format!("{record:04X}")),
            );
        }
        Some(GraphicsTileOwner::OriginalAnimation { slot }) => {
            ui.label(
                super::text(
                    catalog,
                    ExtendedUiTextKey::GraphicsOwnershipOriginalAnimationFormat,
                )
                .replace("{slot}", &format!("{slot:02X}")),
            );
        }
        Some(GraphicsTileOwner::LevelExAnimation { slot }) => {
            ui.label(
                super::text(
                    catalog,
                    ExtendedUiTextKey::GraphicsOwnershipLevelExAnimationFormat,
                )
                .replace("{slot}", &format!("{slot:02X}")),
            );
        }
        Some(GraphicsTileOwner::GlobalExAnimation { slot }) => {
            ui.label(
                super::text(
                    catalog,
                    ExtendedUiTextKey::GraphicsOwnershipGlobalExAnimationFormat,
                )
                .replace("{slot}", &format!("{slot:02X}")),
            );
        }
        None => {
            ui.label(super::text(
                catalog,
                ExtendedUiTextKey::GraphicsOwnershipInvalid,
            ));
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
