mod workspace;

use crate::document_loader::{BoundedRead, DocumentLoader, LoadedDocument};
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog};
use workspace::IpsPatchWorkspace;

#[derive(Default)]
pub(crate) struct IpsPatchDialog {
    loader: DocumentLoader,
    load_revision: Option<u64>,
    workspace: Option<IpsPatchWorkspace>,
    error: Option<String>,
}

impl IpsPatchDialog {
    pub(crate) fn is_busy(&self) -> bool {
        self.loader.is_running() || self.workspace.is_some()
    }

    pub(crate) fn choose_and_start(&mut self, app: &AppState) -> Result<bool, String> {
        if self.is_busy() {
            return Err("an IPS patch workflow is already active".into());
        }
        let Some(path) = crate::dialogs::choose_ips_patch() else {
            return Ok(false);
        };
        self.loader.start(vec![BoundedRead::new(
            path,
            lm_rom::MAX_IPS_PATCH_LEN as u64,
            "IPS patch",
        )])?;
        self.load_revision = Some(app.project_revision());
        Ok(true)
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let catalog = app.localization();
        self.poll_loader(context, app);
        let mut command = None;
        if self.workspace.is_some() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::IpsApplyTitle))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    command = self.contents(ui, app.project_revision(), catalog);
                });
        }
        self.show_error(context, catalog);
        command
    }

    fn poll_loader(&mut self, context: &egui::Context, app: &AppState) {
        let Some(result) = self.loader.show(context) else {
            return;
        };
        let Some(revision) = self.load_revision.take() else {
            self.error = Some("IPS loader lost its project revision".into());
            return;
        };
        let loaded = result.and_then(decode_patch).and_then(|patch| {
            if revision != app.project_revision() {
                return Err("the ROM changed while the IPS patch was loading".into());
            }
            IpsPatchWorkspace::load(app, patch)
        });
        match loaded {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        project_revision: u64,
        catalog: Option<&LocalizationCatalog>,
    ) -> Option<Command> {
        let workspace = self.workspace.as_ref()?;
        let stale = workspace.is_stale(project_revision);
        let mut cancel = false;
        ui.label(text(catalog, ExtendedUiTextKey::IpsApplyHeaderNotice));
        ui.monospace(
            text(catalog, ExtendedUiTextKey::IpsApplySummaryFormat)
                .replace("{source}", &workspace.source_len.to_string())
                .replace("{target}", &workspace.target_len.to_string())
                .replace("{changed}", &workspace.changed_bytes.to_string()),
        );
        ui.label(text(catalog, ExtendedUiTextKey::IpsApplyIdentityNotice));
        if stale {
            ui.colored_label(
                egui::Color32::YELLOW,
                text(catalog, ExtendedUiTextKey::IpsApplyStaleNotice),
            );
        }
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(text(catalog, ExtendedUiTextKey::IpsApplyCancel))
                .clicked()
            {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !stale && workspace.changed_bytes != 0,
                    egui::Button::new(text(catalog, ExtendedUiTextKey::IpsApplyAction)),
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
            egui::Window::new(text(catalog, ExtendedUiTextKey::IpsApplyErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::IpsApplyOk))
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

fn decode_patch(loaded: LoadedDocument) -> Result<Vec<u8>, String> {
    let [(_, bytes)] = loaded.into_exact::<1>("IPS patch")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn complete_ips_apply_form_uses_every_typed_key() {
        let source = include_str!("ips_patch_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("IpsApply"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for hard_coded_caption in [
            "Window::new(\"Apply IPS Patch\")",
            "ui.button(\"Cancel\")",
            "Button::new(\"Apply transactionally\")",
            "Window::new(\"IPS patch error\")",
        ] {
            assert!(!source.contains(hard_coded_caption));
        }
    }

    #[test]
    fn exact_loader_group_is_required() {
        assert_eq!(
            decode_patch(LoadedDocument {
                files: vec![(PathBuf::from("test.ips"), b"PATCHEOF".to_vec())],
            })
            .unwrap(),
            b"PATCHEOF"
        );
        assert!(decode_patch(LoadedDocument { files: Vec::new() }).is_err());
    }
}
