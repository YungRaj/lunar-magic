use eframe::egui;
use lm_app::{AppState, LocalizationCatalog, UiTextKey};

use crate::frontend_ui::localized_text;

const ORIGINAL_DIALOG_ID: u16 = 0x041f;

#[derive(Default)]
pub(super) struct UndoHistorySettings {
    open: bool,
    draft: usize,
}

impl UndoHistorySettings {
    pub(super) fn open(&mut self, current: usize) {
        self.draft = current;
        self.open = true;
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut accepted = None;
        let mut open = self.open;
        let mut close = false;
        egui::Window::new(dialog_title(catalog))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(dialog_control_text(
                    catalog,
                    0x66,
                    UiTextKey::UndoHistorySnapshotsLabel,
                ));
                ui.add(
                    egui::DragValue::new(&mut self.draft)
                        .range(0..=AppState::MAX_UNDO_SNAPSHOT_LIMIT),
                );
                ui.small(localized_text(catalog, UiTextKey::UndoHistoryHint));
                ui.horizontal(|ui| {
                    if ui
                        .button(dialog_control_text(catalog, 1, UiTextKey::CommonApply))
                        .clicked()
                    {
                        accepted = Some(self.draft);
                        close = true;
                    }
                    if ui
                        .button(dialog_control_text(catalog, 2, UiTextKey::CommonCancel))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        self.open = open && !close;
        accepted
    }
}

fn dialog_title(catalog: Option<&LocalizationCatalog>) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_title(ORIGINAL_DIALOG_ID))
        .map(str::to_owned)
        .unwrap_or_else(|| localized_text(catalog, UiTextKey::UndoHistoryWindowTitle))
}

fn dialog_control_text(
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: UiTextKey,
) -> String {
    catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .map(str::to_owned)
        .unwrap_or_else(|| localized_text(catalog, fallback))
}

pub(super) fn encode_preference(limit: usize) -> String {
    format!("v1:{limit}")
}

pub(super) fn decode_preference(encoded: &str) -> Result<usize, String> {
    let value = encoded
        .strip_prefix("v1:")
        .ok_or_else(|| "unknown undo-history preference version".to_owned())?
        .parse::<usize>()
        .map_err(|_| "undo-history preference is not an unsigned integer".to_owned())?;
    if value > AppState::MAX_UNDO_SNAPSHOT_LIMIT {
        return Err(format!(
            "undo-history preference exceeds {}",
            AppState::MAX_UNDO_SNAPSHOT_LIMIT
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::OriginalDialogTextKey;

    #[test]
    fn preference_round_trips_every_original_boundary_and_rejects_bad_values() {
        for value in [0, 1, 2, 33, 51] {
            assert_eq!(decode_preference(&encode_preference(value)).unwrap(), value);
        }
        assert!(decode_preference("33").is_err());
        assert!(decode_preference("v2:33").is_err());
        assert!(decode_preference("v1:-1").is_err());
        assert!(decode_preference("v1:52").is_err());
        assert!(decode_preference("v1:33:extra").is_err());
    }

    #[test]
    fn original_general_options_inventory_localizes_the_complete_undo_form() {
        let catalog = LocalizationCatalog::new(
            "fr-FR",
            UiTextKey::ALL.map(|key| (key, key.english().into())),
        )
        .unwrap()
        .with_original_dialog_texts([
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: u16::MAX,
                    control_id: u32::MAX,
                },
                "Options générales".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 22,
                    control_id: 0x66,
                },
                "Nombre maximal d’annulations (0-50)".into(),
            ),
            (
                OriginalDialogTextKey {
                    dialog_id: ORIGINAL_DIALOG_ID,
                    item_index: 0,
                    control_id: 1,
                },
                "Valider".into(),
            ),
        ])
        .unwrap();

        assert_eq!(dialog_title(Some(&catalog)), "Options générales");
        assert_eq!(
            dialog_control_text(Some(&catalog), 0x66, UiTextKey::UndoHistorySnapshotsLabel),
            "Nombre maximal d’annulations (0-50)"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 1, UiTextKey::CommonApply),
            "Valider"
        );
        assert_eq!(
            dialog_control_text(Some(&catalog), 2, UiTextKey::CommonCancel),
            UiTextKey::CommonCancel.english()
        );
    }
}
