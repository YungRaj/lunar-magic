mod workspace;

use crate::level_editor_forms;
use eframe::egui;
use lm_app::{AppState, Command, ExtendedUiTextKey, LocalizationCatalog};
use lm_rom::CopierHeader;
use workspace::CopierHeaderWorkspace;

#[derive(Default)]
pub(crate) struct CopierHeaderDialog {
    workspace: Option<CopierHeaderWorkspace>,
    fill: String,
    error: Option<String>,
}

impl CopierHeaderDialog {
    pub(crate) fn open(&mut self, app: &AppState) {
        match CopierHeaderWorkspace::load(app) {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                self.fill = "00".into();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub(crate) fn show(&mut self, context: &egui::Context, app: &AppState) -> Option<Command> {
        let catalog = app.localization();
        let mut command = None;
        if let Some(workspace) = self.workspace.as_mut() {
            let mut open = true;
            let mut cancel = false;
            egui::Window::new(text(catalog, ExtendedUiTextKey::CopierHeaderTitle))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::CopierHeaderLogicalRomFormat,
                        "{length}",
                        &format!("{:#X}", workspace.logical_len()),
                    ));
                    ui.label(format_text(
                        catalog,
                        ExtendedUiTextKey::CopierHeaderCurrentStateFormat,
                        "{state}",
                        &header_name(catalog, workspace.current()),
                    ));
                    ui.horizontal(|ui| {
                        ui.label(text(catalog, ExtendedUiTextKey::CopierHeaderTarget));
                        ui.selectable_value(
                            workspace.target_mut(),
                            CopierHeader::Absent,
                            text(catalog, ExtendedUiTextKey::CopierHeaderAbsent),
                        );
                        ui.selectable_value(
                            workspace.target_mut(),
                            CopierHeader::Present,
                            text(catalog, ExtendedUiTextKey::CopierHeaderPresent),
                        );
                    });
                    ui.add_enabled_ui(workspace.target() == CopierHeader::Present, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(text(catalog, ExtendedUiTextKey::CopierHeaderFillByte));
                            ui.text_edit_singleline(&mut self.fill);
                        });
                    });
                    ui.label(text(
                        catalog,
                        ExtendedUiTextKey::CopierHeaderPreservationNotice,
                    ));
                    ui.add_enabled_ui(!workspace.canonical_lunar_magic(), |ui| {
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::CopierHeaderUseCanonical))
                            .clicked()
                        {
                            match workspace.prepare_lunar_magic_canonical(app.project_revision()) {
                                Ok(value) => command = Some(value),
                                Err(error) => self.error = Some(error),
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::CopierHeaderCancel))
                            .clicked()
                        {
                            cancel = true;
                        }
                        if ui
                            .button(text(catalog, ExtendedUiTextKey::CopierHeaderConvert))
                            .clicked()
                        {
                            let fill = if workspace.target() == CopierHeader::Present {
                                level_editor_forms::parse_hex_u8(
                                    &self.fill,
                                    "copier-header fill byte",
                                )
                            } else {
                                Ok(0)
                            };
                            match fill
                                .and_then(|fill| workspace.prepare(app.project_revision(), fill))
                            {
                                Ok(value) => command = Some(value),
                                Err(error) => self.error = Some(error),
                            }
                        }
                    });
                });
            if !open || cancel {
                self.workspace = None;
            }
        }
        if let Some(error) = self.error.clone() {
            egui::Window::new(text(catalog, ExtendedUiTextKey::CopierHeaderErrorTitle)).show(
                context,
                |ui| {
                    ui.label(error);
                    if ui
                        .button(text(catalog, ExtendedUiTextKey::CopierHeaderOk))
                        .clicked()
                    {
                        self.error = None;
                    }
                },
            );
        }
        command
    }

    pub(crate) fn commit_succeeded(&mut self) {
        self.workspace = None;
    }
}

fn text(catalog: Option<&LocalizationCatalog>, key: ExtendedUiTextKey) -> String {
    crate::frontend_ui::extended_localized_text(catalog, key)
}

fn format_text(
    catalog: Option<&LocalizationCatalog>,
    key: ExtendedUiTextKey,
    placeholder: &str,
    value: &str,
) -> String {
    text(catalog, key).replace(placeholder, value)
}

fn header_name(catalog: Option<&LocalizationCatalog>, header: CopierHeader) -> String {
    match header {
        CopierHeader::Absent => text(catalog, ExtendedUiTextKey::CopierHeaderAbsent),
        CopierHeader::Present => text(catalog, ExtendedUiTextKey::CopierHeaderPresent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn complete_copier_header_form_uses_every_typed_key() {
        let source = include_str!("copier_header_dialog.rs");
        for key in ExtendedUiTextKey::ALL
            .into_iter()
            .filter(|key| format!("{key:?}").starts_with("CopierHeader"))
        {
            assert!(source.contains(&format!("ExtendedUiTextKey::{key:?}")));
        }
        for hard_coded_caption in [
            "Window::new(\"Convert Copier Header\")",
            "ui.label(\"Target\")",
            "ui.button(\"Convert transactionally\")",
            "Window::new(\"Copier-header conversion error\")",
        ] {
            assert!(!source.contains(hard_coded_caption));
        }
    }

    fn app() -> AppState {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut app = AppState::default();
        app.load_rom(crate::test_support::pristine_smw_us_rom_bytes())
            .unwrap();
        app
    }

    #[test]
    fn workspace_routes_inverse_state_and_rejects_stale_revision() {
        let mut app = app();
        let workspace = CopierHeaderWorkspace::load(&app).unwrap();
        assert_eq!(workspace.current(), CopierHeader::Absent);
        assert_eq!(workspace.target(), CopierHeader::Present);
        let command = workspace.prepare(0, 0x7e).unwrap();
        app.dispatch(command).unwrap();
        assert!(workspace.prepare(1, 0).is_err());
    }

    #[test]
    fn workspace_routes_the_canonical_lunar_magic_header_and_then_rejects_a_no_op() {
        let mut app = app();
        let workspace = CopierHeaderWorkspace::load(&app).unwrap();
        assert!(!workspace.canonical_lunar_magic());
        app.dispatch(workspace.prepare_lunar_magic_canonical(0).unwrap())
            .unwrap();
        let reopened = CopierHeaderWorkspace::load(&app).unwrap();
        assert!(reopened.canonical_lunar_magic());
        assert!(reopened.prepare_lunar_magic_canonical(1).is_err());
    }

    #[test]
    fn no_op_target_never_prepares_a_command() {
        let app = app();
        let mut workspace = CopierHeaderWorkspace::load(&app).unwrap();
        *workspace.target_mut() = CopierHeader::Absent;
        assert!(workspace.prepare(0, 0).is_err());
    }

    #[test]
    fn dialog_closes_only_after_commit_acknowledgement() {
        let app = app();
        let mut dialog = CopierHeaderDialog::default();
        dialog.open(&app);
        assert!(dialog.workspace.is_some());
        dialog.commit_succeeded();
        assert!(dialog.workspace.is_none());
    }
}
