use eframe::egui;
use lm_app::{AppState, LocalizationCatalog, UiTextKey};

use crate::frontend_ui::localized_text;

const ORIGINAL_DIALOG_ID: u16 = 0x041f;

#[derive(Default)]
pub(super) struct UndoHistorySettings {
    open: bool,
    draft: GeneralOptions,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct GeneralOptions {
    pub undo_limit: usize,
    pub mouse_gestures: bool,
    pub save_mouse_gestures: bool,
    pub maintain_checksum: bool,
    pub silently_add_header: bool,
    pub save_prompt: bool,
    pub joined_graphics: bool,
    pub gfx_bypass_lists: bool,
    pub prefer_past_2mb: bool,
    pub remember_window_size: bool,
    pub allow_control_wheel_zoom: bool,
    pub rom_file_name_in_title: bool,
    pub show_add_editor_ids: bool,
    pub auto_deselect: bool,
    pub correct_fatal_errors: bool,
    pub convert_berry_gfx: bool,
    pub scan_exits: bool,
    pub count_sprites: bool,
    pub check_object_placement: bool,
    pub warn_ips_sibling: bool,
    pub warn_vertical_fireball: bool,
}

impl UndoHistorySettings {
    pub(super) fn open(&mut self, current: GeneralOptions) {
        self.draft = current;
        self.open = true;
    }

    #[cfg(test)]
    pub(super) const fn is_open(&self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub(super) const fn draft(&self) -> GeneralOptions {
        self.draft
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<GeneralOptions> {
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
                    egui::DragValue::new(&mut self.draft.undo_limit)
                        .range(0..=AppState::MAX_UNDO_SNAPSHOT_LIMIT),
                );
                ui.small(localized_text(catalog, UiTextKey::UndoHistoryHint));
                ui.separator();
                option(
                    ui,
                    catalog,
                    0x2292,
                    "Mouse Gestures",
                    &mut self.draft.mouse_gestures,
                );
                ui.add_enabled_ui(self.draft.mouse_gestures, |ui| {
                    option(
                        ui,
                        catalog,
                        0x2293,
                        "Auto-Save on Mouse Gestures",
                        &mut self.draft.save_mouse_gestures,
                    );
                });
                ui.separator();
                option(
                    ui,
                    catalog,
                    0x22a2,
                    "Maintain ROM Checksum",
                    &mut self.draft.maintain_checksum,
                );
                option(
                    ui,
                    catalog,
                    0x22a7,
                    "Silently Add Header to ROM",
                    &mut self.draft.silently_add_header,
                );
                option(
                    ui,
                    catalog,
                    0x22a8,
                    "Save Prompt",
                    &mut self.draft.save_prompt,
                );
                option(
                    ui,
                    catalog,
                    0x22a4,
                    "Use Joined GFX Files",
                    &mut self.draft.joined_graphics,
                );
                option(
                    ui,
                    catalog,
                    0x2297,
                    "Standard GFX Bypass Dialogs",
                    &mut self.draft.gfx_bypass_lists,
                );
                option(
                    ui,
                    catalog,
                    0x22a6,
                    "Prefer Saving in 2MB+ ROM Area",
                    &mut self.draft.prefer_past_2mb,
                );
                ui.separator();
                option(
                    ui,
                    catalog,
                    0x2294,
                    "Remember Window Size",
                    &mut self.draft.remember_window_size,
                );
                option(
                    ui,
                    catalog,
                    0x2299,
                    "Allow Control + Mouse Wheel to Zoom",
                    &mut self.draft.allow_control_wheel_zoom,
                );
                option(
                    ui,
                    catalog,
                    0x229f,
                    "ROM File Name in Main Window Title Bar",
                    &mut self.draft.rom_file_name_in_title,
                );
                option(
                    ui,
                    catalog,
                    0x2296,
                    "Show ID in Add Object/Sprite Editors",
                    &mut self.draft.show_add_editor_ids,
                );
                option(
                    ui,
                    catalog,
                    0x2298,
                    "Auto-Deselect on Editor Select",
                    &mut self.draft.auto_deselect,
                );
                option(
                    ui,
                    catalog,
                    0x22a1,
                    "Correct Fatal Errors",
                    &mut self.draft.correct_fatal_errors,
                );
                option(
                    ui,
                    catalog,
                    0x22a5,
                    "Convert Berry GFX Tile",
                    &mut self.draft.convert_berry_gfx,
                );
                ui.separator();
                option(
                    ui,
                    catalog,
                    0x22a9,
                    "Scan Exits on Save to ROM",
                    &mut self.draft.scan_exits,
                );
                option(
                    ui,
                    catalog,
                    0x22aa,
                    "Count Sprites on Save to ROM",
                    &mut self.draft.count_sprites,
                );
                option(
                    ui,
                    catalog,
                    0x22ab,
                    "Check Object Placement on Save to ROM",
                    &mut self.draft.check_object_placement,
                );
                option(
                    ui,
                    catalog,
                    0x22ac,
                    "Check if ROMFileName.ips Exists",
                    &mut self.draft.warn_ips_sibling,
                );
                option(
                    ui,
                    catalog,
                    0x22ad,
                    "Check if Vertical Fireball has Buoyancy",
                    &mut self.draft.warn_vertical_fireball,
                );
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

fn option(
    ui: &mut egui::Ui,
    catalog: Option<&LocalizationCatalog>,
    control_id: u32,
    fallback: &str,
    value: &mut bool,
) {
    let label = catalog
        .and_then(|catalog| catalog.original_dialog_control_text(ORIGINAL_DIALOG_ID, control_id))
        .unwrap_or(fallback);
    ui.checkbox(value, label);
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

    #[test]
    fn general_options_snapshot_retains_every_staged_field() {
        let options = GeneralOptions {
            undo_limit: 17,
            mouse_gestures: false,
            save_mouse_gestures: true,
            maintain_checksum: false,
            silently_add_header: false,
            save_prompt: false,
            joined_graphics: true,
            gfx_bypass_lists: false,
            prefer_past_2mb: false,
            remember_window_size: false,
            allow_control_wheel_zoom: false,
            rom_file_name_in_title: false,
            show_add_editor_ids: false,
            auto_deselect: true,
            correct_fatal_errors: false,
            convert_berry_gfx: false,
            scan_exits: false,
            count_sprites: false,
            check_object_placement: false,
            warn_ips_sibling: false,
            warn_vertical_fireball: false,
        };
        let mut dialog = UndoHistorySettings::default();
        dialog.open(options);
        assert!(dialog.is_open());
        assert_eq!(dialog.draft(), options);
    }
}
