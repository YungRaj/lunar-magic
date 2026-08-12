use eframe::egui;
use lm_app::{ExtendedUiTextKey as Key, LocalizationCatalog};
use lm_graphics::PaletteEntryOwner;

pub(super) fn show(
    ui: &mut egui::Ui,
    owner: Option<PaletteEntryOwner>,
    catalog: Option<&LocalizationCatalog>,
) -> bool {
    match owner {
        Some(PaletteEntryOwner::Editable) => {
            ui.label(super::text(
                catalog,
                Key::NativeAssetsPaletteOwnershipEditable,
            ));
            true
        }
        Some(PaletteEntryOwner::Fixed) => {
            ui.label(super::text(catalog, Key::NativeAssetsPaletteOwnershipFixed));
            false
        }
        Some(PaletteEntryOwner::ExAnimation { record }) => {
            ui.label(
                super::text(catalog, Key::NativeAssetsPaletteOwnershipExAnimationFormat)
                    .replace("{record}", &format!("{record:04X}")),
            );
            false
        }
        None => {
            ui.label(super::text(
                catalog,
                Key::NativeAssetsPaletteOwnershipInvalid,
            ));
            false
        }
    }
}
