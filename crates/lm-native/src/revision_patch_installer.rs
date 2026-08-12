mod workspace;

use crate::{
    dialogs,
    document_loader::{BoundedRead, DocumentLoader, LoadedDocument},
};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog, RevisionProfile};
use lm_profile::RevisionPatchTemplate;
use workspace::RevisionPatchWorkspace;

struct PendingTemplate {
    revision: u64,
    profile: RevisionProfile,
}

#[derive(Default)]
pub(crate) struct RevisionPatchInstaller {
    loader: DocumentLoader,
    pending: Option<PendingTemplate>,
    workspace: Option<RevisionPatchWorkspace>,
    error: Option<String>,
}

impl RevisionPatchInstaller {
    pub(crate) fn is_busy(&self) -> bool {
        self.loader.is_running() || self.workspace.is_some()
    }

    pub(crate) fn choose_and_start(&mut self, app: &AppState) -> Result<bool, String> {
        if self.is_busy() {
            return Err("a revision-patch installation is already active".into());
        }
        let profile = app
            .revision_profile()
            .ok_or_else(|| "install and audit a matching revision profile first".to_owned())?
            .clone();
        let revision = app.project_revision();
        let Some(path) = dialogs::choose_revision_patch() else {
            return Ok(false);
        };
        self.loader.start(vec![BoundedRead::new(
            path,
            RevisionPatchTemplate::MAX_FILE_LEN as u64,
            "revision patch template",
        )])?;
        self.pending = Some(PendingTemplate { revision, profile });
        Ok(true)
    }

    pub(crate) fn show(
        &mut self,
        context: &egui::Context,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        self.poll_loader(context);
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::RevisionPatchTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    command = self.contents(ui, project_revision, catalog)
                });
        }
        self.show_error(context, catalog);
        command
    }

    fn poll_loader(&mut self, context: &egui::Context) {
        let Some(result) = self.loader.show(context) else {
            return;
        };
        let Some(pending) = self.pending.take() else {
            self.error = Some("revision-patch loader lost its request context".into());
            return;
        };
        let loaded = result.and_then(decode_template).and_then(|template| {
            RevisionPatchWorkspace::new(pending.revision, &pending.profile, template)
        });
        match loaded {
            Ok(workspace) => self.workspace = Some(workspace),
            Err(error) => self.error = Some(error),
        }
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
        ui.heading(&workspace.template.name);
        ui.label(
            text(catalog, ExtendedUiTextKey::RevisionPatchIdentityFormat)
                .replace("{game}", &format!("{:?}", workspace.template.game))
                .replace("{region}", &format!("{:?}", workspace.template.region))
                .replace("{revision}", &workspace.template.revision.to_string())
                .replace("{mapper}", &format!("{:?}", workspace.template.mapper)),
        );
        ui.label(
            text(
                catalog,
                ExtendedUiTextKey::RevisionPatchPayloadSummaryFormat,
            )
            .replace("{payloads}", &workspace.template.payloads.len().to_string())
            .replace("{writes}", &workspace.template.writes.len().to_string()),
        );
        ui.label(text(catalog, ExtendedUiTextKey::RevisionPatchRangeNotice));
        egui::Grid::new("revision-patch-install-fields").show(ui, |ui| {
            ui.label(text(catalog, ExtendedUiTextKey::RevisionPatchSearchStart));
            ui.text_edit_singleline(&mut workspace.search_start);
            ui.end_row();
            ui.label(text(catalog, ExtendedUiTextKey::RevisionPatchSearchEnd));
            ui.text_edit_singleline(&mut workspace.search_end);
            ui.end_row();
            ui.label(text(catalog, ExtendedUiTextKey::RevisionPatchExpansionFill));
            ui.text_edit_singleline(&mut workspace.fill);
            ui.end_row();
        });
        ui.label(text(catalog, ExtendedUiTextKey::RevisionPatchAtomicNotice));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::RevisionPatchStaleNotice),
            );
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::RevisionPatchCancel))
                .clicked()
            {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !stale,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::RevisionPatchInstall)),
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::RevisionPatchErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::RevisionPatchOk))
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

fn decode_template(loaded: LoadedDocument) -> Result<RevisionPatchTemplate, String> {
    let [(_, bytes)] = loaded.into_exact::<1>("revision-patch")?;
    RevisionPatchTemplate::decode(&bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{PatchPayload, PatchWrite};
    use std::path::PathBuf;

    #[test]
    fn complete_revision_patch_form_uses_every_typed_key() {
        let source = include_str!("revision_patch_installer.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("RevisionPatch"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for hard_coded_caption in [
            "Window::new(\"Install Revision Patch\")",
            "ui.label(\"Search start\")",
            "Button::new(\"Install transactionally\")",
            "Window::new(\"Revision patch installation error\")",
        ] {
            assert!(!source.contains(hard_coded_caption));
        }
    }

    fn loaded(bytes: Vec<u8>) -> LoadedDocument {
        LoadedDocument {
            files: vec![(PathBuf::from("fixture.lmpatch"), bytes)],
        }
    }

    fn encoded_template() -> Vec<u8> {
        let profile = lm_profile::test_support::profile();
        RevisionPatchTemplate {
            name: "native loader fixture".into(),
            game: profile.game,
            region: profile.region,
            revision: profile.revision,
            mapper: profile.mapper,
            payloads: vec![PatchPayload {
                bytes: vec![1],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 1,
                expected: vec![2],
                replacement: vec![3],
                fixups: Vec::new(),
            }],
        }
        .encode()
        .unwrap()
    }

    #[test]
    fn bounded_loader_decoder_accepts_only_one_canonical_template() {
        assert_eq!(
            decode_template(loaded(encoded_template())).unwrap().name,
            "native loader fixture"
        );
        assert!(decode_template(loaded(vec![0; 8])).is_err());
        assert!(
            decode_template(LoadedDocument {
                files: vec![
                    (PathBuf::from("one"), encoded_template()),
                    (PathBuf::from("two"), encoded_template()),
                ],
            })
            .is_err()
        );
    }

    #[test]
    fn acknowledgement_is_the_only_success_path_that_closes_workspace() {
        let profile = lm_profile::test_support::profile();
        let template = RevisionPatchTemplate::decode(&encoded_template()).unwrap();
        let mut installer = RevisionPatchInstaller {
            workspace: Some(RevisionPatchWorkspace::new(4, &profile, template).unwrap()),
            ..RevisionPatchInstaller::default()
        };
        assert!(installer.is_busy());
        installer.commit_succeeded();
        assert!(!installer.is_busy());
    }
}
