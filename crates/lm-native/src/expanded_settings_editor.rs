use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader},
    document_persistence::DocumentPersistence,
    expanded_settings_editor_form::ExpandedSettingsForm,
};
use eframe::egui;
use lm_app::{ExpandedSettingsDocumentController, ExtendedUiTextKey, LocalizationCatalog};
use lm_level::ExpandedLevelSettingsRecord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Document,
    Application,
}

#[derive(Default)]
pub(crate) struct ExpandedSettingsEditor {
    controller: Option<ExpandedSettingsDocumentController>,
    form: ExpandedSettingsForm,
    loaded_revision: Option<u64>,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    persistence: DocumentPersistence,
    loader: DocumentLoader,
}

impl ExpandedSettingsEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some() || self.loader.is_running()
    }

    pub(crate) fn open(&mut self) {
        if self.is_open() {
            return;
        }
        let Some(path) = dialogs::choose_expanded_settings_document() else {
            return;
        };
        if let Err(error) = self.loader.start(vec![BoundedRead::new(
            path,
            u64::try_from(ExpandedLevelSettingsRecord::ENCODED_LEN).unwrap_or(u64::MAX),
            "expanded-settings document",
        )]) {
            self.error = Some(error);
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.loader.is_running() {
            self.error = Some("wait for settings loading to finish before closing".into());
            return false;
        }
        if self.persistence.is_running() {
            self.error = Some("wait for settings persistence to finish before closing".into());
            return false;
        }
        let Some(controller) = &self.controller else {
            return true;
        };
        if !controller.is_modified() {
            self.clear();
            return true;
        }
        self.pending_close = Some(if application {
            PendingClose::Application
        } else {
            PendingClose::Document
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        if let Some(result) = self.loader.show(context) {
            match result.and_then(|mut loaded| {
                let (path, bytes) = loaded
                    .files
                    .pop()
                    .ok_or_else(|| "settings loader returned no file".to_string())?;
                ExpandedSettingsDocumentController::decode(path, &bytes)
                    .map_err(|error| error.to_string())
            }) {
                Ok(controller) => {
                    self.controller = Some(controller);
                    self.loaded_revision = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(controller) = self.controller.as_mut()
            && let Some(Err(error)) = self.persistence.show(context, controller)
        {
            self.error = Some(error);
        }
        if self.controller.is_some() {
            self.load_form();
            egui::Window::new(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsDocumentTitle,
            ))
            .default_size([460.0, 560.0])
            .show(context, |ui| self.contents(ui, catalog));
        }
        let approved = self.show_close_confirmation(context, catalog);
        self.show_error(context, catalog);
        approved
    }

    fn load_form(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        if self.loaded_revision != Some(controller.revision()) {
            self.form = ExpandedSettingsForm::load(controller.value());
            self.loaded_revision = Some(controller.revision());
        }
    }

    fn contents(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        self.toolbar(ui, catalog);
        ui.separator();
        ui.label(text(
            catalog,
            ExtendedUiTextKey::ExpandedSettingsRecoveredNotice,
        ));
        ui.checkbox(
            &mut self.form.layer3_enabled,
            text(catalog, ExtendedUiTextKey::RomExpandedSettingsLayer3Enable),
        );
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::RomExpandedSettingsGfxFile));
            ui.text_edit_singleline(&mut self.form.layer3_file);
        });
        ui.add(
            egui::Slider::new(&mut self.form.layer3_length_selector, 0..=3).text(text(
                catalog,
                ExtendedUiTextKey::RomExpandedSettingsLengthSelector,
            )),
        );
        ui.add(
            egui::Slider::new(&mut self.form.layer3_offset_selector, 0..=3).text(text(
                catalog,
                ExtendedUiTextKey::RomExpandedSettingsDestinationSelector,
            )),
        );
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsApplyLayer3,
            ))
            .clicked()
        {
            match self.form.layer3_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        ui.horizontal(|ui| {
            ui.label(text(
                catalog,
                ExtendedUiTextKey::RomExpandedSettingsExpandedMode,
            ));
            ui.text_edit_singleline(&mut self.form.layer3_expanded_mode);
        });
        ui.small(text(
            catalog,
            ExtendedUiTextKey::RomExpandedSettingsExpandedModeNotice,
        ));
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsApplyExpandedMode,
            ))
            .clicked()
        {
            match self.form.layer3_expanded_mode_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.label(text(
            catalog,
            ExtendedUiTextKey::RomExpandedSettingsBypassHeading,
        ));
        ui.checkbox(
            &mut self.form.bypass_enabled,
            text(catalog, ExtendedUiTextKey::RomExpandedSettingsBypassEnable),
        );
        egui::Grid::new("expanded-settings-super-gfx")
            .num_columns(4)
            .show(ui, |ui| {
                for (slot, label) in ["FG1", "FG2", "FG3", "BG1", "BG2", "BG3"]
                    .into_iter()
                    .enumerate()
                {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.form.bypass_foreground_background[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
                for (slot, label) in ["SP1", "SP2", "SP3", "SP4"].into_iter().enumerate() {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(&mut self.form.bypass_sprites[slot])
                            .hexadecimal(3, false, true)
                            .range(0..=0x0fff),
                    );
                    if slot % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsApplyBypass,
            ))
            .clicked()
        {
            match self.form.super_graphics_bypass_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.label(text(
            catalog,
            ExtendedUiTextKey::RomExpandedSettingsBoundaryHeading,
        ));
        ui.checkbox(
            &mut self.form.sprites_beyond_boundaries_use_air,
            text(catalog, ExtendedUiTextKey::RomExpandedSettingsBoundaryAir),
        );
        if ui
            .button(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsApplyBoundary,
            ))
            .clicked()
        {
            match self.form.sprite_boundary_edits() {
                Ok(edits) => self.apply_edits(&edits),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.label(text(
            catalog,
            ExtendedUiTextKey::ExpandedSettingsWordsNotice,
        ));
        egui::Grid::new("expanded-settings-words")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (index, value) in self.form.words.iter_mut().enumerate() {
                    ui.label(
                        text(catalog, ExtendedUiTextKey::RomExpandedSettingsWord)
                            .replace("{index}", &format!("{index:X}")),
                    );
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
            });
        if ui
            .button(text(catalog, ExtendedUiTextKey::ExpandedSettingsApplyWords))
            .clicked()
        {
            match self.form.edits() {
                Ok(edits) => {
                    self.apply_edits(&edits);
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn apply_edits(&mut self, edits: &[(usize, u16)]) {
        let Some(controller) = self.controller.as_mut() else {
            return;
        };
        if let Err(error) = controller.apply_word_edits(controller.revision(), edits) {
            self.error = Some(error.to_string());
        } else {
            self.loaded_revision = None;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, catalog: Option<&LocalizationCatalog>) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        let (can_undo, can_redo, modified) = (
            controller.can_undo(),
            controller.can_redo(),
            controller.is_modified(),
        );
        let mut history = None;
        let mut save_requested = false;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::ExpandedSettingsUndo)),
                )
                .clicked()
            {
                history = Some(true);
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::ExpandedSettingsRedo)),
                )
                .clicked()
            {
                history = Some(false);
            }
            save_requested = ui
                .add_enabled(
                    !self.persistence.is_running(),
                    egui::Button::new(text(catalog, ExtendedUiTextKey::ExpandedSettingsSave)),
                )
                .clicked();
            ui.label(text(
                catalog,
                if modified {
                    ExtendedUiTextKey::ExpandedSettingsModified
                } else {
                    ExtendedUiTextKey::ExpandedSettingsSaved
                },
            ));
        });
        let mut changed = false;
        if let Some(controller) = self.controller.as_mut() {
            if let Some(undo) = history {
                let result = if undo {
                    controller.undo(controller.revision())
                } else {
                    controller.redo(controller.revision())
                };
                match result {
                    Ok(value) => changed = value,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if save_requested {
                if let Err(error) = self.persistence.begin(controller) {
                    self.error = Some(error);
                }
            }
        }
        if changed {
            self.loaded_revision = None;
        }
    }

    fn show_close_confirmation(
        &mut self,
        context: &egui::Context,
        catalog: Option<&LocalizationCatalog>,
    ) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new(text(
            catalog,
            ExtendedUiTextKey::ExpandedSettingsUnsavedTitle,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(context, |ui| {
            ui.label(text(
                catalog,
                ExtendedUiTextKey::ExpandedSettingsDiscardQuestion,
            ));
            ui.horizontal(|ui| {
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomExpandedSettingsCancel))
                    .clicked()
                {
                    self.pending_close = None;
                }
                if ui
                    .button(text(catalog, ExtendedUiTextKey::RomExpandedSettingsDiscard))
                    .clicked()
                {
                    self.clear();
                    approved = pending == PendingClose::Application;
                }
            });
        });
        approved
    }

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::ExpandedSettingsErrorTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RomExpandedSettingsOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.loaded_revision = None;
        self.pending_close = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{ExpandedLevelHeader, ExpandedLevelSettingsRecord, SuperGraphicsBypass};

    #[test]
    fn complete_document_expanded_settings_surface_uses_every_typed_key() {
        let source = include_str!("expanded_settings_editor.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("ExpandedSettings"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for bypass in [
            "egui::Window::new(\"Expanded Settings Editor\")",
            "ui.button(\"Apply Layer 3 settings\")",
            "egui::Button::new(\"Save\")",
            "egui::Window::new(\"Unsaved expanded settings\")",
        ] {
            assert!(!source.contains(bypass));
        }
    }

    #[test]
    fn standalone_semantic_controls_share_history_and_save_exact_state() {
        let source = ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap();
        let mut controller =
            ExpandedSettingsDocumentController::decode("settings.bin".into(), source.encoded())
                .unwrap();

        let mut form = ExpandedSettingsForm::load(controller.value());
        form.bypass_enabled = true;
        form.bypass_foreground_background = [1, 2, 3, 4, 5, 6];
        form.bypass_sprites = [0x101, 0x202, 0x303, 0x404];
        controller
            .apply_word_edits(
                controller.revision(),
                &form.super_graphics_bypass_edits().unwrap(),
            )
            .unwrap();

        form = ExpandedSettingsForm::load(controller.value());
        form.layer3_expanded_mode = "89AFCDEF".into();
        controller
            .apply_word_edits(
                controller.revision(),
                &form.layer3_expanded_mode_edits().unwrap(),
            )
            .unwrap();

        form = ExpandedSettingsForm::load(controller.value());
        form.sprites_beyond_boundaries_use_air = false;
        controller
            .apply_word_edits(
                controller.revision(),
                &form.sprite_boundary_edits().unwrap(),
            )
            .unwrap();

        let value = controller.value().clone();
        assert_eq!(
            ExpandedLevelHeader::from(&value).super_graphics_bypass(),
            SuperGraphicsBypass {
                enabled: true,
                foreground_background: [1, 2, 3, 4, 5, 6],
                sprites: [0x101, 0x202, 0x303, 0x404],
            }
        );
        assert_eq!(value.layer3_expanded_mode_flags().packed(), 0x89ab_cdef);
        assert!(!ExpandedLevelHeader::from(&value).sprites_beyond_boundaries_use_air());
        assert_eq!(controller.revision(), 3);

        let snapshot = controller.begin_save().unwrap();
        assert_eq!(
            ExpandedLevelSettingsRecord::decode(&snapshot.bytes).unwrap(),
            value
        );
        controller.acknowledge_save(snapshot.request_id).unwrap();
        assert!(!controller.is_modified());
        assert!(controller.undo(controller.revision()).unwrap());
        assert!(controller.is_modified());
    }
}
