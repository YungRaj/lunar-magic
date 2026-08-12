mod workspace;

use crate::rom_ownership::RomOwnershipLoader;
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog};
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
        let catalog = app.localization();
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::RatsReclaimTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    command = self.contents(ui, app.project_revision(), catalog);
                });
        }
        self.show_error(context, catalog);
        command
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.workspace.as_mut()?;
        let stale = workspace.is_stale(project_revision);
        let mut cancel = false;
        ui.label(text(catalog, ExtendedUiTextKey::RatsReclaimOwnershipNotice));
        ui.monospace(
            text(catalog, ExtendedUiTextKey::RatsReclaimSummaryFormat)
                .replace("{blocks}", &workspace.reclaimed_blocks.to_string())
                .replace("{bytes}", &workspace.reclaimed_bytes.to_string())
                .replace("{retained}", &workspace.retained_blocks.to_string()),
        );
        ui.horizontal(|ui| {
            ui.label(text(catalog, ExtendedUiTextKey::RatsReclaimFillByte));
            ui.text_edit_singleline(&mut workspace.fill);
        });
        ui.label(text(
            catalog,
            ExtendedUiTextKey::RatsReclaimTransactionNotice,
        ));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::RatsReclaimStaleNotice),
            );
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::RatsReclaimCancel))
                .clicked()
            {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !stale && workspace.reclaimed_blocks != 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RatsReclaimAction)),
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

    fn show_error(&mut self, context: &egui::Context, catalog: Option<&LocalizationCatalog>) {
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::RatsReclaimErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RatsReclaimOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
        }
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.workspace = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_rats_reclamation_form_uses_every_typed_key() {
        let source = include_str!("rats_reclamation_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("RatsReclaim"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for hard_coded_caption in [
            "Window::new(\"Reclaim Owned RATS Blocks\")",
            "ui.label(\"Erase fill byte\")",
            "Button::new(\"Reclaim transactionally\")",
            "Window::new(\"RATS reclamation error\")",
        ] {
            assert!(!source.contains(hard_coded_caption));
        }
    }

    #[test]
    fn successful_acknowledgement_clears_preview_state() {
        let mut dialog = RatsReclamationDialog::default();
        assert!(!dialog.is_busy());
        dialog.commit_succeeded();
        assert!(!dialog.is_busy());
    }
}
