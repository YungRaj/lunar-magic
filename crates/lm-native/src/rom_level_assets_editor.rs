use crate::{document_loader::DocumentLoader, native_level_assets_panels::AggregatePanels};
use eframe::egui;
use lm_app::{
    AppState, Command, NativeLevelAssetsController, ProfiledControllerSnapshot, RevisionProfile,
};
use lm_graphics::PaletteOwnership;
use lm_project::NativeLevelAssetsFile;

mod commit;
mod lifecycle;
mod mwl;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingClose {
    Editor,
    Application,
}

struct Workspace {
    controller: NativeLevelAssetsController,
    profile: RevisionProfile,
    source_slot: u16,
    image: lm_rom::RomImage,
    internal_header: usize,
    ownership: PaletteOwnership,
}

struct PendingLoad {
    profiled: ProfiledControllerSnapshot,
}

#[derive(Default)]
pub(crate) struct RomLevelAssetsEditor {
    workspace: Option<Workspace>,
    panels: AggregatePanels,
    search_start: String,
    search_end: String,
    error: Option<String>,
    pending_close: Option<PendingClose>,
    loader: DocumentLoader,
    mwl_loader: DocumentLoader,
    pending_load: Option<PendingLoad>,
    manifest_loader: crate::rom_ownership::RomOwnershipLoader,
}

impl RomLevelAssetsEditor {
    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
    ) -> (bool, Option<Command>) {
        if let Some(result) = self.loader.show(context) {
            self.finish_ownership_load(result, project_revision);
        }
        let mut command = self.mwl_loader.show(context).and_then(|result| {
            match self.finish_mwl_import(result, project_revision) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            }
        });
        let reclamation_command = match self.manifest_loader.show(context, project_revision) {
            Some(Ok(manifest)) => match self.prepare_commit_with_reclamation(&manifest) {
                Ok(command) => Some(command),
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            },
            Some(Err(error)) => {
                self.error = Some(error);
                None
            }
            None => None,
        };
        if reclamation_command.is_some() {
            command = reclamation_command;
        }
        if self.workspace.is_some() {
            egui::Window::new("ROM Native Level Assets")
                .default_size([900.0, 720.0])
                .vscroll(true)
                .show(context, |ui| {
                    if let Some(ui_command) = self.contents(ui, project_revision) {
                        command = Some(ui_command);
                    }
                });
        }
        let approved = self.close_confirmation(context);
        self.show_error(context);
        (approved, command)
    }

    fn contents(&mut self, ui: &mut egui::Ui, project_revision: u64) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.controller.revision() != project_revision;
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                "The ROM changed. Close and reopen this workspace before committing.",
            );
        }
        ui.horizontal(|ui| {
            ui.label("Allocation search (logical PC hex, end-exclusive)");
            ui.text_edit_singleline(&mut self.search_start);
            ui.label("..");
            ui.text_edit_singleline(&mut self.search_end);
        });
        let file = NativeLevelAssetsFile {
            source_slot: workspace.source_slot,
            assets: workspace.controller.assets().clone(),
        };
        let edit = self.panels.show(
            ui,
            workspace.controller.revision(),
            &file,
            (
                workspace.controller.layer2(),
                workspace.controller.layer2_descriptor(),
            ),
            &workspace.profile.exanimation_double_size_modes,
            &workspace.ownership,
        );
        if let Some(edit) = edit {
            match edit {
                Ok(edit) if !stale => {
                    if let Some(workspace) = self.workspace.as_mut() {
                        if let Err(error) = workspace.controller.apply_edits(&[edit]) {
                            self.error = Some(error.to_string());
                        } else {
                            self.panels.invalidate();
                        }
                    } else {
                        self.error = Some("level-assets workspace is closed".into());
                    }
                }
                Ok(_) => self.error = Some("stale ROM workspace cannot accept more edits".into()),
                Err(error) => self.error = Some(error),
            }
        }
        ui.separator();
        let modified = self
            .workspace
            .as_ref()
            .is_some_and(|w| w.controller.is_modified());
        self.show_mwl_actions(ui, stale, modified);
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit all domains to ROM"),
            )
            .clicked()
        {
            match self.prepare_commit() {
                Ok(command) => {
                    return Some(command);
                }
                Err(error) => self.error = Some(error),
            }
        }
        if ui
            .add_enabled(
                modified && !stale && !self.manifest_loader.is_running(),
                egui::Button::new("Commit and reclaim with LMRATS01 evidence"),
            )
            .clicked()
        {
            if let Err(error) = self.manifest_loader.choose_and_start(project_revision) {
                self.error = Some(error);
            }
        }
        ui.label(if modified {
            "Staged aggregate changes"
        } else {
            "No staged changes"
        });
        None
    }
}
