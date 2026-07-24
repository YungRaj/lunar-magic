mod workspace;

use crate::rom_ownership::RomOwnershipLoader;
use eframe::egui;
use lm_app::{AppState, Command};
use workspace::RatsReclamationWorkspace;

#[derive(Default)]
pub(crate) struct RatsReclamationDialog {
    loader: RomOwnershipLoader,
    workspace: Option<RatsReclamationWorkspace>,
    error: Option<String>,
}

impl RatsReclamationDialog {
    pub(crate) fn is_busy(&self) -> bool {
        self.loader.is_running() || self.workspace.is_some()
    }

    pub(crate) fn choose_and_start(&mut self, app: &AppState) -> Result<bool, String> {
        if self.is_busy() {
            return Err("a RATS reclamation workflow is already active".into());
        }
        self.loader.choose_and_start(app.project_revision())
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        if let Some(result) = self.loader.show(context, app.project_revision()) {
            match result.and_then(|manifest| RatsReclamationWorkspace::load(app, manifest)) {
                Ok(workspace) => {
                    self.workspace = Some(workspace);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new("Reclaim Owned RATS Blocks")
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    command = self.contents(ui, app.project_revision());
                });
        }
        self.show_error(context);
        command
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_mut()?;
        let stale = workspace.is_stale(project_revision);
        let mut cancel = false;
        ui.label("Only blocks explicitly owned and not retained by the manifest will be erased.");
        ui.monospace(format!(
            "reclaim={} blocks / {} bytes    retain={} blocks",
            workspace.reclaimed_blocks, workspace.reclaimed_bytes, workspace.retained_blocks
        ));
        ui.horizontal(|ui| {
            ui.label("Erase fill byte");
            ui.text_edit_singleline(&mut workspace.fill);
        });
        ui.label(
            "The manifest is revalidated against the current ROM. Erasure and checksum repair \
             commit as one undoable project operation.",
        );
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed after this manifest was loaded.",
            );
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !stale && workspace.reclaimed_blocks != 0,
                    egui::Button::new("Reclaim transactionally"),
                )
                .clicked()
            {
                match workspace.prepare(project_revision) {
                    Ok(prepared) => command = Some(prepared),
                    Err(error) => self.error = Some(error),
                }
            }
        });
        if cancel {
            self.workspace = None;
        }
        command
    }

    fn show_error(&mut self, context: &egui::Context) {
        if let Some(error) = self.error.clone() {
            egui::Window::new("RATS reclamation error").show(context, |ui| {
                ui.label(error);
                if ui.button("OK").clicked() {
                    self.error = None;
                }
            });
        }
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.workspace = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_acknowledgement_clears_preview_state() {
        let mut dialog = RatsReclamationDialog::default();
        assert!(!dialog.is_busy());
        dialog.commit_succeeded();
        assert!(!dialog.is_busy());
    }
}
