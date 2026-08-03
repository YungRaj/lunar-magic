use crate::expanded_settings_editor_form::ExpandedSettingsForm;
use eframe::egui;
use lm_app::{AppState, Command, ExpandedSettingsController, RevisionProfileControllers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

#[derive(Default)]
pub(crate) struct RomExpandedSettingsEditor {
    controller: Option<ExpandedSettingsController>,
    form: ExpandedSettingsForm,
    error: Option<String>,
    pending_close: Option<PendingClose>,
}

impl RomExpandedSettingsEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.controller.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        let result = app
            .profiled_controller_snapshot()
            .map_err(|e| e.to_string())
            .and_then(|profiled| {
                profiled
                    .profile
                    .decode_expanded_settings(&profiled.snapshot)
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(controller) => {
                self.form = ExpandedSettingsForm::load(controller.record());
                self.controller = Some(controller);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
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
            PendingClose::Editor
        });
        false
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        let mut command = None;
        if self.controller.is_some() {
            egui::Window::new("ROM Expanded Settings")
                .default_size([470.0, 580.0])
                .show(context, |ui| {
                    command = self.contents(ui, project_revision);
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let controller = self.controller.as_ref()?;
        let stale = controller.revision() != project_revision;
        ui.label("Exact installed 32-byte record; unknown words remain lossless.");
        if stale {
            ui.colored_label(egui::Color32::YELLOW, "The ROM changed after this editor was opened. Close and reopen it before committing.");
        }
        ui.heading("Custom Layer 3 tilemap graphics");
        ui.checkbox(
            &mut self.form.layer3_enabled,
            "Enable custom Layer 3 tilemap",
        );
        ui.horizontal(|ui| {
            ui.label("GFX/ExGFX file");
            ui.text_edit_singleline(&mut self.form.layer3_file);
        });
        ui.add(
            egui::Slider::new(&mut self.form.layer3_length_selector, 0..=3).text("Length selector"),
        );
        ui.add(
            egui::Slider::new(&mut self.form.layer3_offset_selector, 0..=3)
                .text("Destination selector"),
        );
        if ui
            .add_enabled(!stale, egui::Button::new("Stage Layer 3 settings"))
            .clicked()
        {
            match self.form.layer3_edits() {
                Ok(edits) => {
                    if let Some(controller) = self.controller.as_mut() {
                        if let Err(error) = controller.apply_word_edits(&edits) {
                            self.error = Some(error.to_string());
                        } else {
                            self.form = ExpandedSettingsForm::load(controller.record());
                        }
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        ui.label("All sixteen exact native words");
        egui::Grid::new("rom-expanded-settings-words")
            .striped(true)
            .show(ui, |ui| {
                for (index, word) in self.form.words.iter_mut().enumerate() {
                    ui.label(format!("Word {index:X}"));
                    ui.text_edit_singleline(word);
                    ui.end_row();
                }
            });
        let mut result = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale, egui::Button::new("Stage all words"))
                .clicked()
            {
                match self.form.edits() {
                    Ok(edits) => {
                        if let Some(controller) = self.controller.as_mut() {
                            if let Err(error) = controller.apply_word_edits(&edits) {
                                self.error = Some(error.to_string());
                            } else {
                                self.form = ExpandedSettingsForm::load(controller.record());
                            }
                        } else {
                            self.error = Some("expanded-settings workspace is closed".into());
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            let modified = self
                .controller
                .as_ref()
                .is_some_and(ExpandedSettingsController::is_modified);
            if ui
                .add_enabled(modified && !stale, egui::Button::new("Commit to ROM"))
                .clicked()
            {
                if let Some(controller) = self.controller.as_ref() {
                    match controller.prepare_commit("Edit installed expanded settings") {
                        Ok(prepared) => result = Some(prepared.into_command()),
                        Err(error) => self.error = Some(error.to_string()),
                    }
                } else {
                    self.error = Some("expanded-settings workspace is closed".into());
                }
            }
            ui.label(if modified { "Staged" } else { "Unchanged" });
        });
        result
    }

    fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged ROM settings?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("These staged settings have not been committed to the ROM.");
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                    if ui.button("Discard").clicked() {
                        self.clear();
                        approved = pending == PendingClose::Application;
                    }
                });
            });
        approved
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM expanded-settings error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.controller = None;
        self.pending_close = None;
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}
