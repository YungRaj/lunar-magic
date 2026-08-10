use super::workspace::{decode, decode_slot};
use super::{AppState, PendingClose, RomExAnimationEditor, egui};

impl RomExAnimationEditor {
    pub(crate) fn is_open(&self) -> bool {
        self.workspace.is_some()
    }

    pub(crate) fn open(&mut self, app: &AppState) {
        if self.is_open() {
            return;
        }
        match decode(app) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.selected_record = 0;
                self.search_start.clear();
                self.search_end.clear();
                self.invalidate();
                self.load();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn open_level(&mut self, app: &AppState, level: u16) {
        self.open_target(app, level, false);
    }

    pub(crate) fn open_global(&mut self, app: &AppState, level: u16) {
        self.open_target(app, level, true);
    }

    fn open_target(&mut self, app: &AppState, level: u16, global: bool) {
        if let Some(workspace) = self.workspace.as_mut() {
            if workspace.slot != level {
                self.error = Some(
                    "close the current ExAnimation workspace before opening another level".into(),
                );
                return;
            }
            if workspace.editing_global != global && !workspace.switch_target() {
                self.error = Some(
                    "commit or revert the current ExAnimation changes before switching domains"
                        .into(),
                );
                return;
            }
            self.selected_record = 0;
            self.invalidate();
            self.load();
            return;
        }
        match decode_slot(app, level) {
            Ok(mut workspace) => {
                if global && !workspace.switch_target() {
                    self.error = Some(
                        workspace
                            .global_unavailable
                            .clone()
                            .unwrap_or_else(|| "global ExAnimation is unavailable".into()),
                    );
                    return;
                }
                self.workspace = Some(workspace);
                self.selected_record = 0;
                self.search_start.clear();
                self.search_end.clear();
                self.invalidate();
                self.load();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn request_close(&mut self, application: bool) -> bool {
        if self.manifest_loader.is_running() {
            self.error = Some("wait for RATS ownership loading to finish before closing".into());
            return false;
        }
        let Some(workspace) = &self.workspace else {
            return true;
        };
        if !workspace.any_modified() {
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

    pub(super) fn close_confirmation(&mut self, context: &egui::Context) -> bool {
        let Some(pending) = self.pending_close else {
            return false;
        };
        let mut approved = false;
        egui::Window::new("Discard staged ExAnimation changes?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label("These changes have not been committed to the ROM.");
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

    pub(super) fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("ROM ExAnimation error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    fn clear(&mut self) {
        self.workspace = None;
        self.pending_close = None;
        self.paste_target = None;
        self.invalidate();
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.clear();
    }
}
